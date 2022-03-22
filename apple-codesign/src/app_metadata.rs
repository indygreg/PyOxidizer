// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! App metadata profiles

This module provides definitions of the App Metadata Profile data format.
See also <https://help.apple.com/asc/appsspec/#/itc5c07bfa5b>.

Apple's official documentation can be a bit lacking in specifics. You
can find the templates that Apple's own tools use within the
`AppStoreService.framework`. This framework can often be found in
`/Applications/Transporter.app` or `Applications/Xcode.app` in
files like `devidplus-metadata-template.xml`.

The types in this module are likely incomplete. Features are added as support
is needed.
*/

use {
    std::io::Write,
    xml::{
        common::XmlVersion,
        writer::{EmitterConfig, Error as EmitterError, EventWriter, XmlEvent},
    },
};

/// Represents a `<package>` in an App Metadata Profile.
#[derive(Clone, Debug)]
pub struct Package {
    pub software_assets: SoftwareAssets,
}

impl Package {
    /// Convert the instance to XML.
    ///
    /// The bytes constituting the produced XML will be returned.
    ///
    /// The XML is pretty-printed.
    pub fn to_xml(&self) -> Result<Vec<u8>, EmitterError> {
        let config = EmitterConfig::new().perform_indent(true);

        let mut emitter = config.create_writer(std::io::BufWriter::new(vec![]));
        self.write_xml(&mut emitter)?;

        emitter
            .into_inner()
            .into_inner()
            .map_err(|e| EmitterError::Io(e.into()))
    }

    pub fn write_xml<W: Write>(&self, writer: &mut EventWriter<W>) -> Result<(), EmitterError> {
        writer.write(XmlEvent::StartDocument {
            version: XmlVersion::Version10,
            encoding: Some("utf-8"),
            standalone: None,
        })?;

        writer.write(
            XmlEvent::start_element("package")
                .attr("version", "software5.9")
                .ns("", "http://apple.com/itunes/importer"),
        )?;

        self.software_assets.write_xml(writer)?;

        writer.write(XmlEvent::end_element().name("package"))?;

        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct SoftwareAssets {
    pub app_platform: String,
    pub device_id: Option<String>,
    pub primary_bundle_identifier: String,
    pub asset: Asset,
}

impl SoftwareAssets {
    pub fn write_xml<W: Write>(&self, writer: &mut EventWriter<W>) -> Result<(), EmitterError> {
        let e = XmlEvent::start_element("software_assets")
            .attr("app_platform", &self.app_platform)
            .attr("primary_bundle_identifier", &self.primary_bundle_identifier);
        let e = if let Some(id) = &self.device_id {
            e.attr("device_id", id)
        } else {
            e
        };

        writer.write(e)?;

        self.asset.write_xml(writer)?;

        writer.write(XmlEvent::end_element().name("software_assets"))?;

        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct Asset {
    pub typ: String,
    pub data_files: Vec<DataFile>,
}

impl Asset {
    pub fn write_xml<W: Write>(&self, writer: &mut EventWriter<W>) -> Result<(), EmitterError> {
        writer.write(XmlEvent::start_element("asset").attr("type", &self.typ))?;
        for df in &self.data_files {
            df.write_xml(writer)?;
        }

        writer.write(XmlEvent::end_element().name("asset"))?;

        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct DataFile {
    pub file_name: String,
    pub checksum_type: String,
    pub checksum_digest: String,
    pub size: u64,
}

impl DataFile {
    pub fn write_xml<W: Write>(&self, writer: &mut EventWriter<W>) -> Result<(), EmitterError> {
        writer.write(XmlEvent::start_element("data_file"))?;

        writer.write(XmlEvent::start_element("file_name"))?;
        writer.write(XmlEvent::characters(&self.file_name))?;
        writer.write(XmlEvent::end_element())?;

        writer.write(XmlEvent::start_element("checksum").attr("type", &self.checksum_type))?;
        writer.write(XmlEvent::characters(&self.checksum_digest))?;
        writer.write(XmlEvent::end_element())?;

        writer.write(XmlEvent::start_element("size"))?;
        writer.write(XmlEvent::characters(&format!("{}", self.size)))?;
        writer.write(XmlEvent::end_element())?;

        writer.write(XmlEvent::end_element().name("data_file"))?;

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use {super::*, anyhow::Result};

    #[test]
    fn write_xml() -> Result<()> {
        let p = Package {
            software_assets: SoftwareAssets {
                app_platform: "platform".into(),
                device_id: Some("device_id".into()),
                primary_bundle_identifier: "pbi".into(),
                asset: Asset {
                    typ: "my_type".into(),
                    data_files: vec![DataFile {
                        file_name: "name".into(),
                        checksum_type: "md5".into(),
                        checksum_digest: "digest".into(),
                        size: 42,
                    }],
                },
            },
        };

        String::from_utf8(p.to_xml()?)?;

        Ok(())
    }
}
