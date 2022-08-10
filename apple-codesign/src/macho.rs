// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Mach-O primitives related to code signing

Code signing data is embedded within the named `__LINKEDIT` segment of
the Mach-O binary. An `LC_CODE_SIGNATURE` load command in the Mach-O header
will point you at this data. See `find_signature_data()` for this logic.

Within the `__LINKEDIT` segment is a superblob defining embedded signature
data.
*/

use {
    crate::{
        code_hash::segment_digests,
        embedded_signature::EmbeddedSignature,
        error::AppleCodesignError,
        signing_settings::{SettingsScope, SigningSettings},
    },
    cryptographic_message_syntax::time_stamp_message_http,
    goblin::mach::{
        constants::{SEG_LINKEDIT, SEG_PAGEZERO, SEG_TEXT},
        header::MH_EXECUTE,
        load_command::{
            CommandVariant, LinkeditDataCommand, LC_BUILD_VERSION, SIZEOF_LINKEDIT_DATA_COMMAND,
        },
        parse_magic_and_ctx, Mach, MachO,
    },
    scroll::Pread,
    x509_certificate::DigestAlgorithm,
};

/// A Mach-O binary.
pub struct MachOBinary<'a> {
    /// Index within a fat binary this Mach-O resides at.
    ///
    /// If `None`, this is not inside a fat binary.
    pub index: Option<usize>,

    /// The parsed Mach-O binary.
    pub macho: MachO<'a>,

    /// The raw data backing the Mach-O binary.
    pub data: &'a [u8],
}

impl<'a> MachOBinary<'a> {
    /// Parse a non-universal Mach-O binary from raw data.
    pub fn parse(data: &'a [u8]) -> Result<Self, AppleCodesignError> {
        let macho = MachO::parse(data, 0)?;

        Ok(Self {
            index: None,
            macho,
            data,
        })
    }
}

