// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {crate::wix::*, anyhow::Result, std::io::Write, xml::EventWriter};

/// Represents a child of the `<Chain>` WiX XML element.
pub enum ChainElement<'a> {
    ExePackage(ExePackage<'a>),
    MsiPackage(MSIPackage<'a>),
}

impl<'a> ChainElement<'a> {
    pub fn write_xml<W: Write>(&self, writer: &mut EventWriter<W>) -> Result<()> {
        match self {
            Self::ExePackage(exe) => exe.write_xml(writer),
            Self::MsiPackage(msi) => msi.write_xml(writer),
        }
    }
}
