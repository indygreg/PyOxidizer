// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    anyhow::Result,
    std::{borrow::Cow, io::Write},
    xml::writer::{EventWriter, XmlEvent},
};

/// Represents an `<MsiPackage>` WiX XML element.
#[derive(Default)]
pub struct MSIPackage<'a> {
    pub id: Option<Cow<'a, str>>,
    pub display_name: Option<Cow<'a, str>>,
    pub force_per_machine: Option<Cow<'a, str>>,
    pub compressed: Option<Cow<'a, str>>,
    pub source_file: Option<Cow<'a, str>>,
    pub display_internal_ui: Option<Cow<'a, str>>,
    pub install_condition: Option<Cow<'a, str>>,
}

impl<'a> MSIPackage<'a> {
    pub fn write_xml<W: Write>(&self, writer: &mut EventWriter<W>) -> Result<()> {
        let e = XmlEvent::start_element("MsiPackage");

        let e = if let Some(value) = &self.id {
            e.attr("Id", value)
        } else {
            e
        };

        let e = if let Some(value) = &self.display_name {
            e.attr("DisplayName", value)
        } else {
            e
        };

        let e = if let Some(value) = &self.force_per_machine {
            e.attr("ForcePerMachine", value)
        } else {
            e
        };

        let e = if let Some(value) = &self.compressed {
            e.attr("Compressed", value)
        } else {
            e
        };

        let e = if let Some(value) = &self.source_file {
            e.attr("SourceFile", value)
        } else {
            e
        };

        let e = if let Some(value) = &self.display_internal_ui {
            e.attr("DisplayInternalUI", value)
        } else {
            e
        };

        let e = if let Some(value) = &self.install_condition {
            e.attr("InstallCondition", value)
        } else {
            e
        };

        writer.write(e)?;
        writer.write(XmlEvent::end_element().name("MsiPackage"))?;

        Ok(())
    }
}