impl<'a> MachOBinary<'a> {
    /// Attempt to extract a reference to raw signature data in a Mach-O binary.
    ///
    /// An `LC_CODE_SIGNATURE` load command in the Mach-O file header points to
    /// signature data in the `__LINKEDIT` segment.
    ///
    /// This function is used as part of parsing signature data. You probably want to
    /// use a function that parses referenced data.
    pub fn find_signature_data(
        &self,
    ) -> Result<Option<MachOSignatureData<'a>>, AppleCodesignError> {
        if let Some(linkedit_data_command) =
            self.macho.load_commands.iter().find_map(|load_command| {
                if let CommandVariant::CodeSignature(command) = &load_command.command {
                    Some(command)
                } else {
                    None
                }
            })
        {
            // Now find the slice of data in the __LINKEDIT segment we need to parse.
            let (linkedit_segment_index, linkedit) = self
                .macho
                .segments
                .iter()
                .enumerate()
                .find(|(_, segment)| {
                    if let Ok(name) = segment.name() {
                        name == SEG_LINKEDIT
                    } else {
                        false
                    }
                })
                .ok_or(AppleCodesignError::MissingLinkedit)?;

            let linkedit_segment_start_offset = linkedit.fileoff as usize;
            let linkedit_segment_end_offset = linkedit_segment_start_offset + linkedit.data.len();
            let linkedit_signature_start_offset = linkedit_data_command.dataoff as usize;
            let linkedit_signature_end_offset =
                linkedit_signature_start_offset + linkedit_data_command.datasize as usize;
            let signature_start_offset =
                linkedit_data_command.dataoff as usize - linkedit.fileoff as usize;
            let signature_end_offset =
                signature_start_offset + linkedit_data_command.datasize as usize;

            let signature_data = &linkedit.data[signature_start_offset..signature_end_offset];

            Ok(Some(MachOSignatureData {
                linkedit_segment_index,
                linkedit_segment_start_offset,
                linkedit_segment_end_offset,
                linkedit_signature_start_offset,
                linkedit_signature_end_offset,
                signature_start_offset,
                signature_end_offset,
                linkedit_segment_data: linkedit.data,
                signature_data,
            }))
        } else {
            Ok(None)
        }
    }

    /// Obtain the code signature in the entity.
    ///
    /// Returns `Ok(None)` if no signature exists, `Ok(Some)` if it does, or
    /// `Err` if there is a parse error.
    pub fn code_signature(&self) -> Result<Option<EmbeddedSignature>, AppleCodesignError> {
        if let Some(signature) = self.find_signature_data()? {
            Ok(Some(EmbeddedSignature::from_bytes(
                signature.signature_data,
            )?))
        } else {
            Ok(None)
        }
    }

    /// Determine the start and end offset of the executable segment of a binary.
    pub fn executable_segment_boundary(&self) -> Result<(u64, u64), AppleCodesignError> {
        let segment = self
            .macho
            .segments
            .iter()
            .find(|segment| matches!(segment.name(), Ok(SEG_TEXT)))
            .ok_or_else(|| AppleCodesignError::InvalidBinary("no __TEXT segment".into()))?;

        Ok((segment.fileoff, segment.fileoff + segment.data.len() as u64))
    }

    /// Whether this is an executable Mach-O file.
    pub fn is_executable(&self) -> bool {
        self.macho.header.filetype == MH_EXECUTE
    }

    /// The start offset of the code signature data within the __LINKEDIT segment.
    pub fn code_signature_linkedit_start_offset(&self) -> Option<u32> {
        let segment = self
            .macho
            .segments
            .iter()
            .find(|segment| matches!(segment.name(), Ok(SEG_LINKEDIT)));

        if let (Some(segment), Some(command)) = (segment, self.code_signature_load_command()) {
            Some((command.dataoff as u64 - segment.fileoff) as u32)
        } else {
            None
        }
    }

    /// The end offset of the code signature data within the __LINKEDIT segment.
    pub fn code_signature_linkedit_end_offset(&self) -> Option<u32> {
        let start_offset = self.code_signature_linkedit_start_offset()?;

        self.code_signature_load_command()
            .map(|command| start_offset + command.datasize)
    }

    /// The byte offset within the binary at which point "code" stops.
    ///
    /// If a signature is present, this is the offset of the start of the
    /// signature. Else it represents the end of the binary.
    pub fn code_limit_binary_offset(&self) -> Result<u64, AppleCodesignError> {
        let last_segment = self
            .macho
            .segments
            .last()
            .ok_or(AppleCodesignError::MissingLinkedit)?;
        if !matches!(last_segment.name(), Ok(SEG_LINKEDIT)) {
            return Err(AppleCodesignError::LinkeditNotLast);
        }

        if let Some(offset) = self.code_signature_linkedit_start_offset() {
            Ok(last_segment.fileoff + offset as u64)
        } else {
            Ok(last_segment.fileoff + last_segment.data.len() as u64)
        }
    }

    /// Obtain __LINKEDIT segment data before the signature data.
    ///
    /// If there is no signature, returns all the data for the __LINKEDIT segment.
    pub fn linkedit_data_before_signature(&self) -> Option<&[u8]> {
        let segment = self
            .macho
            .segments
            .iter()
            .find(|segment| matches!(segment.name(), Ok(SEG_LINKEDIT)));

        if let Some(segment) = segment {
            if let Some(offset) = self.code_signature_linkedit_start_offset() {
                Some(&segment.data[0..offset as usize])
            } else {
                Some(segment.data)
            }
        } else {
            None
        }
    }

    /// Obtain slices of segment data suitable for digesting.
    ///
    /// The slices are likely digested as part of computing digests
    /// embedded in the code directory.
    pub fn digestable_segment_data(&self) -> Vec<&[u8]> {
        let mut last_segment_end_offset = None;

        self.macho
            .segments
            .iter()
            .filter(|segment| !matches!(segment.name(), Ok(SEG_PAGEZERO)))
            .flat_map(|segment| {
                let mut segments = vec![];

                let our_start_offset = segment.fileoff;
                let our_end_offset = segment.fileoff + segment.filesize;

                // There can be gaps between segments. Emit that gap as its own virtual segment.
                if let Some(last_segment_end_offset) = last_segment_end_offset {
                    if our_start_offset > last_segment_end_offset {
                        let missing_data =
                            &self.data[last_segment_end_offset as usize..our_start_offset as usize];
                        segments.push(missing_data);
                    }
                }

                last_segment_end_offset = Some(our_end_offset);

                // The __LINKEDIT segment is only digested up to the code signature.
                if matches!(segment.name(), Ok(SEG_LINKEDIT)) {
                    segments.push(
                        self.linkedit_data_before_signature()
                            .expect("__LINKEDIT data should resolve"),
                    );
                } else {
                    segments.push(segment.data);
                }

                segments
            })
            .collect::<Vec<_>>()
    }

    /// Resolve the load command for the code signature.
    pub fn code_signature_load_command(&self) -> Option<LinkeditDataCommand> {
        self.macho.load_commands.iter().find_map(|lc| {
            if let CommandVariant::CodeSignature(command) = lc.command {
                Some(command)
            } else {
                None
            }
        })
    }

    /// Attempt to locate embedded Info.plist data.
    pub fn embedded_info_plist(&self) -> Result<Option<Vec<u8>>, AppleCodesignError> {
        // Mach-O binaries can have the Info.plist data in an `__info_plist` section
        // within the __TEXT segment.
        for segment in &self.macho.segments {
            if matches!(segment.name(), Ok(SEG_TEXT)) {
                for (section, data) in segment.sections()? {
                    if matches!(section.name(), Ok("__info_plist")) {
                        return Ok(Some(data.to_vec()));
                    }
                }
            }
        }

        Ok(None)
    }

    /// Determines whether this crate is capable of signing a given Mach-O binary.
    ///
    /// Code in this crate is limited in the amount of Mach-O binary manipulation
    /// it can perform (supporting rewriting all valid Mach-O binaries effectively
    /// requires low-level awareness of all Mach-O constructs in order to perform
    /// offset manipulation). This function can be used to test signing
    /// compatibility.
    ///
    /// We currently only support signing Mach-O files already containing an
    /// embedded signature. Often linked binaries automatically contain an embedded
    /// signature containing just the code directory (without a cryptographically
    /// signed signature), so this limitation hopefully isn't impactful.
    pub fn check_signing_capability(&self) -> Result<(), AppleCodesignError> {
        let last_segment = self
            .macho
            .segments
            .iter()
            .last()
            .ok_or(AppleCodesignError::MissingLinkedit)?;

        // Last segment needs to be __LINKEDIT so we don't have to write offsets.
        if !matches!(last_segment.name(), Ok(SEG_LINKEDIT)) {
            return Err(AppleCodesignError::LinkeditNotLast);
        }

        // Rules:
        //
        // 1. If there is an existing signature, there must be no data in
        //    the binary after it. (We don't know how to update references to
        //    other data to reflect offset changes.)
        // 2. If there isn't an existing signature, there must be "room" between
        //    the last load command and the first section to write a new load
        //    command for the signature.

        if let Some(offset) = self.code_signature_linkedit_end_offset() {
            if offset as usize == last_segment.data.len() {
                Ok(())
            } else {
                Err(AppleCodesignError::DataAfterSignature)
            }
        } else {
            let last_load_command = self
                .macho
                .load_commands
                .iter()
                .last()
                .ok_or_else(|| AppleCodesignError::InvalidBinary("no load commands".into()))?;

            let first_section = self
                .macho
                .segments
                .iter()
                .map(|segment| segment.sections())
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .flatten()
                .next()
                .ok_or_else(|| AppleCodesignError::InvalidBinary("no sections".into()))?;

            let load_commands_end_offset =
                last_load_command.offset + last_load_command.command.cmdsize();

            if first_section.0.offset as usize - load_commands_end_offset
                >= SIZEOF_LINKEDIT_DATA_COMMAND
            {
                Ok(())
            } else {
                Err(AppleCodesignError::LoadCommandNoRoom)
            }
        }
    }

    /// Estimate the size in bytes of an embedded code signature.
    pub fn estimate_embedded_signature_size(
        &self,
        settings: &SigningSettings,
    ) -> Result<usize, AppleCodesignError> {
        let code_directory_count = 1 + settings
            .extra_digests(SettingsScope::Main)
            .map(|x| x.len())
            .unwrap_or_default();

        // Assume the common data structures are 1024 bytes.
        let mut size = 1024 * code_directory_count;

        // Reserve room for the code digests, which are proportional to binary size.
        // We could avoid doing the actual digesting work here. But until people
        // complain, don't worry about it.
        size += segment_digests(
            self.digestable_segment_data().into_iter(),
            *settings.digest_type(),
            4096,
        )?
        .into_iter()
        .map(|x| x.len())
        .sum::<usize>();

        if let Some(digests) = settings.extra_digests(SettingsScope::Main) {
            for digest in digests {
                size += segment_digests(self.digestable_segment_data().into_iter(), *digest, 4096)?
                    .into_iter()
                    .map(|x| x.len())
                    .sum::<usize>();
            }
        }

        // Assume the CMS data will take a fixed size.
        if settings.signing_key().is_some() {
            size += 4096;
        }

        // Long certificate chains could blow up the size. Account for those.
        for cert in settings.certificate_chain() {
            size += cert.constructed_data().len();
        }

        // Obtain an actual timestamp token of placeholder data and use its length.
        // This may be excessive to actually query the time-stamp server and issue
        // a token. But these operations should be "cheap."
        if let Some(timestamp_url) = settings.time_stamp_url() {
            let message = b"deadbeef".repeat(32);

            if let Ok(response) =
                time_stamp_message_http(timestamp_url.clone(), &message, DigestAlgorithm::Sha256)
            {
                if response.is_success() {
                    if let Some(l) = response.token_content_size() {
                        size += l;
                    } else {
                        size += 8192;
                    }
                } else {
                    size += 8192;
                }
            } else {
                size += 8192;
            }
        }

        // Align on 1k boundaries just because.
        size += 1024 - size % 1024;

        Ok(size)
    }

    /// Attempt to resolve the mach-o targeting settings.
    pub fn find_targeting(&self) -> Result<Option<MachoTarget>, AppleCodesignError> {
        let ctx = parse_magic_and_ctx(self.data, 0)?
            .1
            .expect("context should have been parsed before");

        for lc in &self.macho.load_commands {
            if lc.command.cmd() == LC_BUILD_VERSION {
                let build_version = self
                    .data
                    .pread_with::<BuildVersionCommand>(lc.offset, ctx.le)?;

                return Ok(Some(MachoTarget {
                    platform: build_version.platform.into(),
                    minimum_os_version: parse_version_nibbles(build_version.minos),
                    sdk_version: parse_version_nibbles(build_version.sdk),
                }));
            }
        }

        for lc in &self.macho.load_commands {
            let command = match lc.command {
                CommandVariant::VersionMinMacosx(c) => Some((c, Platform::MacOs)),
                CommandVariant::VersionMinIphoneos(c) => Some((c, Platform::IOs)),
                CommandVariant::VersionMinTvos(c) => Some((c, Platform::TvOs)),
                CommandVariant::VersionMinWatchos(c) => Some((c, Platform::WatchOs)),
                _ => None,
            };

            if let Some((command, platform)) = command {
                return Ok(Some(MachoTarget {
                    platform,
                    minimum_os_version: parse_version_nibbles(command.version),
                    sdk_version: parse_version_nibbles(command.sdk),
                }));
            }
        }

        Ok(None)
    }
}

