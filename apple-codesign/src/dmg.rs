// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! DMG file handling.

DMG files can have code signatures as well. However, the mechanism is a bit different
from Mach-O files.

The last 512 bytes of a DMG are a "koly" structure, which we represent by
[KolyTrailer]. Within the [KolyTrailer] are a pair of [u64] denoting the
file offset and size of an embedded code signature.

The embedded code signature is a signature superblob, as represented by our
[EmbeddedSignature].

Apple's `codesign` appears to write the Code Directory, Requirement Set, and
CMS Signature slots. However, Requirement Set is empty and the CMS blob may
have no data (just a blob header).

Within the Code Directory, the code limit field is the offset of the start of
code signature superblob and there is exactly a single code digest. Unlike
Mach-O files which digest in 4kb chunks, the full content of the DMG up to the
superblob are digested in full. However, the page size is advertised as `1`,
which `codesign` reports as `none`.

The Code Directory also contains a digest in the Rep Specific slot. This digest
is over the "koly" trailer, but with the u64 for the code signature size field
zeroed out. This is likely zeroed to prevent a circular dependency: you won't
know the size of the CMS payload until the signature is created so you can't
fill in a known value ahead of time. It's worth noting that for Mach-O, the
superblob is padded with zeroes so the size of the __LINKEDIT segment can be
known before the signature is made. DMG can likely get away without padding
because the "koly" trailer is at the end of the file and any junk between
the code signature and trailer will be ignored or corrupt one of the data
structures.

The Code Directory version is 0x20100.

DMGs are stapled by adding an additional ticket slot to the superblob. However,
this slot's digest is not recorded in the code directory, as stapling occurs
after signing and modifying the code directory would modify the code directory
and invalidate prior signatures.
*/

use {
    crate::{
        code_directory::{CodeDirectoryBlob, CodeSignatureFlags},
        embedded_signature::{
            BlobData, CodeSigningSlot, Digest, DigestType, EmbeddedSignature, RequirementSetBlob,
        },
        embedded_signature_builder::EmbeddedSignatureBuilder,
        AppleCodesignError, SettingsScope, SigningSettings,
    },
    log::warn,
    scroll::{Pread, Pwrite, SizeWith},
    std::{
        borrow::Cow,
        collections::HashMap,
        fs::File,
        io::{Read, Seek, SeekFrom, Write},
        path::Path,
    },
};

const KOLY_SIZE: i64 = 512;

/// DMG trailer describing file content.
///
/// This is the main structure defining a DMG.
#[derive(Clone, Debug, Pread, PartialEq, Pwrite, SizeWith)]
pub struct KolyTrailer {
    /// "koly"
    pub signature: [u8; 4],
    pub version: u32,
    pub header_size: u32,
    pub flags: u32,
    pub running_data_fork_offset: u64,
    pub data_fork_offset: u64,
    pub data_fork_length: u64,
    pub rsrc_fork_offset: u64,
    pub rsrc_fork_length: u64,
    pub segment_number: u32,
    pub segment_count: u32,
    pub segment_id: [u32; 4],
    pub data_fork_digest_type: u32,
    pub data_fork_digest_size: u32,
    pub data_fork_digest: [u32; 32],
    pub plist_offset: u64,
    pub plist_length: u64,
    pub reserved1: [u64; 8],
    pub code_signature_offset: u64,
    pub code_signature_size: u64,
    pub reserved2: [u64; 5],
    pub main_digest_type: u32,
    pub main_digest_size: u32,
    pub main_digest: [u32; 32],
    pub image_variant: u32,
    pub sector_count: u64,
}

impl KolyTrailer {
    /// Construct an instance by reading from a seekable reader.
    ///
    /// The trailer is the final 512 bytes of the seekable stream.
    pub fn read_from<R: Read + Seek>(reader: &mut R) -> Result<Self, AppleCodesignError> {
        reader.seek(SeekFrom::End(-KOLY_SIZE))?;

        // We can't use IOread with structs larger than 256 bytes.
        let mut data = vec![];
        reader.read_to_end(&mut data)?;

        let koly = data.pread_with::<KolyTrailer>(0, scroll::BE)?;

        if &koly.signature != b"koly" {
            return Err(AppleCodesignError::DmgBadMagic);
        }

        Ok(koly)
    }

    /// Obtain the offset byte after the plist data.
    ///
    /// This is the offset at which an embedded signature superblob would be present.
    /// If no embedded signature is present, this is likely the start of [KolyTrailer].
    pub fn offset_after_plist(&self) -> u64 {
        self.plist_offset + self.plist_length
    }

