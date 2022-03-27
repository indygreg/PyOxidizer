// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Attach Apple notarization tickets to signed entities.

Stapling refers to the act of taking an Apple issued notarization
ticket (generated after uploading content to Apple for inspection)
and attaching that ticket to the entity that was uploaded. The
mechanism varies, but stapling is literally just fetching a payload
from Apple and attaching it to something else.
*/

use {
    crate::{
        bundle_signing::SignedMachOInfo,
        embedded_signature::DigestType,
        reader::PathType,
        ticket_lookup::{default_client, lookup_notarization_ticket},
        AppleCodesignError,
    },
    apple_bundles::{BundlePackageType, DirectoryBundle},
    apple_xar::reader::XarReader,
    log::{error, info, warn},
    reqwest::blocking::Client,
    scroll::{IOread, IOwrite, Pread, Pwrite, SizeWith},
    std::{
        fmt::Debug,
        fs::File,
        io::{Read, Seek, SeekFrom, Write},
        path::Path,
    },
};

/// Resolve the notarization ticket record name from a bundle.
///
/// The record name is derived from the digest of the code directory of the
/// main binary within the bundle.
pub fn record_name_from_app_bundle(bundle: &DirectoryBundle) -> Result<String, AppleCodesignError> {
    if !matches!(bundle.package_type(), BundlePackageType::App) {
        return Err(AppleCodesignError::StapleUnsupportedBundleType(
            bundle.package_type(),
        ));
    }

    let main_exe = bundle
        .files(false)
        .map_err(AppleCodesignError::DirectoryBundle)?
        .into_iter()
        .find(|file| matches!(file.is_main_executable(), Ok(true)))
        .ok_or(AppleCodesignError::StapleMainExecutableNotFound)?;

    // Now extract the code signature so we can resolve the code directory.
    info!(
        "resolving bundle's record name from {}",
        main_exe.absolute_path().display()
    );
    let macho_data = std::fs::read(main_exe.absolute_path())?;

    let signed = SignedMachOInfo::parse_data(&macho_data)?;

    let record_name = signed.notarization_ticket_record_name()?;

    Ok(record_name)
}

/// Staple a ticket to a bundle as defined by the path to a directory.
///
/// Stapling a bundle (e.g. `MyApp.app`) is literally just writing a
/// `Contents/CodeResources` file containing the raw ticket data.
pub fn staple_ticket_to_bundle(
    bundle: &DirectoryBundle,
    ticket_data: &[u8],
) -> Result<(), AppleCodesignError> {
    let path = bundle.resolve_path("CodeResources");

    warn!("writing notarization ticket to {}", path.display());
    std::fs::write(&path, ticket_data)?;

    Ok(())
}

/// Magic header for xar trailer struct.
///
/// `t8lr`.
const XAR_NOTARIZATION_TRAILER_MAGIC: [u8; 4] = [0x74, 0x38, 0x6c, 0x72];

#[derive(Clone, Copy, Debug, IOread, IOwrite, Pread, Pwrite, SizeWith)]
pub struct XarNotarizationTrailer {
    /// "t8lr"
    pub magic: [u8; 4],
    pub version: u16,
    pub typ: u16,
    pub length: u32,
    pub unused: u32,
}

#[derive(Clone, Copy, Debug)]
#[repr(u16)]
pub enum XarNotarizationTrailerType {
    Invalid = 0,
    Terminator = 1,
    Ticket = 2,
}

/// Obtain the notarization trailer data for a XAR archive.
///
/// The trailer data consists of a [XarNotarizationTrailer] of type `Terminator`
/// to denote the end of XAR content followed by the raw ticket data followed by a
/// [XarNotarizationTrailer] with type `Ticket`. Essentially, a reader can look for
/// a ticket trailer at the end of the file then quickly seek to the beginning of
/// ticket data.
pub fn xar_notarization_trailer(ticket_data: &[u8]) -> Result<Vec<u8>, AppleCodesignError> {
    let terminator = XarNotarizationTrailer {
        magic: XAR_NOTARIZATION_TRAILER_MAGIC,
        version: 1,
        typ: XarNotarizationTrailerType::Terminator as u16,
        length: 0,
        unused: 0,
    };
    let ticket = XarNotarizationTrailer {
        magic: XAR_NOTARIZATION_TRAILER_MAGIC,
        version: 1,
        typ: XarNotarizationTrailerType::Ticket as u16,
        length: ticket_data.len() as _,
        unused: 0,
    };

    let mut cursor = std::io::Cursor::new(Vec::new());
    cursor.iowrite_with(terminator, scroll::LE)?;
    cursor.write_all(ticket_data)?;
    cursor.iowrite_with(ticket, scroll::LE)?;

    Ok(cursor.into_inner())
}

/// Handles stapling operations.
pub struct Stapler {
    client: Client,
}

impl Stapler {
    /// Construct a new instance with defaults.
    pub fn new() -> Result<Self, AppleCodesignError> {
        Ok(Self {
            client: default_client()?,
        })
    }

    /// Set the HTTP client to use for ticket lookups.
    pub fn set_client(&mut self, client: Client) {
        self.client = client;
    }