/// Describes signature data embedded within a Mach-O binary.
pub struct MachOSignatureData<'a> {
    /// Which segment offset is the `__LINKEDIT` segment.
    pub linkedit_segment_index: usize,

    /// Start offset of `__LINKEDIT` segment within the binary.
    pub linkedit_segment_start_offset: usize,

    /// End offset of `__LINKEDIT` segment within the binary.
    pub linkedit_segment_end_offset: usize,

    /// Start offset of signature data in `__LINKEDIT` within the binary.
    pub linkedit_signature_start_offset: usize,

    /// End offset of signature data in `__LINKEDIT` within the binary.
    pub linkedit_signature_end_offset: usize,

    /// The start offset of the signature data within the `__LINKEDIT` segment.
    pub signature_start_offset: usize,

    /// The end offset of the signature data within the `__LINKEDIT` segment.
    pub signature_end_offset: usize,

    /// Raw data in the `__LINKEDIT` segment.
    pub linkedit_segment_data: &'a [u8],

    /// The signature data within the `__LINKEDIT` segment.
    pub signature_data: &'a [u8],
}

/// Content of an `LC_BUILD_VERSION` load command.
#[derive(Clone, Debug, Pread)]
pub struct BuildVersionCommand {
    /// LC_BUILD_VERSION
    pub cmd: u32,
    /// Size of load command data.
    ///
    /// sizeof(self) + self.ntools * sizeof(BuildToolsVersion)
    pub cmdsize: u32,
    /// Platform identifier.
    pub platform: u32,
    /// Minimum operating system version.
    ///
    /// X.Y.Z encoded in nibbles as xxxx.yy.zz.
    pub minos: u32,
    /// SDK version.
    ///
    /// X.Y.Z encoded in nibbles as xxxx.yy.zz.
    pub sdk: u32,
    /// Number of tools entries following this structure.
    pub ntools: u32,
}

