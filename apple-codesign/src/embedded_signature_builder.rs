// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Provides primitives for constructing embeddable signature data structures.

use {
    crate::{
        code_directory::CodeDirectoryBlob,
        embedded_signature::{
            create_superblob, Blob, BlobData, BlobWrapperBlob, CodeSigningMagic, CodeSigningSlot,
            DigestType, EmbeddedSignature,
        },
        error::AppleCodesignError,
    },
    bcder::{encode::PrimitiveContent, Oid},
    bytes::Bytes,
    cryptographic_message_syntax::{asn1::rfc5652::OID_ID_DATA, SignedDataBuilder, SignerBuilder},
    log::{info, warn},
    reqwest::Url,
    std::collections::BTreeMap,
    x509_certificate::{rfc5652::AttributeValue, CapturedX509Certificate, DigestAlgorithm, Sign},
};

/// OID for signed attribute containing plist of code directory hashes.
///
/// 1.2.840.113635.100.9.1.
pub const CDHASH_PLIST_OID: bcder::ConstOid = Oid(&[42, 134, 72, 134, 247, 99, 100, 9, 1]);

/// OID for signed attribute containing the SHA-256 of code directory digests.
///
/// 1.2.840.113635.100.9.2
pub const CDHASH_SHA256_OID: bcder::ConstOid = Oid(&[42, 134, 72, 134, 247, 99, 100, 9, 2]);

/// Obtain the XML plist containing code directory hashes.
///
/// This plist is embedded as a signed attribute in the CMS signature.
pub fn create_code_directory_hashes_plist<'a>(
    code_directories: impl Iterator<Item = &'a CodeDirectoryBlob<'a>>,
    digest_type: DigestType,
) -> Result<Vec<u8>, AppleCodesignError> {
    let hashes = code_directories
        .map(|cd| {
            let mut digest = cd.digest_with(digest_type)?;

            // While we may use stronger digests, it appears that the XML in the
            // signed attribute is always truncated so it is the length of a SHA-1
            // digest.
            digest.truncate(20);

            Ok(plist::Value::Data(digest))
        })
        .collect::<Result<Vec<_>, AppleCodesignError>>()?;

    let mut plist = plist::Dictionary::new();
    plist.insert("cdhashes".to_string(), plist::Value::Array(hashes));

    let mut buffer = Vec::<u8>::new();
    plist::Value::from(plist)
        .to_writer_xml(&mut buffer)
        .map_err(AppleCodesignError::CodeDirectoryPlist)?;
    // We also need to include a trailing newline to conform with Apple's XML
    // writer.
    buffer.push(b'\n');

    Ok(buffer)
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum BlobsState {
    Empty,
    SpecialAdded,
    CodeDirectoryAdded,
    SignatureAdded,
    TicketAdded,
}

impl Default for BlobsState {
    fn default() -> Self {
        Self::Empty
    }
}

/// An entity for producing and writing [EmbeddedSignature].
///
/// This entity can be used to incrementally build up super blob data.
#[derive(Debug, Default)]
pub struct EmbeddedSignatureBuilder<'a> {
    state: BlobsState,
    blobs: BTreeMap<CodeSigningSlot, BlobData<'a>>,
}

impl<'a> EmbeddedSignatureBuilder<'a> {
    /// Create a new instance suitable for stapling a notarization ticket.
    ///
    /// This starts with an existing [EmbeddedSignature] / superblob because stapling
    /// a notarization ticket just adds a new ticket slot without modifying existing
    /// slots.
    pub fn new_for_stapling(signature: EmbeddedSignature<'a>) -> Result<Self, AppleCodesignError> {
        let blobs = signature
            .blobs
            .into_iter()
            .map(|blob| {
                let parsed = blob.into_parsed_blob()?;

                Ok((parsed.blob_entry.slot, parsed.blob))
            })
            .collect::<Result<BTreeMap<_, _>, AppleCodesignError>>()?;

        Ok(Self {
            state: BlobsState::CodeDirectoryAdded,
            blobs,
        })
    }

