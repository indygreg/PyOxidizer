// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Reading support for Apple flat package (`.pkg`) installers.

use {
    crate::{
        component_package::ComponentPackageReader, distribution::Distribution, Error, PkgResult,
    },
    apple_xar::reader::XarReader,
    std::{
        fmt::Debug,
        io::{Cursor, Read, Seek},
    },
};

/// The type of a flat package.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PkgFlavor {
    /// A *component* installer.
    ///
    /// This consists of a single component.
    Component,

    /// A *product* installer.
    ///
    /// This consists of multiple components, described by a `Distribution` file.
    Product,
}

/// Read-only interface to a single flat package XAR archive.
pub struct PkgReader<R: Read + Seek + Sized + Debug> {
    xar: XarReader<R>,
    flavor: PkgFlavor,
}

impl<R: Read + Seek + Sized + Debug> PkgReader<R> {
    /// Construct an instance from a reader.
    ///
    /// The reader will read the contents of a XAR archive. This is likely
    /// a `.pkg` file.
    pub fn new(reader: R) -> PkgResult<Self> {
        let xar = XarReader::new(reader)?;

        let flavor = if xar.find_file("Distribution")?.is_some() {
            PkgFlavor::Product
        } else {
            PkgFlavor::Component
        };

        Ok(Self { xar, flavor })
    }

    /// Return the inner reader, consuming self.
    pub fn into_inner(self) -> XarReader<R> {
        self.xar
    }

    /// Obtain the flavor of the flat package.
    pub fn flavor(&self) -> PkgFlavor {
        self.flavor
    }

    /// Obtain the parsed `Distribution` XML file describing the installer.
    ///
    /// Not all flat packages have a `Distribution` file, so this may resolve to
    /// `None`.
    pub fn distribution(&mut self) -> PkgResult<Option<Distribution>> {
        if let Some(xml_data) = self.xar.get_file_data_from_path("Distribution")? {
            Ok(Some(Distribution::from_reader(Cursor::new(xml_data))?))
        } else {
            Ok(None)
        }
    }

    /// Attempt to resolve a component given a path prefix.
    ///
    /// If a component is found under a given path, `Some` is returned. Otherwise
    /// `None` is returned.
    ///
    /// A *found* component is defined by the presence of 1 or more well-known files
    /// in components (`Bom`, `PackageInfo`, `Payload`, etc).
    fn resolve_component(
        &mut self,
        path_prefix: &str,
    ) -> PkgResult<Option<ComponentPackageReader>> {
        let prefix = if path_prefix.is_empty() {
            "".to_string()
        } else {
            format!("{}/", path_prefix)
        };

        let mut bom_data = None;
        let mut package_info_data = None;
        let mut payload_data = None;
        let mut scripts_data = None;

        for (filename, file) in self
            .xar
            .files()?
            .into_iter()
            .filter(|(filename, _)| filename.starts_with(&prefix))
        {
            let mut data = Vec::<u8>::with_capacity(file.size.unwrap_or(0) as _);
            self.xar
                .write_file_data_decoded_from_file(&file, &mut data)?;

            let filename = filename.strip_prefix(&prefix).expect("prefix should match");

            match filename {
                "Bom" => {
                    bom_data = Some(data);
                }
                "PackageInfo" => {
                    package_info_data = Some(data);
                }
                "Payload" => {
                    payload_data = Some(data);
                }
                "Scripts" => {
                    scripts_data = Some(data);
                }
                _ => {}
            }
        }

        if bom_data.is_some()
            || package_info_data.is_some()
            || payload_data.is_some()
            || scripts_data.is_some()
        {
            Ok(Some(ComponentPackageReader::from_file_data(
                bom_data,
                package_info_data,
                payload_data,
                scripts_data,
            )?))
        } else {
            Ok(None)
        }
    }

    /// Obtain the *root* component in this installer.
    ///
    /// This will only return a component of this is a single component installer, not
    /// a product installer.
    pub fn root_component(&mut self) -> PkgResult<Option<ComponentPackageReader>> {
        self.resolve_component("")
    }

    /// Obtain *component package* instances in this flat package.
    ///
    /// *Component packages* are the individual installable packages contained
    /// in a flat package archive.
    ///
    /// If this is a single component installer, only a single instance will be
    /// returned. For product installers, all components are returned.
    pub fn component_packages(&mut self) -> PkgResult<Vec<ComponentPackageReader>> {
        // TODO obtain instances from Distribution XML instead of scanning filenames.
        let components = self
            .xar
            .files()?
            .into_iter()
            .filter_map(|(filename, _)| {
                if filename.ends_with(".pkg") && !filename.contains('/') {
                    Some(filename)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        let mut res = vec![];

        for component in components {
            res.push(
                self.resolve_component(&component)?
                    .ok_or(Error::ComponentResolution)?,
            );
        }

        Ok(res)
    }
}