/// Represents `PLATFORM_` mach-o constants.
pub enum Platform {
    MacOs,
    IOs,
    TvOs,
    WatchOs,
    BridgeOs,
    MacCatalyst,
    IosSimulator,
    TvOsSimulator,
    WatchOsSimulator,
    DriverKit,
    Unknown(u32),
}

impl std::fmt::Display for Platform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MacOs => f.write_str("macOS"),
            Self::IOs => f.write_str("iOS"),
            Self::TvOs => f.write_str("tvOS"),
            Self::WatchOs => f.write_str("watchOS"),
            Self::BridgeOs => f.write_str("bridgeOS"),
            Self::MacCatalyst => f.write_str("macCatalyst"),
            Self::IosSimulator => f.write_str("iOSSimulator"),
            Self::TvOsSimulator => f.write_str("tvOSSimulator"),
            Self::WatchOsSimulator => f.write_str("watchOSSimulator"),
            Self::DriverKit => f.write_str("driverKit"),
            Self::Unknown(v) => f.write_fmt(format_args!("Unknown ({})", v)),
        }
    }
}

impl From<u32> for Platform {
    fn from(v: u32) -> Self {
        match v {
            1 => Self::MacOs,
            2 => Self::IOs,
            3 => Self::TvOs,
            4 => Self::WatchOs,
            5 => Self::BridgeOs,
            6 => Self::MacCatalyst,
            7 => Self::IosSimulator,
            8 => Self::TvOsSimulator,
            9 => Self::WatchOsSimulator,
            10 => Self::DriverKit,
            _ => Self::Unknown(v),
        }
    }
}