    /// Obtain the code directory registered with this instance.
    pub fn code_directory(&self) -> Option<&CodeDirectoryBlob> {
        self.blobs.get(&CodeSigningSlot::CodeDirectory).map(|blob| {
            if let BlobData::CodeDirectory(cd) = blob {
                (*cd).as_ref()
            } else {
                panic!("a non code directory should never be stored in the code directory slot");
            }
        })
    }

    /// Register a blob into a slot.
    ///
    /// There can only be a single blob per slot. Last write wins.
    ///
    /// The code directory and embedded signature cannot be added using this method.
    ///
    /// Blobs cannot be registered after a code directory or signature are added, as this
    /// would invalidate the signature.
    pub fn add_blob(
        &mut self,
        slot: CodeSigningSlot,
        blob: BlobData<'a>,
    ) -> Result<(), AppleCodesignError> {
        match self.state {
            BlobsState::Empty | BlobsState::SpecialAdded => {}
            BlobsState::CodeDirectoryAdded
            | BlobsState::SignatureAdded
            | BlobsState::TicketAdded => {
                return Err(AppleCodesignError::SignatureBuilder(
                    "cannot add blobs after code directory or signature is registered",
                ));
            }
        }

        if matches!(
            blob,
            BlobData::CodeDirectory(_)
                | BlobData::EmbeddedSignature(_)
                | BlobData::EmbeddedSignatureOld(_)
        ) {
            return Err(AppleCodesignError::SignatureBuilder(
                "cannot register code directory or signature blob via add_blob()",
            ));
        }

        self.blobs.insert(slot, blob);

        self.state = BlobsState::SpecialAdded;

        Ok(())
    }

    /// Register a [CodeDirectoryBlob] with this builder.
    ///
    /// This is the recommended mechanism to register a Code Directory with this instance.
    ///
    /// When a code directory is registered, this method will automatically ensure digests
    /// of previously registered blobs/slots are present in the code directory. This
    /// removes the burden from callers of having to keep the code directory in sync with
    /// other registered blobs.
    ///
    /// This function accepts the slot to add the code directory to because alternative
    /// slots can be registered.
    pub fn add_code_directory(
        &mut self,
        cd_slot: CodeSigningSlot,
        mut cd: CodeDirectoryBlob<'a>,
    ) -> Result<&CodeDirectoryBlob, AppleCodesignError> {
        if matches!(self.state, BlobsState::SignatureAdded) {
            return Err(AppleCodesignError::SignatureBuilder(
                "cannot add code directory after signature data added",
            ));
        }

        for (slot, blob) in &self.blobs {
            // Not all slots are expressible in the cd specials list!
            if !slot.is_code_directory_specials_expressible() {
                continue;
            }

            let digest = blob.digest_with(cd.digest_type)?;

            cd.set_slot_digest(*slot, digest)?;
        }

        self.blobs.insert(cd_slot, cd.into());
        self.state = BlobsState::CodeDirectoryAdded;

        Ok(self.code_directory().expect("we just inserted this key"))
    }

    /// Add an alternative code directory.
    ///
    /// This is a wrapper for [Self::add_code_directory()] that has logic for determining the
    /// appropriate slot for the code directory.
    pub fn add_alternative_code_directory(
        &mut self,
        cd: CodeDirectoryBlob<'a>,
    ) -> Result<&CodeDirectoryBlob, AppleCodesignError> {
        let mut our_slot = CodeSigningSlot::AlternateCodeDirectory0;

        for slot in self.blobs.keys() {
            if slot.is_alternative_code_directory() {
                our_slot = CodeSigningSlot::from(u32::from(*slot) + 1);

                if !our_slot.is_alternative_code_directory() {
                    return Err(AppleCodesignError::SignatureBuilder(
                        "no more available alternative code directory slots",
                    ));
                }
            }
        }

        self.add_code_directory(our_slot, cd)
    }

