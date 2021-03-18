// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Functionality related to "code resources," external resources captured in signatures.
//!
//! Bundles can contain a `_CodeSignature/CodeResources` XML plist file
//! denoting signatures for resources not in the binary. The signature data
//! in the binary can record the digest of this file so integrity is transitively
//! verified.
//!
//! We've implemented our own (de)serialization code in this module because
//! the default derived Deserialize provided by the `plist` crate doesn't
//! handle enums correctly. We attempted to implement our own `Deserialize`
//! and `Visitor` traits to get things to parse, but we couldn't make it work.
//! We gave up and decided to just coerce the [plist::Value] instances instead.

use {
    plist::{Dictionary, Value},
    std::{collections::HashMap, convert::TryFrom, io::Write},
};

/// Represents an error when handling code resources.
#[derive(Debug)]
pub enum CodeResourcesError {
    Plist(plist::Error),
    Base64(base64::DecodeError),
    PlistParseError(String),
}

impl std::fmt::Display for CodeResourcesError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Plist(e) => f.write_fmt(format_args!("plist error: {}", e)),
            Self::Base64(e) => f.write_fmt(format_args!("base64 error: {}", e)),
            Self::PlistParseError(msg) => f.write_fmt(format_args!("plist parse error: {}", msg)),
        }
    }
}

impl std::error::Error for CodeResourcesError {}

impl From<plist::Error> for CodeResourcesError {
    fn from(e: plist::Error) -> Self {
        Self::Plist(e)
    }
}

impl From<base64::DecodeError> for CodeResourcesError {
    fn from(e: base64::DecodeError) -> Self {
        Self::Base64(e)
    }
}

#[derive(Clone, PartialEq)]
enum FilesValue {
    Required(Vec<u8>),
    Optional(Vec<u8>),
}

impl std::fmt::Debug for FilesValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Required(digest) => f
                .debug_struct("FilesValue")
                .field("required", &true)
                .field("digest", &hex::encode(digest))
                .finish(),
            Self::Optional(digest) => f
                .debug_struct("FilesValue")
                .field("required", &false)
                .field("digest", &hex::encode(digest))
                .finish(),
        }
    }
}

impl std::fmt::Display for FilesValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Required(digest) => {
                f.write_fmt(format_args!("{} (required)", hex::encode(digest)))
            }
            Self::Optional(digest) => {
                f.write_fmt(format_args!("{} (optional)", hex::encode(digest)))
            }
        }
    }
}

impl TryFrom<&Value> for FilesValue {
    type Error = CodeResourcesError;

    fn try_from(v: &Value) -> Result<Self, Self::Error> {
        match v {
            Value::Data(digest) => Ok(Self::Required(digest.to_vec())),
            Value::Dictionary(dict) => {
                let mut digest = None;
                let mut optional = None;

                for (key, value) in dict.iter() {
                    match key.as_str() {
                        "hash" => {
                            let data = value.as_data().ok_or_else(|| {
                                CodeResourcesError::PlistParseError(format!(
                                    "expected <data> for files <dict> entry, got {:?}",
                                    value
                                ))
                            })?;

                            digest = Some(data.to_vec());
                        }
                        "optional" => {
                            let v = value.as_boolean().ok_or_else(|| {
                                CodeResourcesError::PlistParseError(format!(
                                    "expected boolean for optional key, got {:?}",
                                    value
                                ))
                            })?;

                            optional = Some(v);
                        }
                        key => {
                            return Err(CodeResourcesError::PlistParseError(format!(
                                "unexpected key in files dict: {}",
                                key
                            )));
                        }
                    }
                }

                match (digest, optional) {
                    (Some(digest), Some(true)) => Ok(Self::Optional(digest)),
                    (Some(digest), Some(false)) => Ok(Self::Required(digest)),
                    _ => Err(CodeResourcesError::PlistParseError(
                        "missing hash or optional key".to_string(),
                    )),
                }
            }
            _ => Err(CodeResourcesError::PlistParseError(format!(
                "bad value in files <dict>; expected <data> or <dict>, got {:?}",
                v
            ))),
        }
    }
}