impl Platform {
    /// Resolve SHA-256 digest/signatures support for a given platform type.
    pub fn sha256_digest_support(&self) -> Result<semver::VersionReq, AppleCodesignError> {
        let version = match self {
            // macOS 10.11.4 introduced support for SHA-256.
            Self::MacOs => ">=10.11.4",
            // 11.0+ support SHA-256.
            Self::IOs | Self::TvOs => ">=11.0.0",
            // WatchOS always uses SHA-1 it appears.
            Self::WatchOs => ">9999",
            // Assume no platform needs SHA-1.
            Self::Unknown(0) => ">9999",
            // Assume everything else is new and supports SHA-256.
            _ => "*",
        };

        Ok(semver::VersionReq::parse(version)?)
    }
}

/// Targeting settings for a Mach-O binary.
pub struct MachoTarget {
    /// The OS/platform being targeted.
    pub platform: Platform,
    /// Minimum required OS version.
    pub minimum_os_version: semver::Version,
    /// SDK version targeting.
    pub sdk_version: semver::Version,
}

/// Parses and integer with nibbles xxxx.yy.zz into a [semver::Version].
pub fn parse_version_nibbles(v: u32) -> semver::Version {
    let major = v >> 16;
    let minor = v << 16 >> 24;
    let patch = v & 0xff;

    semver::Version::new(major as _, minor as _, patch as _)
}

