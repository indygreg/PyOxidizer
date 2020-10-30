// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Defines primitives in control files.

See https://www.debian.org/doc/debian-policy/ch-controlfields.html
for the canonical source of truth for how control files work.
*/

use std::{
    borrow::Cow,
    io::{BufRead, Write},
};

pub enum ControlError {
    UnknownFieldKey(String),
    IOError(std::io::Error),
    ParseError(String),
}

impl From<std::io::Error> for ControlError {
    fn from(e: std::io::Error) -> Self {
        Self::IOError(e)
    }
}

/// A field value in a control file.
#[derive(Clone, Debug)]
pub enum ControlFieldValue<'a> {
    Simple(Cow<'a, str>),
    Folded(Cow<'a, str>),
    Multiline(Cow<'a, str>),
}

impl<'a> ControlFieldValue<'a> {
    /// Write this value to a writer.
    pub fn write<W: Write>(&self, writer: &mut W) -> std::io::Result<()> {
        let data = match self {
            Self::Simple(v) => v,
            Self::Folded(v) => v,
            Self::Multiline(v) => v,
        };

        writer.write_all(data.as_bytes())
    }
}

impl<'a> ControlFieldValue<'a> {
    pub fn from_field(
        key: &str,
        value: Cow<'a, str>,
    ) -> Result<ControlFieldValue<'a>, ControlError> {
        // TODO should we examine content of string and derive type from that
        // instead? Perhaps we should store a series of lines instead of a raw
        // string?
        match key {
            // Ordered list from https://www.debian.org/doc/debian-policy/ch-controlfields.html#list-of-fields.
            "Source" => Ok(ControlFieldValue::Simple(value)),
            "Maintainer" => Ok(ControlFieldValue::Simple(value)),
            "Uploaders" => Ok(ControlFieldValue::Folded(value)),
            "Changed-By" => Ok(ControlFieldValue::Simple(value)),
            "Section" => Ok(ControlFieldValue::Simple(value)),
            "Priority" => Ok(ControlFieldValue::Simple(value)),
            "Package" => Ok(ControlFieldValue::Simple(value)),
            "Architecture" => Ok(ControlFieldValue::Simple(value)),
            "Essential" => Ok(ControlFieldValue::Simple(value)),
            "Depends" => Ok(ControlFieldValue::Folded(value)),
            "Pre-Depends" => Ok(ControlFieldValue::Folded(value)),
            "Recommends" => Ok(ControlFieldValue::Folded(value)),
            "Suggests" => Ok(ControlFieldValue::Folded(value)),
            "Breaks" => Ok(ControlFieldValue::Folded(value)),
            "Conflicts" => Ok(ControlFieldValue::Folded(value)),
            "Provides" => Ok(ControlFieldValue::Folded(value)),
            "Replaces" => Ok(ControlFieldValue::Folded(value)),
            "Enhances" => Ok(ControlFieldValue::Folded(value)),
            "Standards-Version" => Ok(ControlFieldValue::Simple(value)),
            "Version" => Ok(ControlFieldValue::Simple(value)),
            "Description" => Ok(ControlFieldValue::Multiline(value)),
            "Distribution" => Ok(ControlFieldValue::Simple(value)),
            "Date" => Ok(ControlFieldValue::Simple(value)),
            "Format" => Ok(ControlFieldValue::Simple(value)),
            "Urgency" => Ok(ControlFieldValue::Simple(value)),
            "Changes" => Ok(ControlFieldValue::Multiline(value)),
            "Binary" => Ok(ControlFieldValue::Folded(value)),
            "Files" => Ok(ControlFieldValue::Folded(value)),
            "Closes" => Ok(ControlFieldValue::Simple(value)),
            "Homepage" => Ok(ControlFieldValue::Simple(value)),
            "Checksums-Sha1" => Ok(ControlFieldValue::Multiline(value)),
            "Checksums-Sha256" => Ok(ControlFieldValue::Multiline(value)),
            "DM-Upload-Allowed" => Ok(ControlFieldValue::Simple(value)),
            "Vcs-Browser" => Ok(ControlFieldValue::Simple(value)),
            "Vcs-Arch" => Ok(ControlFieldValue::Simple(value)),
            "Vcs-Bzr" => Ok(ControlFieldValue::Simple(value)),
            "Vcs-Cvs" => Ok(ControlFieldValue::Simple(value)),
            "Vcs-Darcs" => Ok(ControlFieldValue::Simple(value)),
            "Vcs-Git" => Ok(ControlFieldValue::Simple(value)),
            "Vcs-Hg" => Ok(ControlFieldValue::Simple(value)),
            "Vcs-Mtn" => Ok(ControlFieldValue::Simple(value)),
            "Vcs-Svn" => Ok(ControlFieldValue::Simple(value)),
            "Package-List" => Ok(ControlFieldValue::Multiline(value)),
            "Package-Type" => Ok(ControlFieldValue::Simple(value)),
            "Dgit" => Ok(ControlFieldValue::Folded(value)),
            "Testsuite" => Ok(ControlFieldValue::Simple(value)),
            "Rules-Requires-Root" => Ok(ControlFieldValue::Simple(value)),

            // Additional from https://www.debian.org/doc/debian-policy/ch-relationships.html#relationships-between-source-and-binary-packages-build-depends-build-depends-indep-build-depends-arch-build-conflicts-build-conflicts-indep-build-conflicts-arch.
            "Build-Depends" => Ok(ControlFieldValue::Folded(value)),
            "Build-Depends-Indep" => Ok(ControlFieldValue::Folded(value)),
            "Build-Depends-Arch" => Ok(ControlFieldValue::Folded(value)),
            "Build-Conflicts" => Ok(ControlFieldValue::Folded(value)),
            "Build-Conflicts-Indep" => Ok(ControlFieldValue::Folded(value)),
            "Build-Conflicts-Arch" => Ok(ControlFieldValue::Folded(value)),

            // Additional fields from https://www.debian.org/doc/debian-policy/ch-controlfields.html#source-package-control-files-debian-control.
            "Built-Using" => Ok(ControlFieldValue::Simple(value)),

            // Additional fields from https://www.debian.org/doc/debian-policy/ch-controlfields.html#binary-package-control-files-debian-control.
            "Installed-Size" => Ok(ControlFieldValue::Simple(value)),

            _ => Err(ControlError::UnknownFieldKey(key.to_string())),
        }
    }
}

/// A field in a control file.
#[derive(Clone, Debug)]
pub struct ControlField<'a> {
    name: Cow<'a, str>,
    value: ControlFieldValue<'a>,
}