    /// Obtain the digest of the trailer in a way compatible with code directory digesting.
    ///
    /// This will compute the digest of the current values but with the code signature
    /// size set to 0.
    pub fn digest_for_code_directory(
        &self,
        digest: DigestType,
    ) -> Result<Vec<u8>, AppleCodesignError> {
        let mut koly = self.clone();
        koly.code_signature_size = 0;
        koly.code_signature_offset = self.offset_after_plist();

        let mut buf = [0u8; KOLY_SIZE as usize];
        buf.pwrite_with(koly, 0, scroll::BE)?;

        digest.digest_data(&buf)
    }
}

/// An entity for reading DMG files.
///
/// It only implements enough to create code signatures over the DMG.
pub struct DmgReader {
    koly: KolyTrailer,

    /// Caches the embedded code signature data.
    code_signature_data: Option<Vec<u8>>,
}

impl DmgReader {
    /// Construct a new instance from a reader.
    pub fn new<R: Read + Seek>(reader: &mut R) -> Result<Self, AppleCodesignError> {
        let koly = KolyTrailer::read_from(reader)?;

        let code_signature_offset = koly.code_signature_offset;
        let code_signature_size = koly.code_signature_size;

        let code_signature_data = if code_signature_offset != 0 && code_signature_size != 0 {
            reader.seek(SeekFrom::Start(code_signature_offset))?;
            let mut data = vec![];
            reader.take(code_signature_size).read_to_end(&mut data)?;

            Some(data)
        } else {
            None
        };

        Ok(Self {
            koly,
            code_signature_data,
        })
    }

    /// Obtain the main data structure describing this DMG.
    pub fn koly(&self) -> &KolyTrailer {
        &self.koly
    }

    /// Obtain the embedded code signature superblob.
    pub fn embedded_signature(&self) -> Result<Option<EmbeddedSignature<'_>>, AppleCodesignError> {
        if let Some(data) = &self.code_signature_data {
            Ok(Some(EmbeddedSignature::from_bytes(data)?))
        } else {
            Ok(None)
        }
    }

    /// Digest an arbitrary slice of the file.
    fn digest_slice_with<R: Read + Seek>(
        &self,
        digest: DigestType,
        reader: &mut R,
        offset: u64,
        length: u64,
    ) -> Result<Digest<'static>, AppleCodesignError> {
        reader.seek(SeekFrom::Start(offset))?;

        let mut reader = reader.take(length);

        let mut d = digest.as_hasher()?;

        loop {
            let mut buffer = [0u8; 16384];
            let count = reader.read(&mut buffer)?;

            d.update(&buffer[0..count]);

            if count == 0 {
                break;
            }
        }

        Ok(Digest {
            data: d.finish().as_ref().to_vec().into(),
        })
    }

    /// Digest the content of the DMG up to the code signature or [KolyTrailer].
    ///
    /// This digest is used as the code digest in the code directory.
    pub fn digest_content_with<R: Read + Seek>(
        &self,
        digest: DigestType,
        reader: &mut R,
    ) -> Result<Digest<'static>, AppleCodesignError> {
        if self.koly.code_signature_offset != 0 {
            self.digest_slice_with(digest, reader, 0, self.koly.code_signature_offset)
        } else {
            reader.seek(SeekFrom::End(-KOLY_SIZE))?;
            let size = reader.stream_position()?;

            self.digest_slice_with(digest, reader, 0, size)
        }
    }
}

/// Determines whether a filesystem path is a DMG.
///
/// Returns true if the path has a DMG trailer.
pub fn path_is_dmg(path: impl AsRef<Path>) -> Result<bool, AppleCodesignError> {
    let mut fh = File::open(path.as_ref())?;

    Ok(KolyTrailer::read_from(&mut fh).is_ok())
}

/// Entity for signing DMG files.
#[derive(Clone, Debug, Default)]
pub struct DmgSigner {}

impl DmgSigner {
    /// Sign a DMG.
    ///
    /// Parameters controlling the signing operation are specified by `settings`.
    ///
    /// `file` is a readable and writable file. The DMG signature will be written
    /// into the source file.
    pub fn sign_file(
        &self,
        settings: &SigningSettings,
        fh: &mut File,
    ) -> Result<(), AppleCodesignError> {
        warn!("signing DMG");

        let koly = DmgReader::new(fh)?.koly().clone();
        let signature = self.create_superblob(settings, fh)?;

        Self::write_embedded_signature(fh, koly, &signature)
    }

