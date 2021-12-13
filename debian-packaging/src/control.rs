// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Defines primitives in control files.

See <https://www.debian.org/doc/debian-policy/ch-controlfields.html>
for the canonical source of truth for how control files work.
*/

use {
    crate::error::{DebianError, Result},
    futures::{AsyncBufRead, AsyncBufReadExt},
    pin_project::pin_project,
    std::{
        borrow::Cow,
        collections::HashMap,
        io::{BufRead, Write},
    },
};

/// A field value in a control file.
///
/// This represents the value after the colon (`:`) in field definitions.
///
/// There are canonically 3 types of field values: *simple*, *folded*, and *multiline*.
/// The differences between *folded* and *multiline* are semantic. *folded* is logically
/// a single line spanning multiple formatted lines and whitespace is not significant.
/// *multiline* has similar syntax as *folded* but whitespace is significant.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ControlFieldValue<'a> {
    Simple(Cow<'a, str>),
    Folded(Cow<'a, str>),
    Multiline(Cow<'a, str>),
}

impl<'a> AsRef<Cow<'a, str>> for ControlFieldValue<'a> {
    fn as_ref(&self) -> &Cow<'a, str> {
        match self {
            Self::Simple(v) => v,
            Self::Folded(v) => v,
            Self::Multiline(v) => v,
        }
    }
}

impl<'a> ControlFieldValue<'a> {
    /// Obtain an iterator over string values in this field.
    ///
    /// [Self::Simple] variants will emit a single item.
    ///
    /// [Self::Folded] and [Self::Multiline] may emit multiple items.
    ///
    /// For variants stored as multiple lines, leading whitespace will be trimmed, as necessary.
    pub fn iter_lines(&self) -> Box<(dyn Iterator<Item = &str> + '_)> {
        match self {
            Self::Simple(v) => Box::new([v.as_ref()].into_iter()),
            Self::Folded(values) => Box::new(values.lines().map(|x| x.trim_start())),
            Self::Multiline(values) => Box::new(values.lines().map(|x| x.trim_start())),
        }
    }

    /// Obtain an iterator over words in the string value.
    ///
    /// The result may be non-meaningful for multiple line variants.
    pub fn iter_words(&self) -> Box<(dyn Iterator<Item = &str> + '_)> {
        Box::new(self.as_ref().split_ascii_whitespace())
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

    /// Obtain the inner string backing this value and consume this instance.
    pub fn into_inner(self) -> Cow<'a, str> {
        match self {
            Self::Simple(v) => v,
            Self::Folded(v) => v,
            Self::Multiline(v) => v,
        }
    }

    /// Convert this strongly typed value into a [ControlField].
    pub fn to_control_field(self, name: Cow<'a, str>) -> ControlField<'a> {
        ControlField::new(name, self.into_inner())
    }
}

/// A field in a control file.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ControlField<'a> {
    name: Cow<'a, str>,
    value: Cow<'a, str>,
}

impl<'a> ControlField<'a> {
    /// Construct an instance from a field name and value.
    pub fn new(name: Cow<'a, str>, value: Cow<'a, str>) -> Self {
        Self { name, value }
    }

    /// Construct an instance from an iterable of lines.
    ///
    /// Each line should not have leading whitespace.
    pub fn from_lines(name: Cow<'a, str>, lines: impl Iterator<Item = String>) -> Self {
        let value = lines
            .enumerate()
            .map(|(i, line)| if i == 0 { line } else { format!(" {}", line) })
            .collect::<Vec<_>>()
            .join("\n")
            .into();

        Self { name, value }
    }

    /// The name of this field.
    pub fn name(&self) -> &str {
        self.name.as_ref()
    }

    /// Obtain the value as a [&str].
    ///
    /// The value's original file formatting (including newlines and leading whitespace)
    /// is included.
    pub fn value_str(&self) -> &str {
        self.value.as_ref()
    }

    /// Obtain the value as a [ControlFieldValue::Simple] if possible.
    ///
    /// This will fail if the tracked value has multiple lines.
    pub fn as_simple(&self) -> Result<ControlFieldValue<'a>> {
        if self.value.as_ref().contains('\n') {
            Err(DebianError::ControlSimpleValueNoMultiline)
        } else {
            Ok(ControlFieldValue::Simple(self.value.clone()))
        }
    }

    /// Obtain the value as a [ControlFieldValue::Folded].
    pub fn as_folded(&self) -> ControlFieldValue<'a> {
        ControlFieldValue::Folded(self.value.clone())
    }

    /// Obtain the value as a [ControlFieldValue::Multiline].
    pub fn as_multiline(&self) -> ControlFieldValue<'a> {
        ControlFieldValue::Multiline(self.value.clone())
    }

    /// Obtain an iterator of words in the value.
    pub fn iter_words(&self) -> Box<(dyn Iterator<Item = &str> + '_)> {
        Box::new(self.value.as_ref().split_ascii_whitespace())
    }

    /// Obtain an iterator of lines in the value.
    ///
    /// Leading whitespace from each line is stripped.
    pub fn iter_lines(&self) -> Box<(dyn Iterator<Item = &str> + '_)> {
        Box::new(self.value.lines().map(|x| x.trim_start()))
    }

    /// Write the contents of this field to a writer.
    pub fn write<W: Write>(&self, writer: &mut W) -> std::io::Result<()> {
        writer.write_all(self.name.as_bytes())?;
        writer.write_all(b": ")?;
        writer.write_all(self.value.as_ref().as_bytes())?;
        writer.write_all(b"\n")
    }
}