impl From<&FilesValue> for Value {
    fn from(v: &FilesValue) -> Self {
        match v {
            FilesValue::Required(digest) => Self::Data(digest.to_vec()),
            FilesValue::Optional(digest) => {
                let mut dict = Dictionary::new();
                dict.insert("hash".to_string(), Value::Data(digest.to_vec()));
                dict.insert("optional".to_string(), Value::Boolean(true));

                Self::Dictionary(dict)
            }
        }
    }
}

#[derive(Clone, PartialEq)]
struct Files2Value {
    cdhash: Option<Vec<u8>>,
    hash2: Option<Vec<u8>>,
    requirement: Option<String>,
    symlink: Option<String>,
}

impl std::fmt::Debug for Files2Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Files2Value")
            .field(
                "cdhash",
                &format_args!("{:?}", self.cdhash.as_ref().map(hex::encode)),
            )
            .field(
                "hash2",
                &format_args!("{:?}", self.hash2.as_ref().map(hex::encode)),
            )
            .field("requirement", &format_args!("{:?}", self.requirement))
            .field("symlink", &format_args!("{:?}", self.symlink))
            .finish()
    }
}

impl TryFrom<&Value> for Files2Value {
    type Error = CodeResourcesError;

    fn try_from(v: &Value) -> Result<Self, Self::Error> {
        let dict = v.as_dictionary().ok_or_else(|| {
            CodeResourcesError::PlistParseError("files2 value should be a dict".to_string())
        })?;

        let mut hash2 = None;
        let mut cdhash = None;
        let mut requirement = None;
        let mut symlink = None;

        for (key, value) in dict.iter() {
            match key.as_str() {
                "cdhash" => {
                    let data = value.as_data().ok_or_else(|| {
                        CodeResourcesError::PlistParseError(format!(
                            "expected <data> for files2 cdhash entry, got {:?}",
                            value
                        ))
                    })?;

                    cdhash = Some(data.to_vec());
                }

                "hash2" => {
                    let data = value.as_data().ok_or_else(|| {
                        CodeResourcesError::PlistParseError(format!(
                            "expected <data> for files2 hash entry, got {:?}",
                            value
                        ))
                    })?;

                    hash2 = Some(data.to_vec());
                }
                "requirement" => {
                    let v = value.as_string().ok_or_else(|| {
                        CodeResourcesError::PlistParseError(format!(
                            "expected string for requirement key, got {:?}",
                            value
                        ))
                    })?;

                    requirement = Some(v.to_string());
                }
                "symlink" => {
                    symlink = Some(
                        value
                            .as_string()
                            .ok_or_else(|| {
                                CodeResourcesError::PlistParseError(format!(
                                    "expected string for symlink key, got {:?}",
                                    value
                                ))
                            })?
                            .to_string(),
                    );
                }
                key => {
                    return Err(CodeResourcesError::PlistParseError(format!(
                        "unexpected key in files2 dict entry: {}",
                        key
                    )));
                }
            }
        }

        Ok(Self {
            cdhash,
            hash2,
            requirement,
            symlink,
        })
    }
}

impl From<&Files2Value> for Value {
    fn from(v: &Files2Value) -> Self {
        let mut dict = Dictionary::new();

        if let Some(cdhash) = &v.cdhash {
            dict.insert("cdhash".to_string(), Value::Data(cdhash.to_vec()));
        }

        if let Some(hash2) = &v.hash2 {
            dict.insert("hash2".to_string(), Value::Data(hash2.to_vec()));
        }

        if let Some(requirement) = &v.requirement {
            dict.insert(
                "requirement".to_string(),
                Value::String(requirement.to_string()),
            );
        }

        if let Some(symlink) = &v.symlink {
            dict.insert("symlink".to_string(), Value::String(symlink.to_string()));
        }

        Value::Dictionary(dict)
    }
}