    /// Staple a notarization ticket to a DMG.
    pub fn staple_file(
        &self,
        fh: &mut File,
        ticket_data: Vec<u8>,
    ) -> Result<(), AppleCodesignError> {
        warn!(
            "stapling DMG with {} byte notarization ticket",
            ticket_data.len()
        );

        let reader = DmgReader::new(fh)?;
        let koly = reader.koly().clone();
        let signature = reader
            .embedded_signature()?
            .ok_or(AppleCodesignError::DmgStapleNoSignature)?;

        let mut builder = EmbeddedSignatureBuilder::new_for_stapling(signature)?;
        builder.add_notarization_ticket(ticket_data)?;

        let signature = builder.create_superblob()?;

        Self::write_embedded_signature(fh, koly, &signature)
    }

    fn write_embedded_signature(
        fh: &mut File,
        mut koly: KolyTrailer,
        signature: &[u8],
    ) -> Result<(), AppleCodesignError> {
        warn!("writing {} byte signature", signature.len());
        fh.seek(SeekFrom::Start(koly.offset_after_plist()))?;
        fh.write_all(signature)?;

        koly.code_signature_offset = koly.offset_after_plist();
        koly.code_signature_size = signature.len() as _;

        let mut trailer = [0u8; KOLY_SIZE as usize];
        trailer.pwrite_with(&koly, 0, scroll::BE)?;

        fh.write_all(&trailer)?;

        fh.set_len(koly.code_signature_offset + koly.code_signature_size + KOLY_SIZE as u64)?;

        Ok(())
    }

    /// Create the embedded signature superblob content.
    pub fn create_superblob<F: Read + Write + Seek>(
        &self,
        settings: &SigningSettings,
        fh: &mut F,
    ) -> Result<Vec<u8>, AppleCodesignError> {
        let mut builder = EmbeddedSignatureBuilder::default();

        for (slot, blob) in self.create_special_blobs()? {
            builder.add_blob(slot, blob)?;
        }

        builder.add_code_directory(self.create_code_directory(settings, fh)?)?;

        if let Some((signing_key, signing_cert)) = settings.signing_key() {
            builder.create_cms_signature(
                signing_key,
                signing_cert,
                settings.time_stamp_url(),
                settings.certificate_chain().iter().cloned(),
            )?;
        }

        builder.create_superblob()
    }

    /// Create the code directory data structure that is part of the embedded signature.
    ///
    /// This won't be the final data structure state that is serialized, as it may be
    /// amended to in other functions.
    pub fn create_code_directory<F: Read + Write + Seek>(
        &self,
        settings: &SigningSettings,
        fh: &mut F,
    ) -> Result<CodeDirectoryBlob<'static>, AppleCodesignError> {
        let reader = DmgReader::new(fh)?;

        let mut flags = settings
            .code_signature_flags(SettingsScope::Main)
            .unwrap_or_else(CodeSignatureFlags::empty);

        if settings.signing_key().is_some() {
            flags -= CodeSignatureFlags::ADHOC;
        } else {
            flags |= CodeSignatureFlags::ADHOC;
        }

        warn!("using code signature flags: {:?}", flags);

        let ident = Cow::Owned(
            settings
                .binary_identifier(SettingsScope::Main)
                .ok_or(AppleCodesignError::NoIdentifier)?
                .to_string(),
        );

        warn!("using identifier {}", ident);

        let code_hashes = vec![reader.digest_content_with(*settings.digest_type(), fh)?];

        let koly_digest = reader
            .koly()
            .digest_for_code_directory(*settings.digest_type())?;

        let mut special_hashes = HashMap::new();
        special_hashes.insert(CodeSigningSlot::RepSpecific, koly_digest.into());

        let cd = CodeDirectoryBlob {
            version: 0x20100,
            flags,
            code_limit: reader.koly().offset_after_plist() as u32,
            hash_size: settings.digest_type().hash_len()? as u8,
            hash_type: *settings.digest_type(),
            platform: 0,
            page_size: 1,
            spare2: 0,
            scatter_offset: None,
            spare3: None,
            code_limit_64: None,
            exec_seg_base: None,
            exec_seg_limit: None,
            exec_seg_flags: None,
            runtime: None,
            pre_encrypt_offset: None,
            linkage_hash_type: None,
            linkage_truncated: None,
            spare4: None,
            linkage_offset: None,
            linkage_size: None,
            ident,
            team_name: None,
            code_hashes,
            special_hashes,
        };

        Ok(cd)
    }

    /// Create special blobs that are added to the superblob.
    pub fn create_special_blobs(
        &self,
    ) -> Result<Vec<(CodeSigningSlot, BlobData)>, AppleCodesignError> {
        Ok(vec![(
            CodeSigningSlot::RequirementSet,
            RequirementSetBlob::default().into(),
        )])
    }
}