impl<'a> ControlField<'a> {
    /// Construct an instance from a field name and typed value.
    pub fn new(name: Cow<'a, str>, value: ControlFieldValue<'a>) -> Self {
        Self { name, value }
    }

    /// Construct a field from a named key and string value.
    ///
    /// The type of the field value will be derived from the key name.
    ///
    /// Unknown keys will be rejected.
    pub fn from_string_value(key: Cow<'a, str>, value: Cow<'a, str>) -> Result<Self, ControlError> {
        let value = ControlFieldValue::from_field(key.as_ref(), value)?;

        Ok(Self { name: key, value })
    }

    /// Write the contents of this field to a writer.
    pub fn write<W: Write>(&self, writer: &mut W) -> std::io::Result<()> {
        writer.write_all(self.name.as_bytes())?;
        writer.write_all(b": ")?;
        self.value.write(writer)?;
        writer.write_all(b"\n")
    }
}

/// A paragraph in a control file.
///
/// A paragraph is an ordered series of control fields.
#[derive(Clone, Debug, Default)]
pub struct ControlParagraph<'a> {
    fields: Vec<ControlField<'a>>,
}

impl<'a> ControlParagraph<'a> {
    /// Add a `ControlField` to this instance.
    pub fn add_field(&mut self, field: ControlField<'a>) {
        self.fields.push(field);
    }

    /// Add a field defined via strings.
    pub fn add_field_from_string(
        &mut self,
        name: Cow<'a, str>,
        value: Cow<'a, str>,
    ) -> Result<(), ControlError> {
        self.fields
            .push(ControlField::from_string_value(name, value)?);
        Ok(())
    }

    /// Whether a named field is present in this paragraph.
    pub fn has_field(&self, name: &str) -> bool {
        self.fields.iter().any(|f| f.name == name)
    }

    /// Obtain the first field with a given name in this paragraph.
    pub fn get_field(&self, name: &str) -> Option<&ControlField> {
        self.fields.iter().find(|f| f.name == name)
    }

    /// Obtain a mutable reference to the first field with a given name.
    pub fn get_field_mut(&mut self, name: &str) -> Option<&'a mut ControlField> {
        self.fields.iter_mut().find(|f| f.name == name)
    }

    /// Serialize the paragraph to a writer.
    pub fn write<W: Write>(&self, writer: &mut W) -> std::io::Result<()> {
        for field in &self.fields {
            field.write(writer)?;
        }

        writer.write_all(b"\n")
    }
}