#[derive(Clone, Debug, PartialEq)]
struct RulesValue {
    required: bool,
    weight: Option<f64>,
}

impl TryFrom<&Value> for RulesValue {
    type Error = CodeResourcesError;

    fn try_from(v: &Value) -> Result<Self, Self::Error> {
        match v {
            Value::Boolean(true) => Ok(Self {
                required: true,
                weight: None,
            }),
            Value::Dictionary(dict) => {
                let mut optional = None;
                let mut weight = None;

                for (key, value) in dict {
                    match key.as_str() {
                        "optional" => {
                            optional = Some(value.as_boolean().ok_or_else(|| {
                                CodeResourcesError::PlistParseError(format!(
                                    "rules optional key value not a boolean, got {:?}",
                                    value
                                ))
                            })?);
                        }
                        "weight" => {
                            weight = Some(value.as_real().ok_or_else(|| {
                                CodeResourcesError::PlistParseError(format!(
                                    "rules weight key value not a real, got {:?}",
                                    value
                                ))
                            })?);
                        }
                        key => {
                            return Err(CodeResourcesError::PlistParseError(format!(
                                "extra key in rules dict: {}",
                                key
                            )));
                        }
                    }
                }

                match (optional, weight) {
                    (Some(optional), Some(_)) => Ok(Self {
                        required: !optional,
                        weight,
                    }),
                    _ => Err(CodeResourcesError::PlistParseError(
                        "rules dict must have optional and weight keys".to_string(),
                    )),
                }
            }
            _ => Err(CodeResourcesError::PlistParseError(
                "invalid value for rules entry".to_string(),
            )),
        }
    }
}

