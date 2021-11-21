// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Defines primitives in control files.

See https://www.debian.org/doc/debian-policy/ch-controlfields.html
for the canonical source of truth for how control files work.
*/

use {
    std::{
        borrow::Cow,
        io::{BufRead, Write},
    },
    thiserror::Error,
};

#[derive(Debug, Error)]
pub enum ControlError {
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("parse error: {0}")]
    ParseError(String),
}

/// A field value in a control file.
#[derive(Clone, Debug, PartialEq, PartialOrd)]
pub enum ControlFieldValue<'a> {
    Simple(Cow<'a, str>),
    Folded(Cow<'a, str>),
    Multiline(Cow<'a, str>),
}

impl<'a> ControlFieldValue<'a> {
    /// Obtain the field value as a [&str].
    ///
    /// The raw stored value is returned. For multiline variants, lines will have leading
    /// whitespace.
    pub fn as_str(&self) -> &str {
        match self {
            Self::Simple(v) => v,
            Self::Folded(v) => v,
            Self::Multiline(v) => v,
        }
    }

    /// Obtain an iterator over string values in this field.
    ///
    /// [Self::Simple] variants will emit a single item.
    ///
    /// [Self::Folded] and [Self::Multiline] may emit multiple items.
    ///
    /// For variants stored as multiple lines, leading whitespace will be trimmed, as necessary.
    pub fn iter_values(&self) -> Box<(dyn Iterator<Item = &str> + '_)> {
        match self {
            Self::Simple(v) => Box::new([v.as_ref()].into_iter()),
            Self::Folded(values) => Box::new(values.lines().map(|x| x.trim_start())),
            Self::Multiline(values) => Box::new(values.lines().map(|x| x.trim_start())),
        }
    }

    /// Obtain an iterator over words in the string value.
    ///
    /// The result may be non-meaningful for multiple line variants.
    pub fn iter_value_words(&self) -> Box<(dyn Iterator<Item = &str> + '_)> {
        Box::new(self.as_str().split_ascii_whitespace())
    }

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

    /// Obtain the value as a [&str].
    ///
    /// The value's original file formatting (including newlines and leading whitespace)
    /// is included.
    pub fn value_str(&self) -> &str {
        self.value.as_str()
    }

    /// Obtain an iterator over string values in this field.
    ///
    /// See [ControlField::iter_values] for behavior.
    pub fn iter_values(&self) -> Box<(dyn Iterator<Item = &str> + '_)> {
        self.value.iter_values()
    }

    /// Obtain an iterator over string words in this field.
    ///
    /// See [ControlField::iter_value_words] for behavior.
    pub fn iter_value_words(&self) -> Box<(dyn Iterator<Item = &str> + '_)> {
        self.value.iter_value_words()
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
    /// Whether the paragraph is empty.
    ///
    /// Empty is defined by the lack of any fields.
    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }

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

    /// Iterate over fields in this paragraph.
    pub fn iter_fields(&self) -> impl Iterator<Item = &ControlField<'a>> {
        self.fields.iter()
    }

    /// Obtain the first field with a given name in this paragraph.
    pub fn first_field(&self, name: &str) -> Option<&ControlField> {
        self.fields.iter().find(|f| f.name == name)
    }

    /// Obtain a mutable reference to the first field with a given name.
    pub fn first_field_mut(&mut self, name: &str) -> Option<&'a mut ControlField> {
        self.fields.iter_mut().find(|f| f.name == name)
    }

    /// Obtain the raw string value of the first occurrence of a named field.
    pub fn first_field_str(&self, name: &str) -> Option<&str> {
        self.first_field(name).map(|f| f.value_str())
    }

    /// Obtain an iterator of values of the first occurrence of a named field.
    pub fn first_field_iter_values(
        &self,
        name: &str,
    ) -> Option<Box<(dyn Iterator<Item = &str> + '_)>> {
        self.first_field(name).map(|f| f.iter_values())
    }

    /// Obtain an iterator of words in the first occurrence of a named field.
    pub fn first_field_iter_value_words(
        &self,
        name: &str,
    ) -> Option<Box<(dyn Iterator<Item = &str> + '_)>> {
        self.first_field(name).map(|f| f.iter_value_words())
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
                    if !current_paragraph.is_empty() {
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

        if !current_paragraph.is_empty() {
            paragraphs.push(current_paragraph);
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
    fn parse_paragraph_release() -> Result<(), ControlError> {
        let mut reader = std::io::Cursor::new(include_bytes!("testdata/release-debian-bullseye"));
        let cf = ControlFile::parse_reader(&mut reader)?;
        assert_eq!(cf.paragraphs.len(), 1);

        let p = &cf.paragraphs[0];
        assert_eq!(p.fields.len(), 14);

        assert!(p.has_field("Origin"));
        assert!(p.has_field("Version"));
        assert!(!p.has_field("Missing"));

        assert!(p.first_field("Version").is_some());

        let fields = &p.fields;
        assert_eq!(fields[0].name, "Origin");
        assert_eq!(fields[0].value, ControlFieldValue::Simple("Debian".into()));

        assert_eq!(fields[3].name, "Version");
        assert_eq!(fields[3].value, ControlFieldValue::Simple("11.1".into()));

        assert!(matches!(
            p.first_field("MD5Sum").unwrap().value,
            ControlFieldValue::Folded(_)
        ));
        assert!(matches!(
            p.first_field("SHA256").unwrap().value,
            ControlFieldValue::Folded(_)
        ));

        assert_eq!(fields[0].iter_values().collect::<Vec<_>>(), vec!["Debian"]);

        let values = p
            .first_field_iter_values("MD5Sum")
            .unwrap()
            .collect::<Vec<_>>();
        assert_eq!(values.len(), 600);
        assert_eq!(
            values[0],
            "7fdf4db15250af5368cc52a91e8edbce   738242 contrib/Contents-all"
        );
        assert_eq!(
            values[1],
            "cbd7bc4d3eb517ac2b22f929dfc07b47    57319 contrib/Contents-all.gz"
        );
        assert_eq!(
            values[599],
            "e3830f6fc5a946b5a5b46e8277e1d86f    80488 non-free/source/Sources.xz"
        );

        let values = p
            .first_field_iter_values("SHA256")
            .unwrap()
            .collect::<Vec<_>>();
        assert_eq!(values.len(), 600);
        assert_eq!(
            values[0],
            "3957f28db16e3f28c7b34ae84f1c929c567de6970f3f1b95dac9b498dd80fe63   738242 contrib/Contents-all",
        );
        assert_eq!(
            values[1],
            "3e9a121d599b56c08bc8f144e4830807c77c29d7114316d6984ba54695d3db7b    57319 contrib/Contents-all.gz",
        );
        assert_eq!(values[599], "30f3f996941badb983141e3b29b2ed5941d28cf81f9b5f600bb48f782d386fc7    80488 non-free/source/Sources.xz");

        Ok(())
    }

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
