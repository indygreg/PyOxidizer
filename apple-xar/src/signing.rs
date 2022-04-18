// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! XAR signing.
//!
//! XAR files can be signed with both an RSA and a CMS signature. The principle is the same:
//! a cryptographic signature is made over the ToC checksum, which itself is a digest of the
//! content of the table of contents.
//!
//! There is some trickiness to avoid a circular dependency when signing. The signatures
//! themselves are stored at the beginning of the heap. This means the signature output
//! alters the offsets of file entries within the heap. Metadata about the signatures -
//! including their offset, size, and public certificates - are included in the table
//! of contents and are digested. So care must be taken to not alter the table of contents
//! after signature generation time.

use {
    crate::{
        format::XarChecksum,
        reader::XarReader,
        table_of_contents::{Checksum, ChecksumType, File, KeyInfo, Signature, SignatureStyle},
        Error, XarResult,
    },
    bcder::Oid,
    cryptographic_message_syntax::{asn1::rfc5652::OID_ID_DATA, SignedDataBuilder, SignerBuilder},
    flate2::{write::ZlibEncoder, Compression},
    log::{error, info, warn},
    rand::RngCore,
    scroll::IOwrite,
    std::{
        cmp::Ordering,
        collections::HashMap,
        fmt::Debug,
        io::{Read, Seek, Write},
    },
    url::Url,
    x509_certificate::{CapturedX509Certificate, KeyInfoSigner},
};

/// Entity for signing a XAR file.
pub struct XarSigner<R: Read + Seek + Sized + Debug> {
    reader: XarReader<R>,
    checksum_type: ChecksumType,
}

impl<R: Read + Seek + Sized + Debug> XarSigner<R> {
    /// Create a new instance bound to an existing XAR.
    pub fn new(reader: XarReader<R>) -> Self {
        let checksum_type = reader.table_of_contents().checksum.style;

        Self {
            reader,
            checksum_type,
        }
    }

    /// Sign a XAR file using signing parameters.
    ///
    /// The `signing_key` and `signing_cert` form the certificate to use for signing.
    /// `time_stamp_url` is an optional Time-Stamp Protocol server URL to use for the CMS
    /// signature.
    /// `certificates` is an iterable of X.509 certificates to attach to the signature.
    pub fn sign<W: Write>(
        &mut self,
        writer: &mut W,
        signing_key: &dyn KeyInfoSigner,
        signing_cert: &CapturedX509Certificate,
        time_stamp_url: Option<&Url>,
        certificates: impl Iterator<Item = CapturedX509Certificate>,
    ) -> XarResult<()> {
        let extra_certificates = certificates.collect::<Vec<_>>();

        // Base64 encoding of all public certificates.
        let chain = std::iter::once(signing_cert)
            .chain(extra_certificates.iter())
            .collect::<Vec<_>>();

        // Sending the same content to the Time-Stamp Server on every invocation might
        // raise suspicions. So randomize the input and thus the digest.
        let mut random = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut random);
        let empty_digest = self.checksum_type.digest_data(&random)?;
        let digest_size = empty_digest.len() as u64;

        info!("performing empty RSA signature to calculate signature length");
        let rsa_signature_len = signing_key.try_sign(&empty_digest)?.as_ref().len();

        info!("performing empty CMS signature to calculate data length");
        let signer =
            SignerBuilder::new(signing_key, signing_cert.clone()).message_id_content(empty_digest);

        let signer = if let Some(time_stamp_url) = time_stamp_url {
            info!("using time-stamp server {}", time_stamp_url);
            signer.time_stamp_url(time_stamp_url.clone())?
        } else {
            signer
        };

        let cms_signature_len = SignedDataBuilder::default()
            .content_type(Oid(OID_ID_DATA.as_ref().into()))
            .signer(signer.clone())
            .certificates(extra_certificates.iter().cloned())
            .build_der()?
            .len();

        // Pad it a little because CMS signatures are variable size.
        let cms_signature_len = cms_signature_len + 512;

        // Now build up a new table of contents to sign.
        let mut toc = self.reader.table_of_contents().clone();
        toc.checksum = Checksum {
            style: self.checksum_type,
            offset: 0,
            size: digest_size,
        };

        let rsa_signature = Signature {
            style: SignatureStyle::Rsa,
            // The RSA signature goes right after the digest data.
            offset: digest_size,
            size: rsa_signature_len as _,
            key_info: KeyInfo::from_certificates(chain.iter().copied())?,
        };

        let cms_signature = Signature {
            style: SignatureStyle::Cms,
            // The CMS signature goes right after the RSA signature.
            offset: rsa_signature.offset + rsa_signature.size,
            size: cms_signature_len as _,
            key_info: KeyInfo::from_certificates(chain.iter().copied())?,
        };

        let mut current_offset = cms_signature.offset + cms_signature.size;

        toc.signature = Some(rsa_signature);
        toc.x_signature = Some(cms_signature);

        // Now go through and update file offsets. Files are nested. So we do a pass up
        // front to calculate all the offsets then we recursively descend and update all
        // references.
        let mut ids_to_offsets = HashMap::new();

        for (_, file) in self.reader.files()? {
            if let Some(data) = &file.data {
                ids_to_offsets.insert(file.id, current_offset);
                current_offset += data.length;
            }
        }

        toc.visit_files_mut(&|file: &mut File| {
            if let Some(data) = &mut file.data {
                data.offset = *ids_to_offsets
                    .get(&file.id)
                    .expect("file should have offset recorded");
            }
        });

        // The TOC should be all set up now. Let's serialize it so we can produce
        // a valid signature.
        warn!("generating new XAR table of contents XML");
        let toc_data = toc.to_xml()?;
        info!("table of contents size: {}", toc_data.len());

        let mut zlib = ZlibEncoder::new(Vec::new(), Compression::default());
        zlib.write_all(&toc_data)?;
        let toc_compressed = zlib.finish()?;

        let toc_digest = self.checksum_type.digest_data(&toc_compressed)?;

        // Sign it for real.
        let rsa_signature = signing_key.try_sign(&toc_digest)?;

        let mut cms_signature = SignedDataBuilder::default()
            .content_type(Oid(OID_ID_DATA.as_ref().into()))
            .signer(signer.message_id_content(toc_digest.clone()))
            .certificates(extra_certificates.iter().cloned())
            .build_der()?;

        match cms_signature.len().cmp(&cms_signature_len) {
            Ordering::Greater => {
                error!("real CMS signature overflowed allocated space for signature (please report this bug)");
                return Err(Error::Unsupported("CMS signature overflow"));
            }
            Ordering::Equal => {}
            Ordering::Less => {
                cms_signature
                    .extend_from_slice(&b"\0".repeat(cms_signature_len - cms_signature.len()));
            }
        }

        // Now let's write everything out.
        let mut header = *self.reader.header();
        header.checksum_algorithm_id = XarChecksum::from(self.checksum_type).into();
        header.toc_length_compressed = toc_compressed.len() as _;
        header.toc_length_uncompressed = toc_data.len() as _;

        writer.iowrite_with(header, scroll::BE)?;
        writer.write_all(&toc_compressed)?;
        writer.write_all(&toc_digest)?;
        writer.write_all(rsa_signature.as_ref())?;
        writer.write_all(&cms_signature)?;

        // And write all the files to the heap.
        for (path, file) in self.reader.files()? {
            if file.data.is_some() {
                info!("copying {} to output XAR", path);
                self.reader.write_file_data_heap_from_file(&file, writer)?;
            }
        }

        Ok(())
    }
}