impl From<&RulesValue> for Value {
    fn from(v: &RulesValue) -> Self {
        if v.required {
            Value::Boolean(true)
        } else {
            let mut dict = Dictionary::new();

            dict.insert("optional".to_string(), Value::Boolean(true));

            if let Some(weight) = v.weight {
                dict.insert("weight".to_string(), Value::Real(weight));
            }

            Value::Dictionary(dict)
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct Rules2Value {
    nested: Option<bool>,
    omit: Option<bool>,
    weight: Option<f64>,
}

impl TryFrom<&Value> for Rules2Value {
    type Error = CodeResourcesError;

    fn try_from(v: &Value) -> Result<Self, Self::Error> {
        let dict = v.as_dictionary().ok_or_else(|| {
            CodeResourcesError::PlistParseError("rules2 value should be a dict".to_string())
        })?;

        let mut nested = None;
        let mut omit = None;
        let mut weight = None;

        for (key, value) in dict.iter() {
            match key.as_str() {
                "nested" => {
                    nested = Some(value.as_boolean().ok_or_else(|| {
                        CodeResourcesError::PlistParseError(format!(
                            "expected bool for rules2 nested key, got {:?}",
                            value
                        ))
                    })?);
                }
                "omit" => {
                    omit = Some(value.as_boolean().ok_or_else(|| {
                        CodeResourcesError::PlistParseError(format!(
                            "expected bool for rules2 omit key, got {:?}",
                            value
                        ))
                    })?);
                }
                "weight" => {
                    weight = Some(value.as_real().ok_or_else(|| {
                        CodeResourcesError::PlistParseError(format!(
                            "expected real for rules2 weight key, got {:?}",
                            value
                        ))
                    })?);
                }
                key => {
                    return Err(CodeResourcesError::PlistParseError(format!(
                        "unexpected key in rules dict entry: {}",
                        key
                    )));
                }
            }
        }

        Ok(Self {
            nested,
            omit,
            weight,
        })
    }
}

impl From<&Rules2Value> for Value {
    fn from(v: &Rules2Value) -> Self {
        let mut dict = Dictionary::new();

        if let Some(true) = v.omit {
            dict.insert("omit".to_string(), Value::Boolean(true));
        }

        if let Some(weight) = v.weight {
            dict.insert("weight".to_string(), Value::Real(weight));
        }

        if let Some(true) = v.nested {
            dict.insert("nested".to_string(), Value::Boolean(true));
        }

        Value::Dictionary(dict)
    }
}

/// Represents a `_CodeSignature/CodeResources` XML plist.
///
/// This file/type represents a collection of file-based resources whose
/// content is digested and captured in this file.
#[derive(Clone, Debug, PartialEq)]
pub struct CodeResources {
    files: HashMap<String, FilesValue>,
    files2: HashMap<String, Files2Value>,
    rules: HashMap<String, RulesValue>,
    rules2: HashMap<String, Rules2Value>,
}

impl CodeResources {
    /// Construct an instance by parsing an XML plist.
    pub fn from_xml(xml: &[u8]) -> Result<Self, CodeResourcesError> {
        let plist = Value::from_reader_xml(xml)?;

        let dict = plist.into_dictionary().ok_or_else(|| {
            CodeResourcesError::PlistParseError("plist root element should be a <dict>".to_string())
        })?;

        let mut files = HashMap::new();
        let mut files2 = HashMap::new();
        let mut rules = HashMap::new();
        let mut rules2 = HashMap::new();

        for (key, value) in dict.iter() {
            match key.as_ref() {
                "files" => {
                    let dict = value.as_dictionary().ok_or_else(|| {
                        CodeResourcesError::PlistParseError(format!(
                            "expecting files to be a dict, got {:?}",
                            value
                        ))
                    })?;

                    for (key, value) in dict {
                        files.insert(key.to_string(), FilesValue::try_from(value)?);
                    }
                }
                "files2" => {
                    let dict = value.as_dictionary().ok_or_else(|| {
                        CodeResourcesError::PlistParseError(format!(
                            "expecting files2 to be a dict, got {:?}",
                            value
                        ))
                    })?;

                    for (key, value) in dict {
                        files2.insert(key.to_string(), Files2Value::try_from(value)?);
                    }
                }
                "rules" => {
                    let dict = value.as_dictionary().ok_or_else(|| {
                        CodeResourcesError::PlistParseError(format!(
                            "expecting rules to be a dict, got {:?}",
                            value
                        ))
                    })?;

                    for (key, value) in dict {
                        rules.insert(key.to_string(), RulesValue::try_from(value)?);
                    }
                }
                "rules2" => {
                    let dict = value.as_dictionary().ok_or_else(|| {
                        CodeResourcesError::PlistParseError(format!(
                            "expecting rules2 to be a dict, got {:?}",
                            value
                        ))
                    })?;

                    for (key, value) in dict {
                        rules2.insert(key.to_string(), Rules2Value::try_from(value)?);
                    }
                }
                key => {
                    return Err(CodeResourcesError::PlistParseError(format!(
                        "unexpected key in root dict: {}",
                        key
                    )));
                }
            }
        }

        Ok(Self {
            files,
            files2,
            rules,
            rules2,
        })
    }

    pub fn to_writer_xml(&self, writer: impl Write) -> Result<(), CodeResourcesError> {
        let value = Value::from(self);

        Ok(value.to_writer_xml(writer)?)
    }
}

impl From<&CodeResources> for Value {
    fn from(cr: &CodeResources) -> Self {
        let mut dict = Dictionary::new();

        if !cr.files.is_empty() {
            dict.insert(
                "files".to_string(),
                Value::Dictionary(
                    cr.files
                        .iter()
                        .map(|(key, value)| (key.to_string(), Value::from(value)))
                        .collect::<Dictionary>(),
                ),
            );
        }

        if !cr.files2.is_empty() {
            dict.insert(
                "files2".to_string(),
                Value::Dictionary(
                    cr.files2
                        .iter()
                        .map(|(key, value)| (key.to_string(), Value::from(value)))
                        .collect::<Dictionary>(),
                ),
            );
        }

        if !cr.rules.is_empty() {
            dict.insert(
                "rules".to_string(),
                Value::Dictionary(
                    cr.rules
                        .iter()
                        .map(|(key, value)| (key.to_string(), Value::from(value)))
                        .collect::<Dictionary>(),
                ),
            );
        }

        if !cr.rules2.is_empty() {
            dict.insert(
                "rules2".to_string(),
                Value::Dictionary(
                    cr.rules2
                        .iter()
                        .map(|(key, value)| (key.to_string(), Value::from(value)))
                        .collect::<Dictionary>(),
                ),
            );
        }

        Value::Dictionary(dict)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIREFOX_SNIPPET: &str = r#"
        <?xml version="1.0" encoding="UTF-8"?>
        <!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
        <plist version="1.0">
          <dict>
            <key>files</key>
            <dict>
              <key>Resources/XUL.sig</key>
              <data>Y0SEPxyC6hCQ+rl4LTRmXy7F9DQ=</data>
              <key>Resources/en.lproj/InfoPlist.strings</key>
              <dict>
                <key>hash</key>
                <data>U8LTYe+cVqPcBu9aLvcyyfp+dAg=</data>
                <key>optional</key>
                <true/>
              </dict>
              <key>Resources/firefox-bin.sig</key>
              <data>ZvZ3yDciAF4kB9F06Xr3gKi3DD4=</data>
            </dict>
            <key>files2</key>
            <dict>
              <key>Library/LaunchServices/org.mozilla.updater</key>
              <dict>
                <key>hash2</key>
                <data>iMnDHpWkKTI6xLi9Av93eNuIhxXhv3C18D4fljCfw2Y=</data>
              </dict>
              <key>MacOS/XUL</key>
              <dict>
                <key>cdhash</key>
                <data>NevNMzQBub9OjomMUAk2xBumyHM=</data>
                <key>requirement</key>
                <string>anchor apple generic and certificate leaf[field.1.2.840.113635.100.6.1.9] /* exists */ or anchor apple generic and certificate 1[field.1.2.840.113635.100.6.2.6] /* exists */ and certificate leaf[field.1.2.840.113635.100.6.1.13] /* exists */ and certificate leaf[subject.OU] = "43AQ936H96"</string>
              </dict>
              <key>MacOS/SafariForWebKitDevelopment</key>
              <dict>
                <key>symlink</key>
                <string>/Library/Application Support/Apple/Safari/SafariForWebKitDevelopment</string>
              </dict>
            </dict>
            <key>rules</key>
            <dict>
              <key>^Resources/</key>
              <true/>
              <key>^Resources/.*\.lproj/</key>
              <dict>
                <key>optional</key>
                <true/>
                <key>weight</key>
                <real>1000</real>
              </dict>
            </dict>
            <key>rules2</key>
            <dict>
              <key>.*\.dSYM($|/)</key>
              <dict>
                <key>weight</key>
                <real>11</real>
              </dict>
              <key>^(.*/)?\.DS_Store$</key>
              <dict>
                <key>omit</key>
                <true/>
                <key>weight</key>
                <real>2000</real>
              </dict>
              <key>^[^/]+$</key>
              <dict>
                <key>nested</key>
                <true/>
                <key>weight</key>
                <real>10</real>
              </dict>
            </dict>
          </dict>
        </plist>"#;

    #[test]
    fn parse_firefox() {
        let resources = CodeResources::from_xml(FIREFOX_SNIPPET.as_bytes()).unwrap();

        println!("{:#?}", resources);

        // Serialize back to XML.
        let mut buffer = Vec::<u8>::new();
        resources.to_writer_xml(&mut buffer).unwrap();
        let resources2 = CodeResources::from_xml(&buffer).unwrap();

        assert_eq!(resources, resources2);
    }
}