/// A debian control file.
///
/// A control file is an ordered series of paragraphs.
#[derive(Clone, Debug, Default)]
pub struct ControlFile<'a> {
    paragraphs: Vec<ControlParagraph<'a>>,
}

impl<'a> ControlFile<'a> {
    /// Construct a new instance by parsing data from a reader.
    pub fn parse_reader<R: BufRead>(reader: &mut R) -> Result<Self, ControlError> {
        let mut paragraphs = Vec::new();
        let mut current_paragraph = ControlParagraph::default();
        let mut current_field: Option<String> = None;

        loop {
            let mut line = String::new();
            let bytes_read = reader.read_line(&mut line)?;

            let is_empty_line = line.trim().is_empty();
            let is_indented = line.starts_with(' ') && line.len() > 1;

            current_field = match (is_empty_line, current_field, is_indented) {
                // We have a field on the stack and got an unindented line. This
                // must be the beginning of a new field. Flush the current field.
                (_, Some(v), false) => {
                    let mut parts = v.splitn(2, ':');

                    let name = parts.next().ok_or_else(|| {
                        ControlError::ParseError(format!(
                            "error parsing line '{}'; missing colon",
                            line
                        ))
                    })?;
                    let value = parts
                        .next()
                        .ok_or_else(|| {
                            ControlError::ParseError(format!(
                                "error parsing field '{}'; could not detect value",
                                v
                            ))
                        })?
                        .trim();

                    current_paragraph.add_field_from_string(
                        Cow::Owned(name.to_string()),
                        Cow::Owned(value.to_string()),
                    )?;

                    if is_empty_line {
                        None
                    } else {
                        Some(line)
                    }
                }

                // If we're an empty line and no fields is on the stack, we're at
                // the end of the paragraph with no field to flush. Just flush the
                // paragraph if it is non-empty.
                (true, _, _) => {
                    if !current_paragraph.fields.is_empty() {
                        paragraphs.push(current_paragraph);
                        current_paragraph = ControlParagraph::default();
                    }

                    None
                }
                // We got a non-empty line and no field is currently being
                // processed. This must be the start of a new field.
                (false, None, _) => Some(line),
                // We have a field on the stack and got an indented line. This
                // must be a field value continuation. Add it to the current
                // field.
                (false, Some(v), true) => Some(v + &line),
            };

            // .read_line() indicates EOF by Ok(0).
            if bytes_read == 0 {
                break;
            }
        }

        Ok(Self { paragraphs })
    }

    /// Parse a control file from a string.
    pub fn parse_str(s: &str) -> Result<Self, ControlError> {
        let mut reader = std::io::BufReader::new(s.as_bytes());
        Self::parse_reader(&mut reader)
    }

    /// Add a paragraph to this control file.
    pub fn add_paragraph(&mut self, p: ControlParagraph<'a>) {
        self.paragraphs.push(p);
    }

    /// Obtain paragraphs in this control file.
    pub fn paragraphs(&self) -> impl Iterator<Item = &ControlParagraph<'a>> {
        self.paragraphs.iter()
    }

    /// Serialize the control file to a writer.
    pub fn write<W: Write>(&self, writer: &mut W) -> std::io::Result<()> {
        for p in &self.paragraphs {
            p.write(writer)?;
        }

        // Paragraph writer adds additional line break. So no need to
        // add another here.

        Ok(())
    }
}

/// Represents a `debian/control` file.
///
/// Specified at https://www.debian.org/doc/debian-policy/ch-controlfields.html#source-package-control-files-debian-control.
#[derive(Default)]
pub struct SourceControl<'a> {
    general: ControlParagraph<'a>,
    binaries: Vec<ControlParagraph<'a>>,
}

impl<'a> SourceControl<'a> {
    /// Construct an instance by parsing a control file from a reader.
    pub fn parse_reader<R: BufRead>(reader: &mut R) -> Result<Self, ControlError> {
        let control = ControlFile::parse_reader(reader)?;

        let mut paragraphs = control.paragraphs();

        let general = paragraphs
            .next()
            .ok_or_else(|| {
                ControlError::ParseError("no general paragraph in source control file".to_string())
            })?
            .to_owned();

        let binaries = paragraphs.map(|x| x.to_owned()).collect();

        Ok(Self { general, binaries })
    }

    pub fn parse_str(s: &str) -> Result<Self, ControlError> {
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
