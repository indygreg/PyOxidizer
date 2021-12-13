// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Source package control files. */

use {
    crate::{
        control::{ControlFile, ControlParagraph},
        error::{DebianError, Result},
    },
    std::io::BufRead,
};

/// Represents a `debian/control` file.
///
/// Specified at <https://www.debian.org/doc/debian-policy/ch-controlfields.html#source-package-control-files-debian-control>.
#[derive(Default)]
pub struct SourceControlFile<'a> {
    general: ControlParagraph<'a>,
    binaries: Vec<ControlParagraph<'a>>,
}

impl<'a> SourceControlFile<'a> {
    /// Construct an instance from an iterable of [ControlParagraph].
    pub fn from_paragraphs(
        mut paragraphs: impl Iterator<Item = ControlParagraph<'a>>,
    ) -> Result<Self> {
        let general = paragraphs.next().ok_or_else(|| {
            DebianError::ControlParseError(
                "no general paragraph in source control file".to_string(),
            )
        })?;

        let binaries = paragraphs.collect::<Vec<_>>();

        Ok(Self { general, binaries })
    }

    /// Construct an instance by parsing a control file from a reader.
    pub fn parse_reader<R: BufRead>(reader: &mut R) -> Result<Self> {
        let control = ControlFile::parse_reader(reader)?;

        Self::from_paragraphs(control.paragraphs().map(|x| x.to_owned()))
    }

    /// Construct an instance by parsing a string.
    pub fn parse_str(s: &str) -> Result<Self> {
        let mut reader = std::io::BufReader::new(s.as_bytes());
        Self::parse_reader(&mut reader)
    }

    /// Obtain a handle on the general paragraph.
    pub fn general_paragraph(&self) -> &ControlParagraph<'a> {
        &self.general
    }

    /// Obtain an iterator over paragraphs defining binaries.
    pub fn binary_paragraphs(&self) -> impl Iterator<Item = &ControlParagraph<'a>> {
        self.binaries.iter()
    }
}
