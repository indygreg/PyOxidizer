// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Defines primitives in control files.

See <https://www.debian.org/doc/debian-policy/ch-controlfields.html>
for the canonical source of truth for how control files work.
*/

use {
    std::{
        borrow::Cow,
        collections::HashMap,
        io::{BufRead, Write},
    },
    thiserror::Error,
};

#[cfg(feature = "async")]
use {
    futures::{AsyncBufRead, AsyncBufReadExt},
    std::pin::Pin,
};

#[derive(Debug, Error)]
pub enum ControlError {
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("parse error: {0}")]
    ParseError(String),
}

/// A field value in a control file.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
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
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
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
#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
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

    /// Convert this paragraph to a [HashMap].
    ///
    /// Values will be the string normalization of the field value, including newlines and
    /// leading whitespace.
    ///
    /// If a field occurs multiple times, its last value will be recorded in the returned map.
    pub fn as_str_hash_map(&self) -> HashMap<&str, &str> {
        HashMap::from_iter(
            self.fields
                .iter()
                .map(|field| (field.name.as_ref(), field.value_str())),
        )
    }

    /// Serialize the paragraph to a writer.
    pub fn write<W: Write>(&self, writer: &mut W) -> std::io::Result<()> {
        for field in &self.fields {
            field.write(writer)?;
        }

        writer.write_all(b"\n")
    }
}

/// Holds parsing state for Debian control files.
///
/// Instances of this type are essentially fed lines of text and periodically emit
/// [ControlParagraph] instances as they are completed.
#[derive(Clone, Debug, Default)]
pub struct ControlFileParser {
    paragraph: ControlParagraph<'static>,
    field: Option<String>,
}

impl ControlFileParser {
    /// Write a line to the parser.
    ///
    /// If the line terminates an in-progress paragraph, that paragraph will be returned.
    /// Otherwise `Ok(None)` is returned.
    ///
    /// `Err` is returned if the control file in invalid.
    pub fn write_line(
        &mut self,
        line: &str,
    ) -> Result<Option<ControlParagraph<'static>>, ControlError> {
        let is_empty_line = line.trim().is_empty();
        let is_indented = line.starts_with(' ') && line.len() > 1;

        let current_field = self.field.take();

        // Empty lines signify the end of a paragraph. Flush any state.
        if is_empty_line {
            if let Some(field) = current_field {
                self.flush_field(field)?;
            }

            return Ok(if self.paragraph.is_empty() {
                None
            } else {
                let para = self.paragraph.clone();
                self.paragraph = ControlParagraph::default();
                Some(para)
            });
        }

        match (current_field, is_indented) {
            // We have a field on the stack and got an unindented line. This
            // must be the beginning of a new field. Flush the current field.
            (Some(v), false) => {
                self.flush_field(v)?;

                self.field = if is_empty_line {
                    None
                } else {
                    Some(line.to_string())
                };

                Ok(None)
            }

            // We got a non-empty line and no field is currently being
            // processed. This must be the start of a new field.
            (None, _) => {
                self.field = Some(line.to_string());

                Ok(None)
            }
            // We have a field on the stack and got an indented line. This
            // must be a field value continuation. Add it to the current
            // field.
            (Some(v), true) => {
                self.field = Some(v + line);

                Ok(None)
            }
        }
    }

    /// Finish parsing, consuming self.
    ///
    /// If a non-empty paragraph is present in the instance, it will be returned. Else if there
    /// is no unflushed state, None is returned.
    pub fn finish(mut self) -> Result<Option<ControlParagraph<'static>>, ControlError> {
        if let Some(field) = self.field.take() {
            self.flush_field(field)?;
        }

        Ok(if self.paragraph.is_empty() {
            None
        } else {
            Some(self.paragraph)
        })
    }

    fn flush_field(&mut self, v: String) -> Result<(), ControlError> {
        let mut parts = v.splitn(2, ':');

        let name = parts.next().ok_or_else(|| {
            ControlError::ParseError(format!("error parsing line '{}'; missing colon", v))
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

        self.paragraph
            .add_field_from_string(Cow::Owned(name.to_string()), Cow::Owned(value.to_string()))?;

        Ok(())
    }
}

/// A reader for [ControlParagraph].
///
/// Instances are bound to a reader, which is capable of feeding lines into a parser.
///
/// Instances can be consumed as an iterator. Each call into the iterator will attempt to
/// read a full paragraph from the underlying reader.
pub struct ControlParagraphReader<R: BufRead> {
    reader: R,
    parser: Option<ControlFileParser>,
}

impl<R: BufRead> ControlParagraphReader<R> {
    /// Create a new instance bound to a reader.
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            parser: Some(ControlFileParser::default()),
        }
    }

    /// Consumes the instance, returning the original reader.
    pub fn into_inner(self) -> R {
        self.reader
    }

    fn get_next(&mut self) -> Result<Option<ControlParagraph<'static>>, ControlError> {
        let mut parser = self.parser.take().unwrap();

        loop {
            let mut line = String::new();

            let bytes_read = self.reader.read_line(&mut line)?;

            if bytes_read != 0 {
                if let Some(paragraph) = parser.write_line(&line)? {
                    self.parser.replace(parser);
                    return Ok(Some(paragraph));
                }
                // Continue reading.
            } else {
                return if let Some(paragraph) = parser.finish()? {
                    Ok(Some(paragraph))
                } else {
                    Ok(None)
                };
            }
        }
    }
}