/// Convert a [semver::Version] to a u32 with nibble encoding used by Mach-O.
pub fn semver_to_macho_target_version(version: &semver::Version) -> u32 {
    let major = version.major as u32;
    let minor = version.minor as u32;
    let patch = version.patch as u32;

    (major << 16) | ((minor & 0xff) << 8) | (patch & 0xff)
}

/// Represents a semi-parsed Mach[-O] binary.
pub struct MachFile<'a> {
    machos: Vec<MachOBinary<'a>>,
}

impl<'a> MachFile<'a> {
    /// Construct an instance from data.
    pub fn parse(data: &'a [u8]) -> Result<Self, AppleCodesignError> {
        let mach = Mach::parse(data)?;

        let machos = match mach {
            Mach::Binary(macho) => vec![MachOBinary {
                index: None,
                macho,
                data,
            }],
            Mach::Fat(multiarch) => {
                let mut machos = vec![];

                for (index, arch) in multiarch.arches()?.into_iter().enumerate() {
                    let macho = multiarch.get(index)?;

                    machos.push(MachOBinary {
                        index: Some(index),
                        macho,
                        data: arch.slice(data),
                    });
                }

                machos
            }
        };

        Ok(Self { machos })
    }

    /// Whether this Mach-O data has multiple architectures.
    pub fn is_fat(&self) -> bool {
        self.machos.len() > 1
    }

    /// Iterate [MachO] instances in this data.
    ///
    /// The `Option<usize>` is `Some` if this is a universal Mach-O or `None` otherwise.
    pub fn iter_macho(&self) -> impl Iterator<Item = &MachOBinary> {
        self.machos.iter()
    }

