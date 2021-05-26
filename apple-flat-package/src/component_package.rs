// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Interface to component packages, installable units within flat packages.

use {
    crate::{package_info::PackageInfo, PkgResult},
    cpio_archive::ChainedCpioReader,
    std::io::{Cursor, Read},
};

const GZIP_HEADER: [u8; 3] = [0x1f, 0x8b, 0x08];

/// Attempt to decode the compressed content of an archive file.
///
/// The content can be compressed with various formats. This attempts to
/// sniff them and apply an appropriate decompressor.
fn decode_archive(data: Vec<u8>) -> PkgResult<Box<dyn Read>> {
    if data.len() > 3 && data[0..3] == GZIP_HEADER {
        Ok(Box::new(flate2::read::GzDecoder::new(Cursor::new(data))) as Box<dyn Read>)
    } else {
        Ok(Box::new(Cursor::new(data)) as Box<dyn Read>)
    }
}

/// Type alias representing a generic reader for a cpio archive.
pub type CpioReader = Box<ChainedCpioReader<Box<dyn Read>>>;

fn cpio_reader(data: &[u8]) -> PkgResult<CpioReader> {
    let decoder = decode_archive(data.to_vec())?;
    Ok(cpio_archive::reader(decoder)?)
}

/// Read-only interface for a single *component package*.
pub struct ComponentPackageReader {
    bom: Option<Vec<u8>>,
    package_info: Option<PackageInfo>,
    payload: Option<Vec<u8>>,
    scripts: Option<Vec<u8>>,
}

impl ComponentPackageReader {
    /// Construct an instance with raw file data backing different files.
    pub fn from_file_data(
        bom: Option<Vec<u8>>,
        package_info: Option<Vec<u8>>,
        payload: Option<Vec<u8>>,
        scripts: Option<Vec<u8>>,
    ) -> PkgResult<Self> {
        let package_info = if let Some(data) = package_info {
            Some(PackageInfo::from_reader(Cursor::new(data))?)
        } else {
            None
        };

        Ok(Self {
            bom,
            package_info,
            payload,
            scripts,
        })
    }

    /// Obtained the contents of the `Bom` file.
    pub fn bom(&self) -> Option<&[u8]> {
        self.bom.as_ref().map(|x| x.as_ref())
    }

    /// Obtain the parsed `PackageInfo` XML file.
    pub fn package_info(&self) -> Option<&PackageInfo> {
        self.package_info.as_ref()
    }

    /// Obtain a reader for the `Payload` cpio archive.
    pub fn payload_reader(&self) -> PkgResult<Option<CpioReader>> {
        if let Some(payload) = &self.payload {
            Ok(Some(cpio_reader(payload)?))
        } else {
            Ok(None)
        }
    }

    /// Obtain a reader for the `Scripts` cpio archive.
    pub fn scripts_reader(&self) -> PkgResult<Option<CpioReader>> {
        if let Some(data) = &self.scripts {
            Ok(Some(cpio_reader(data)?))
        } else {
            Ok(None)
        }
    }
}