impl<'a> ToString for ControlField<'a> {
    fn to_string(&self) -> String {
        format!("{}: {}\n", self.name, self.value_str())
    }
}

/// A paragraph in a control file.
///
/// A paragraph is an ordered series of control fields.
///
/// Field names are case insensitive on read and case preserving on set.
///
/// Paragraphs can only contain a single occurrence of a field and this is enforced through
/// the mutation APIs.
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

    /// Set the value of a field via a [ControlField].
    ///
    /// If a field with the same name (case insensitive compare) already exists, the old value
    /// will be replaced by the incoming value.
    pub fn set_field(&mut self, field: ControlField<'a>) {
        self.fields
            .retain(|cf| cf.name.to_lowercase() != field.name.to_lowercase());
        self.fields.push(field);
    }

    /// Set the value of a field defined via strings.
    ///
    /// If a field with the same name (case insensitive compare) already exists, the old value
    /// will be replaced by the incoming value.
    pub fn set_field_from_string(&mut self, name: Cow<'a, str>, value: Cow<'a, str>) {
        self.set_field(ControlField::new(name, value));
    }

    /// Whether a named field is present in this paragraph.
    pub fn has_field(&self, name: &str) -> bool {
        self.field(name).is_some()
    }

    /// Iterate over fields in this paragraph.
    ///
    /// Iteration order is insertion order.
    pub fn iter_fields(&self) -> impl Iterator<Item = &ControlField<'a>> {
        self.fields.iter()
    }

    /// Obtain the field with a given name in this paragraph.
    pub fn field(&self, name: &str) -> Option<&'_ ControlField<'a>> {
        self.fields
            .iter()
            .find(|f| f.name.as_ref().to_lowercase() == name.to_lowercase())
    }

    /// Obtain a mutable reference to the field with a given name.
    pub fn field_mut(&mut self, name: &str) -> Option<&'a mut ControlField> {
        self.fields
            .iter_mut()
            .find(|f| f.name.as_ref().to_lowercase() == name.to_lowercase())
    }

    /// Obtain the raw string value of the named field.
    pub fn field_str(&self, name: &str) -> Option<&str> {
        self.field(name).map(|f| f.value_str())
    }

    /// Obtain the value of a field, evaluated as a boolean.
    ///
    /// The field is [true] iff its string value is `yes`.
    pub fn field_bool(&self, name: &str) -> Option<bool> {
        self.field_str(name).map(|v| matches!(v, "yes"))
    }

    /// Obtain the field with the given name as a [ControlFieldValue::Simple], if possible.
    pub fn field_simple(&self, name: &str) -> Option<Result<ControlFieldValue<'a>>> {
        self.field(name).map(|cf| cf.as_simple())
    }

    /// Obtain the field with the given name as a [ControlFieldValue::Folded].
    pub fn field_folded(&self, name: &str) -> Option<ControlFieldValue<'a>> {
        self.field(name).map(|cf| cf.as_folded())
    }

    /// Obtain the field with the given name as a [ControlFieldValue::Multiline].
    pub fn field_multiline(&self, name: &str) -> Option<ControlFieldValue<'a>> {
        self.field(name).map(|cf| cf.as_multiline())
    }

    /// Obtain an iterator of words in the named field.
    pub fn field_iter_value_words(
        &self,
        name: &str,
    ) -> Option<Box<(dyn Iterator<Item = &str> + '_)>> {
        self.field(name)
            .map(|f| Box::new(f.value.split_ascii_whitespace()) as Box<dyn Iterator<Item = &str>>)
    }

    /// Obtain an iterator of lines in the named field.
    pub fn field_iter_value_lines(
        &self,
        name: &str,
    ) -> Option<Box<(dyn Iterator<Item = &str> + '_)>> {
        self.field(name).map(|f| f.iter_lines())
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
    ///
    /// A trailing newline is written as part of the final field. However, an
    /// extra newline is not present. So if serializing multiple paragraphs, an
    /// additional line break must be written to effectively terminate this paragraph
    /// if the writer is not at EOF.
    pub fn write<W: Write>(&self, writer: &mut W) -> std::io::Result<()> {
        for field in &self.fields {
            field.write(writer)?;
        }

        Ok(())
    }
}