    pub fn nth_macho(&self, index: usize) -> Result<&MachOBinary<'a>, AppleCodesignError> {
        Ok(self
            .machos
            .get(index)
            .ok_or_else(|| AppleCodesignError::InvalidMachOIndex(index))?)
    }

    /// Produce an iterator over each [MachOBinary], consuming self.
    pub fn into_iter(self) -> impl Iterator<Item = MachOBinary<'a>> {
        self.machos.into_iter()
    }
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        crate::embedded_signature::Blob,
        std::{
            io::Read,
            path::{Path, PathBuf},
        },
    };

    const MACHO_UNIVERSAL_MAGIC: [u8; 4] = [0xca, 0xfe, 0xba, 0xbe];
    const MACHO_64BIT_MAGIC: [u8; 4] = [0xfe, 0xed, 0xfa, 0xcf];

    /// Find files in a directory appearing to be Mach-O by sniffing magic.
    ///
    /// Ignores file I/O errors.
    fn find_likely_macho_files(path: &Path) -> Vec<PathBuf> {
        let mut res = Vec::new();

        let dir = std::fs::read_dir(path).unwrap();

        for entry in dir {
            let entry = entry.unwrap();

            if let Ok(mut fh) = std::fs::File::open(&entry.path()) {
                let mut magic = [0; 4];

                if let Ok(size) = fh.read(&mut magic) {
                    if size == 4 && (magic == MACHO_UNIVERSAL_MAGIC || magic == MACHO_64BIT_MAGIC) {
                        res.push(entry.path());
                    }
                }
            }
        }

        res
    }

    fn find_apple_embedded_signature<'a>(macho: &'a MachOBinary) -> Option<EmbeddedSignature<'a>> {
        if let Ok(Some(signature)) = macho.code_signature() {
            Some(signature)
        } else {
            None
        }
    }

    fn validate_macho(path: &Path, macho: &MachOBinary) {
        // We found signature data in the binary.
        if let Some(signature) = find_apple_embedded_signature(macho) {
            // Attempt a deep parse of all blobs.
            for blob in &signature.blobs {
                match blob.clone().into_parsed_blob() {
                    Ok(parsed) => {
                        // Attempt to roundtrip the blob data.
                        match parsed.blob.to_blob_bytes() {
                            Ok(serialized) => {
                                if serialized != blob.data {
                                    println!("blob serialization roundtrip failure on {}: index {}, magic {:?}",
                                        path.display(),
                                        blob.index,
                                        blob.magic,
                                    );
                                }
                            }
                            Err(e) => {
                                println!(
                                    "blob serialization failure on {}; index {}, magic {:?}: {:?}",
                                    path.display(),
                                    blob.index,
                                    blob.magic,
                                    e
                                );
                            }
                        }
                    }
                    Err(e) => {
                        println!(
                            "blob parse failure on {}; index {}, magic {:?}: {:?}",
                            path.display(),
                            blob.index,
                            blob.magic,
                            e
                        );
                    }
                }
            }

            // Found a CMS signed data blob.
            if matches!(signature.signature_data(), Ok(Some(_))) {
                match signature.signed_data() {
                    Ok(Some(signed_data)) => {
                        for signer in signed_data.signers() {
                            if let Err(e) = signer.verify_signature_with_signed_data(&signed_data) {
                                println!(
                                    "signature verification failed for {}: {}",
                                    path.display(),
                                    e
                                );
                            }

                            if let Ok(()) =
                                signer.verify_message_digest_with_signed_data(&signed_data)
                            {
                                println!(
                                    "message digest verification unexpectedly correct for {}",
                                    path.display()
                                );
                            }
                        }
                    }
                    Ok(None) => {
                        panic!("this shouln't happen (validated signature data is present");
                    }
                    Err(e) => {
                        println!("error performing CMS parse of {}: {:?}", path.display(), e);
                    }
                }
            }
        }
    }

    fn validate_macho_in_dir(dir: &Path) {
        for path in find_likely_macho_files(dir).into_iter() {
            if let Ok(file_data) = std::fs::read(&path) {
                if let Ok(mach) = MachFile::parse(&file_data) {
                    for macho in mach.into_iter() {
                        validate_macho(&path, &macho);
                    }
                }
            }
        }
    }

    #[test]
    fn parse_applications_macho_signatures() {
        // This test scans common directories containing Mach-O files on macOS and
        // verifies we can parse CMS blobs within.

        if let Ok(dir) = std::fs::read_dir("/Applications") {
            for entry in dir {
                let entry = entry.unwrap();

                let search_dir = entry.path().join("Contents").join("MacOS");

                if search_dir.exists() {
                    validate_macho_in_dir(&search_dir);
                }
            }
        }

        for dir in &["/usr/bin", "/usr/local/bin", "/opt/homebrew/bin"] {
            let dir = PathBuf::from(dir);

            if dir.exists() {
                validate_macho_in_dir(&dir);
            }
        }
    }

    #[test]
    fn version_nibbles() {
        assert_eq!(
            parse_version_nibbles(12 << 16 | 1 << 8 | 2),
            semver::Version::new(12, 1, 2)
        );
        assert_eq!(
            parse_version_nibbles(11 << 16 | 10 << 8 | 15),
            semver::Version::new(11, 10, 15)
        );
        assert_eq!(
            semver_to_macho_target_version(&semver::Version::new(12, 1, 2)),
            12 << 16 | 1 << 8 | 2
        );
    }
}