    /// The a CMS signature and register its signature blob.
    ///
    /// `signing_key` and `signing_cert` denote the keypair being used to produce a
    /// cryptographic signature.
    ///
    /// `time_stamp_url` is an optional time-stamp protocol server to use to record
    /// the signature in.
    ///
    /// `certificates` are extra X.509 certificates to register in the signing chain.
    ///
    /// This method errors if called before a code directory is registered.
    pub fn create_cms_signature(
        &mut self,
        signing_key: &dyn Sign,
        signing_cert: &CapturedX509Certificate,
        time_stamp_url: Option<&Url>,
        certificates: impl Iterator<Item = CapturedX509Certificate>,
    ) -> Result<(), AppleCodesignError> {
        let code_directory = self
            .code_directory()
            .ok_or(AppleCodesignError::SignatureBuilder(
                "cannot create CMS signature unless code directory is present",
            ))?;

        if let Some(cn) = signing_cert.subject_common_name() {
            warn!("creating cryptographic signature with certificate {}", cn);
        }

        // We need the blob serialized content of the code directory to compute
        // the message digest using alternate data.
        let code_directory_raw = code_directory.to_blob_bytes()?;

        // We need an XML plist containing code directory hashes to include as a signed
        // attribute.
        let code_directories = vec![code_directory];
        let code_directory_hashes_plist = create_code_directory_hashes_plist(
            code_directories.into_iter(),
            code_directory.digest_type,
        )?;

        let signer = SignerBuilder::new(signing_key, signing_cert.clone())
            .message_id_content(code_directory_raw)
            .signed_attribute_octet_string(
                Oid(Bytes::copy_from_slice(CDHASH_PLIST_OID.as_ref())),
                &code_directory_hashes_plist,
            );

        // If we're using a digest beyond SHA-1, that digest is included as an additional
        // signed attribute. However, Apple is using unregistered OIDs here. We only know about
        // the SHA-256 one. It exists as an `(OID, OCTET STRING)` value where the OID
        // is 2.16.840.1.101.3.4.2.1, which is registered.
        let signer = if code_directory.digest_type == DigestType::Sha256 {
            let digest = code_directory.digest_with(DigestType::Sha256)?;

            signer.signed_attribute(
                Oid(CDHASH_SHA256_OID.as_ref().into()),
                vec![AttributeValue::new(bcder::Captured::from_values(
                    bcder::Mode::Der,
                    bcder::encode::sequence((
                        Oid::from(DigestAlgorithm::Sha256).encode_ref(),
                        bcder::OctetString::new(digest.into()).encode_ref(),
                    )),
                ))],
            )
        } else {
            signer
        };

        let signer = if let Some(time_stamp_url) = time_stamp_url {
            info!("Using time-stamp server {}", time_stamp_url);
            signer.time_stamp_url(time_stamp_url.clone())?
        } else {
            signer
        };

        let der = SignedDataBuilder::default()
            // The default is `signed-data`. But Apple appears to use the `data` content-type,
            // in violation of RFC 5652 Section 5, which says `signed-data` should be
            // used when there are signatures.
            .content_type(Oid(OID_ID_DATA.as_ref().into()))
            .signer(signer)
            .certificates(certificates)
            .build_der()?;

        self.blobs.insert(
            CodeSigningSlot::Signature,
            BlobData::BlobWrapper(Box::new(BlobWrapperBlob::from_data_owned(der))),
        );
        self.state = BlobsState::SignatureAdded;

        Ok(())
    }

    /// Add notarization ticket data.
    ///
    /// This will register a new ticket slot holding the notarization ticket data.
    pub fn add_notarization_ticket(
        &mut self,
        ticket_data: Vec<u8>,
    ) -> Result<(), AppleCodesignError> {
        self.blobs.insert(
            CodeSigningSlot::Ticket,
            BlobData::BlobWrapper(Box::new(BlobWrapperBlob::from_data_owned(ticket_data))),
        );
        self.state = BlobsState::TicketAdded;

        Ok(())
    }

    /// Create the embedded signature "superblob" data.
    pub fn create_superblob(&self) -> Result<Vec<u8>, AppleCodesignError> {
        if matches!(self.state, BlobsState::Empty | BlobsState::SpecialAdded) {
            return Err(AppleCodesignError::SignatureBuilder(
                "code directory required in order to materialize superblob",
            ));
        }

        let blobs = self
            .blobs
            .iter()
            .map(|(slot, blob)| {
                let data = blob.to_blob_bytes()?;

                Ok((*slot, data))
            })
            .collect::<Result<Vec<_>, AppleCodesignError>>()?;

        create_superblob(CodeSigningMagic::EmbeddedSignature, blobs.iter())
    }
}
