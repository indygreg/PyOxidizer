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

#[derive(Debug)]
pub enum ControlError {
    IoError(std::io::Error),
    ParseError(String),
}

impl From<std::io::Error> for ControlError {
    fn from(e: std::io::Error) -> Self {
        Self::IoError(e)
    }
}

impl std::fmt::Display for ControlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::IoError(inner) => write!(f, "I/O error: {}", inner),
            Self::ParseError(msg) => write!(f, "parse error: {}", msg),
        }
    }
}

impl std::error::Error for ControlError {}

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

impl<'a> From<Cow<'a, str>> for ControlFieldValue<'a> {
    fn from(value: Cow<'a, str>) -> Self {
        if value.contains('\n') {
            if value.starts_with(' ') || value.starts_with('\t') {
                ControlFieldValue::Multiline(value)
            } else {
                ControlFieldValue::Folded(value)
            }
        } else {
            ControlFieldValue::Simple(value)
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
        let value = ControlFieldValue::from(value);

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

#[cfg(test)]
mod tests {
    use {super::*, anyhow::Result};

    #[test]
    fn test_parse_system_lists() -> Result<()> {
        let paths = glob::glob("/var/lib/apt/lists/*_Packages")?
            .chain(glob::glob("/var/lib/apt/lists/*_Sources")?)
            .chain(glob::glob("/var/lib/apt/lists/*i18n_Translation-*")?);

        for path in paths {
            let path = path?;

            eprintln!("parsing {}", path.display());
            let fh = std::fs::File::open(&path)?;
            let mut reader = std::io::BufReader::new(fh);

            ControlFile::parse_reader(&mut reader)?;
        }

        Ok(())
    }
}