impl<R: BufRead> Iterator for ControlParagraphReader<R> {
    type Item = Result<ControlParagraph<'static>, ControlError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.parser.is_none() {
            None
        } else {
            match self.get_next() {
                Ok(Some(para)) => Some(Ok(para)),
                Ok(None) => None,
                Err(e) => Some(Err(e)),
            }
        }
    }
}

/// An asynchronous reader of [ControlParagraph].
///
/// Instances are bound to a reader, which is capable of reading lines.
#[cfg(feature = "async")]
pub struct ControlParagraphAsyncReader<R: AsyncBufRead> {
    reader: Pin<Box<R>>,
    parser: Option<ControlFileParser>,
}

#[cfg(feature = "async")]
impl<R: AsyncBufRead> ControlParagraphAsyncReader<R> {
    /// Create a new instance bound to a reader.
    pub fn new(reader: R) -> Self {
        Self {
            reader: Box::pin(reader),
            parser: Some(ControlFileParser::default()),
        }
    }

    /// Read the next available paragraph from this reader.
    ///
    /// Resolves to [None] on end of input.
    pub async fn read_paragraph(
        &mut self,
    ) -> Result<Option<ControlParagraph<'static>>, ControlError> {
        let mut parser = if let Some(parser) = self.parser.take() {
            parser
        } else {
            return Ok(None);
        };

        loop {
            let mut line = String::new();

            let bytes_read = self.reader.read_line(&mut line).await?;

            if bytes_read != 0 {
                if let Some(paragraph) = parser.write_line(&line)? {
                    self.parser.replace(parser);
                    return Ok(Some(paragraph));
                }
                // Continue reading.
            } else {
                return if let Some(paragraph) = parser.finish()? {
                    Ok(Some(paragraph))
                } else {
                    Ok(None)
                };
            }
        }
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
        let mut parser = ControlFileParser::default();

        loop {
            let mut line = String::new();
            let bytes_read = reader.read_line(&mut line)?;

            // .read_line() indicates EOF by Ok(0).
            if bytes_read == 0 {
                break;
            }

            if let Some(paragraph) = parser.write_line(&line)? {
                paragraphs.push(paragraph);
            }
        }

        if let Some(paragraph) = parser.finish()? {
            paragraphs.push(paragraph);
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

    /// Obtain paragraphs in this control file, consuming self.
    pub fn into_paragraphs(self) -> impl Iterator<Item = ControlParagraph<'a>> {
        self.paragraphs.into_iter()
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
/// Specified at <https://www.debian.org/doc/debian-policy/ch-controlfields.html#source-package-control-files-debian-control>.
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
        let paragraphs = ControlParagraphReader::new(std::io::Cursor::new(include_bytes!(
            "testdata/release-debian-bullseye"
        )))
        .collect::<Result<Vec<_>, _>>()?;

        assert_eq!(paragraphs.len(), 1);
        let p = &paragraphs[0];

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
            let reader = std::io::BufReader::new(fh);

            for para in ControlParagraphReader::new(reader) {
                para?;
            }
        }

        Ok(())
    }
}