impl<'a> ToString for ControlParagraph<'a> {
    fn to_string(&self) -> String {
        let fields = self
            .fields
            .iter()
            .map(|f| f.to_string())
            .collect::<Vec<_>>();

        fields.join("")
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
    pub fn write_line(&mut self, line: &str) -> Result<Option<ControlParagraph<'static>>> {
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
    pub fn finish(mut self) -> Result<Option<ControlParagraph<'static>>> {
        if let Some(field) = self.field.take() {
            self.flush_field(field)?;
        }

        Ok(if self.paragraph.is_empty() {
            None
        } else {
            Some(self.paragraph)
        })
    }

    fn flush_field(&mut self, v: String) -> Result<()> {
        let mut parts = v.splitn(2, ':');

        let name = parts.next().ok_or_else(|| {
            DebianError::ControlParseError(format!("error parsing line '{}'; missing colon", v))
        })?;
        let value = parts
            .next()
            .ok_or_else(|| {
                DebianError::ControlParseError(format!(
                    "error parsing field '{}'; could not detect value",
                    v
                ))
            })?
            .trim();

        self.paragraph
            .set_field_from_string(Cow::Owned(name.to_string()), Cow::Owned(value.to_string()));

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

    fn get_next(&mut self) -> Result<Option<ControlParagraph<'static>>> {
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
    type Item = Result<ControlParagraph<'static>>;

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
#[pin_project]
pub struct ControlParagraphAsyncReader<R> {
    #[pin]
    reader: R,
    parser: Option<ControlFileParser>,
}

impl<R> ControlParagraphAsyncReader<R>
where
    R: AsyncBufRead + Unpin,
{
    /// Create a new instance bound to a reader.
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            parser: Some(ControlFileParser::default()),
        }
    }

    /// Consumes self, returning the inner reader.
    pub fn into_inner(self) -> R {
        self.reader
    }

    /// Read the next available paragraph from this reader.
    ///
    /// Resolves to [None] on end of input.
    pub async fn read_paragraph(&mut self) -> Result<Option<ControlParagraph<'static>>> {
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
    pub fn parse_reader<R: BufRead>(reader: &mut R) -> Result<Self> {
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
    pub fn parse_str(s: &str) -> Result<Self> {
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
            writer.write_all(b"\n")?;
        }

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
    pub fn parse_reader<R: BufRead>(reader: &mut R) -> Result<Self> {
        let control = ControlFile::parse_reader(reader)?;

        let mut paragraphs = control.paragraphs();

        let general = paragraphs
            .next()
            .ok_or_else(|| {
                DebianError::ControlParseError(
                    "no general paragraph in source control file".to_string(),
                )
            })?
            .to_owned();

        let binaries = paragraphs.map(|x| x.to_owned()).collect();

        Ok(Self { general, binaries })
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_paragraph_field_semantics() {
        let mut p = ControlParagraph::default();

        // Same cased field name results in overwrite.
        p.set_field_from_string("foo".into(), "bar".into());
        p.set_field_from_string("foo".into(), "baz".into());
        assert_eq!(p.field("foo").unwrap().value, "baz");

        // Different case results in overwrite.
        p.set_field_from_string("FOO".into(), "bar".into());
        assert_eq!(p.field("foo").unwrap().value, "bar");
        assert_eq!(p.field("FOO").unwrap().value, "bar");
    }

    #[test]
    fn parse_paragraph_release() -> Result<()> {
        let paragraphs = ControlParagraphReader::new(std::io::Cursor::new(include_bytes!(
            "testdata/release-debian-bullseye"
        )))
        .collect::<Result<Vec<_>>>()?;

        assert_eq!(paragraphs.len(), 1);
        let p = &paragraphs[0];

        assert_eq!(p.fields.len(), 14);

        assert!(p.has_field("Origin"));
        assert!(p.has_field("Version"));
        assert!(!p.has_field("Missing"));

        assert!(p.field("Version").is_some());

        let fields = &p.fields;
        assert_eq!(fields[0].name, "Origin");
        assert_eq!(fields[0].value, "Debian");

        assert_eq!(fields[3].name, "Version");
        assert_eq!(fields[3].value, "11.1");

        let ml = p.field_multiline("MD5Sum").unwrap();
        assert_eq!(ml.iter_lines().count(), 600);
        assert_eq!(
            ml.iter_lines().next().unwrap(),
            "7fdf4db15250af5368cc52a91e8edbce   738242 contrib/Contents-all"
        );

        assert!(p.field_multiline("SHA256").is_some());

        assert_eq!(fields[0].iter_words().collect::<Vec<_>>(), vec!["Debian"]);

        let values = p
            .field_multiline("MD5Sum")
            .unwrap()
            .iter_lines()
            .map(|x| x.to_string())
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
            .field_multiline("SHA256")
            .unwrap()
            .iter_lines()
            .map(|x| x.to_string())
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
        let paths = glob::glob("/var/lib/apt/lists/*_Packages")
            .unwrap()
            .chain(glob::glob("/var/lib/apt/lists/*_Sources").unwrap())
            .chain(glob::glob("/var/lib/apt/lists/*i18n_Translation-*").unwrap());

        for path in paths {
            let path = path.unwrap();

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