    /// Look up a notarization ticket for an app bundle.
    ///
    /// This will resolve the notarization ticket record name from the contents
    /// of the bundle then attempt to look up that notarization ticket against
    /// Apple's servers.
    ///
    /// This errors if there is a problem deriving the notarization ticket record name
    /// or if a failure occurs when looking up the notarization ticket. This can include
    /// a notarization ticket not existing for the requested record.
    pub fn lookup_ticket_for_app_bundle(
        &self,
        bundle: &DirectoryBundle,
    ) -> Result<Vec<u8>, AppleCodesignError> {
        let record_name = record_name_from_app_bundle(bundle)?;

        let response = lookup_notarization_ticket(&self.client, &record_name)?;

        let ticket_data = response.signed_ticket(&record_name)?;

        Ok(ticket_data)
    }

    /// Attempt to staple a bundle by obtaining a notarization ticket automatically.
    pub fn staple_bundle(&self, bundle: &DirectoryBundle) -> Result<(), AppleCodesignError> {
        warn!(
            "attempting to find notarization ticket for bundle at {}",
            bundle.root_dir().display()
        );
        let ticket_data = self.lookup_ticket_for_app_bundle(bundle)?;
        staple_ticket_to_bundle(bundle, &ticket_data)?;

        Ok(())
    }

    /// Lookup ticket data for a XAR archive (e.g. a `.pkg` file).
    pub fn lookup_ticket_for_xar<R: Read + Seek + Sized + Debug>(
        &self,
        reader: &mut XarReader<R>,
    ) -> Result<Vec<u8>, AppleCodesignError> {
        let mut digest = reader.checksum_data()?;
        digest.truncate(20);
        let digest = hex::encode(digest);

        let digest_type = DigestType::try_from(reader.table_of_contents().checksum.style)?;
        let digest_type: u8 = digest_type.into();

        let record_name = format!("2/{}/{}", digest_type, digest);

        let response = lookup_notarization_ticket(&self.client, &record_name)?;

        response.signed_ticket(&record_name)
    }

    /// Staple a XAR archive.
    ///
    /// Takes the handle to a readable, writable, and seekable object.
    ///
    /// The stream will be opened as a XAR file. If a ticket is found, that ticket
    /// will be appended to the end of the file.
    pub fn staple_xar<F: Read + Write + Seek + Sized + Debug>(
        &self,
        mut xar: XarReader<F>,
    ) -> Result<(), AppleCodesignError> {
        let ticket_data = self.lookup_ticket_for_xar(&mut xar)?;

        warn!("found notarization ticket; proceeding with stapling");

        let mut fh = xar.into_inner();

        // As a convenience, we look for an existing ticket trailer so we can tell
        // the user we're effectively overwriting it. We could potentially try to
        // delete or overwrite the old trailer. BUt it is just easier to append,
        // as a writer likely only looks for the ticket trailer at the tail end
        // of the file.
        let trailer_size = 16;
        fh.seek(SeekFrom::End(-trailer_size))?;

        let trailer = fh.ioread_with::<XarNotarizationTrailer>(scroll::LE)?;
        if trailer.magic == XAR_NOTARIZATION_TRAILER_MAGIC {
            let trailer_type = match trailer.typ {
                x if x == XarNotarizationTrailerType::Invalid as u16 => "invalid",
                x if x == XarNotarizationTrailerType::Ticket as u16 => "ticket",
                x if x == XarNotarizationTrailerType::Terminator as u16 => "terminator",
                _ => "unknown",
            };

            warn!("found an existing XAR trailer of type {}", trailer_type);
            warn!("this existing trailer will be preserved and will likely be ignored");
        }

        let trailer = xar_notarization_trailer(&ticket_data)?;

        warn!(
            "stapling notarization ticket trailer ({} bytes) to end of XAR",
            trailer.len()
        );
        fh.write_all(&trailer)?;

        Ok(())
    }

    /// Attempt to staple an entity at a given filesystem path.
    ///
    /// The path will be modified on successful stapling operation.
    pub fn staple_path(&self, path: impl AsRef<Path>) -> Result<(), AppleCodesignError> {
        let path = path.as_ref();
        warn!("attempting to staple {}", path.display());

        match PathType::from_path(path)? {
            PathType::MachO => {
                error!("cannot staple Mach-O binaries");
                Err(AppleCodesignError::StapleUnsupportedPath(
                    path.to_path_buf(),
                ))
            }
            PathType::Dmg => Err(AppleCodesignError::Unimplemented("DMG stapling")),
            PathType::Bundle => {
                warn!("activating bundle stapling mode");
                let bundle = DirectoryBundle::new_from_path(path)
                    .map_err(AppleCodesignError::DirectoryBundle)?;
                self.staple_bundle(&bundle)
            }
            PathType::Xar => {
                warn!("activating XAR stapling mode");
                let xar = XarReader::new(File::options().read(true).write(true).open(path)?)?;
                self.staple_xar(xar)
            }
            PathType::Other => Err(AppleCodesignError::StapleUnsupportedPath(
                path.to_path_buf(),
            )),
        }
    }
}
