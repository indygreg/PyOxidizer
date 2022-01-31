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
    crate::{
        bundle_signing::{BundleFileHandler, SignedMachOInfo},
        error::AppleCodesignError,
        macho::DigestType,
    },
    apple_bundles::DirectoryBundleFile,
    plist::{Dictionary, Value},
    slog::{debug, info, Logger},
    std::{cmp::Ordering, collections::BTreeMap, io::Write},
};

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
    type Error = AppleCodesignError;

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
                                AppleCodesignError::ResourcesPlistParse(format!(
                                    "expected <data> for files <dict> entry, got {:?}",
                                    value
                                ))
                            })?;

                            digest = Some(data.to_vec());
                        }
                        "optional" => {
                            let v = value.as_boolean().ok_or_else(|| {
                                AppleCodesignError::ResourcesPlistParse(format!(
                                    "expected boolean for optional key, got {:?}",
                                    value
                                ))
                            })?;

                            optional = Some(v);
                        }
                        key => {
                            return Err(AppleCodesignError::ResourcesPlistParse(format!(
                                "unexpected key in files dict: {}",
                                key
                            )));
                        }
                    }
                }

                match (digest, optional) {
                    (Some(digest), Some(true)) => Ok(Self::Optional(digest)),
                    (Some(digest), Some(false)) => Ok(Self::Required(digest)),
                    _ => Err(AppleCodesignError::ResourcesPlistParse(
                        "missing hash or optional key".to_string(),
                    )),
                }
            }
            _ => Err(AppleCodesignError::ResourcesPlistParse(format!(
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
    optional: Option<bool>,
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
            .field("optional", &format_args!("{:?}", self.optional))
            .field("requirement", &format_args!("{:?}", self.requirement))
            .field("symlink", &format_args!("{:?}", self.symlink))
            .finish()
    }
}

impl TryFrom<&Value> for Files2Value {
    type Error = AppleCodesignError;

    fn try_from(v: &Value) -> Result<Self, Self::Error> {
        let dict = v.as_dictionary().ok_or_else(|| {
            AppleCodesignError::ResourcesPlistParse("files2 value should be a dict".to_string())
        })?;

        let mut hash2 = None;
        let mut cdhash = None;
        let mut optional = None;
        let mut requirement = None;
        let mut symlink = None;

        for (key, value) in dict.iter() {
            match key.as_str() {
                "cdhash" => {
                    let data = value.as_data().ok_or_else(|| {
                        AppleCodesignError::ResourcesPlistParse(format!(
                            "expected <data> for files2 cdhash entry, got {:?}",
                            value
                        ))
                    })?;

                    cdhash = Some(data.to_vec());
                }
                "hash2" => {
                    let data = value.as_data().ok_or_else(|| {
                        AppleCodesignError::ResourcesPlistParse(format!(
                            "expected <data> for files2 hash entry, got {:?}",
                            value
                        ))
                    })?;

                    hash2 = Some(data.to_vec());
                }
                "optional" => {
                    let v = value.as_boolean().ok_or_else(|| {
                        AppleCodesignError::ResourcesPlistParse(format!(
                            "expected bool for optional key, got {:?}",
                            value
                        ))
                    })?;

                    optional = Some(v);
                }
                "requirement" => {
                    let v = value.as_string().ok_or_else(|| {
                        AppleCodesignError::ResourcesPlistParse(format!(
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
                                AppleCodesignError::ResourcesPlistParse(format!(
                                    "expected string for symlink key, got {:?}",
                                    value
                                ))
                            })?
                            .to_string(),
                    );
                }
                key => {
                    return Err(AppleCodesignError::ResourcesPlistParse(format!(
                        "unexpected key in files2 dict entry: {}",
                        key
                    )));
                }
            }
        }

        Ok(Self {
            cdhash,
            hash2,
            optional,
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

        if let Some(optional) = &v.optional {
            dict.insert("optional".to_string(), Value::Boolean(*optional));
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
    omit: bool,
    required: bool,
    weight: Option<f64>,
}

impl TryFrom<&Value> for RulesValue {
    type Error = AppleCodesignError;

    fn try_from(v: &Value) -> Result<Self, Self::Error> {
        match v {
            Value::Boolean(true) => Ok(Self {
                omit: false,
                required: true,
                weight: None,
            }),
            Value::Dictionary(dict) => {
                let mut omit = None;
                let mut optional = None;
                let mut weight = None;

                for (key, value) in dict {
                    match key.as_str() {
                        "omit" => {
                            omit = Some(value.as_boolean().ok_or_else(|| {
                                AppleCodesignError::ResourcesPlistParse(format!(
                                    "rules omit key value not a boolean; got {:?}",
                                    value
                                ))
                            })?);
                        }
                        "optional" => {
                            optional = Some(value.as_boolean().ok_or_else(|| {
                                AppleCodesignError::ResourcesPlistParse(format!(
                                    "rules optional key value not a boolean, got {:?}",
                                    value
                                ))
                            })?);
                        }
                        "weight" => {
                            weight = Some(value.as_real().ok_or_else(|| {
                                AppleCodesignError::ResourcesPlistParse(format!(
                                    "rules weight key value not a real, got {:?}",
                                    value
                                ))
                            })?);
                        }
                        key => {
                            return Err(AppleCodesignError::ResourcesPlistParse(format!(
                                "extra key in rules dict: {}",
                                key
                            )));
                        }
                    }
                }

                Ok(Self {
                    omit: omit.unwrap_or(false),
                    required: !optional.unwrap_or(false),
                    weight,
                })
            }
            _ => Err(AppleCodesignError::ResourcesPlistParse(
                "invalid value for rules entry".to_string(),
            )),
        }
    }
}

impl From<&RulesValue> for Value {
    fn from(v: &RulesValue) -> Self {
        if v.required && !v.omit && v.weight.is_none() {
            Value::Boolean(true)
        } else {
            let mut dict = Dictionary::new();

            if v.omit {
                dict.insert("omit".to_string(), Value::Boolean(true));
            }
            if !v.required {
                dict.insert("optional".to_string(), Value::Boolean(true));
            }

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
    optional: Option<bool>,
    weight: Option<f64>,
}

impl TryFrom<&Value> for Rules2Value {
    type Error = AppleCodesignError;

    fn try_from(v: &Value) -> Result<Self, Self::Error> {
        let dict = v.as_dictionary().ok_or_else(|| {
            AppleCodesignError::ResourcesPlistParse("rules2 value should be a dict".to_string())
        })?;

        let mut nested = None;
        let mut omit = None;
        let mut optional = None;
        let mut weight = None;

        for (key, value) in dict.iter() {
            match key.as_str() {
                "nested" => {
                    nested = Some(value.as_boolean().ok_or_else(|| {
                        AppleCodesignError::ResourcesPlistParse(format!(
                            "expected bool for rules2 nested key, got {:?}",
                            value
                        ))
                    })?);
                }
                "omit" => {
                    omit = Some(value.as_boolean().ok_or_else(|| {
                        AppleCodesignError::ResourcesPlistParse(format!(
                            "expected bool for rules2 omit key, got {:?}",
                            value
                        ))
                    })?);
                }
                "optional" => {
                    optional = Some(value.as_boolean().ok_or_else(|| {
                        AppleCodesignError::ResourcesPlistParse(format!(
                            "expected bool for rules2 optional key, got {:?}",
                            value
                        ))
                    })?);
                }
                "weight" => {
                    weight = Some(value.as_real().ok_or_else(|| {
                        AppleCodesignError::ResourcesPlistParse(format!(
                            "expected real for rules2 weight key, got {:?}",
                            value
                        ))
                    })?);
                }
                key => {
                    return Err(AppleCodesignError::ResourcesPlistParse(format!(
                        "unexpected key in rules dict entry: {}",
                        key
                    )));
                }
            }
        }

        Ok(Self {
            nested,
            omit,
            optional,
            weight,
        })
    }
}

impl From<&Rules2Value> for Value {
    fn from(v: &Rules2Value) -> Self {
        let mut dict = Dictionary::new();

        if let Some(true) = v.nested {
            dict.insert("nested".to_string(), Value::Boolean(true));
        }

        if let Some(true) = v.omit {
            dict.insert("omit".to_string(), Value::Boolean(true));
        }

        if let Some(true) = v.optional {
            dict.insert("optional".to_string(), Value::Boolean(true));
        }

        if let Some(weight) = v.weight {
            dict.insert("weight".to_string(), Value::Real(weight));
        }

        if dict.is_empty() {
            Value::Boolean(true)
        } else {
            Value::Dictionary(dict)
        }
    }
}

/// Represents an abstract rule in a `CodeResources` XML plist.
///
/// This type represents both `<rules>` and `<rules2>` entries. It contains a
/// superset of all fields for these entries.
#[derive(Clone, Debug)]
pub struct CodeResourcesRule {
    /// The rule pattern.
    ///
    /// The `<key>` in the `<rules>` or `<rules2>` dict.
    pub pattern: String,

    /// Whether this is an exclusion rule.
    pub exclude: bool,

    pub nested: bool,

    pub omit: bool,

    /// Whether the rule is optional.
    pub optional: bool,

    /// Weighting to apply to the rule.
    pub weight: Option<u32>,

    re: regex::Regex,
}

impl PartialEq for CodeResourcesRule {
    fn eq(&self, other: &Self) -> bool {
        self.pattern == other.pattern
            && self.exclude == other.exclude
            && self.nested == other.nested
            && self.omit == other.omit
            && self.optional == other.optional
            && self.weight == other.weight
    }
}

impl Eq for CodeResourcesRule {}

impl PartialOrd for CodeResourcesRule {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        // Default weight is 1 if not specified.
        let our_weight = self.weight.unwrap_or(1);
        let their_weight = other.weight.unwrap_or(1);

        // Exclusion rules always take priority over inclusion rules.
        // The smaller the weight, the less important it is.
        match self.exclude.cmp(&other.exclude) {
            Ordering::Equal => their_weight.partial_cmp(&our_weight),
            Ordering::Greater => Some(Ordering::Less),
            Ordering::Less => Some(Ordering::Greater),
        }
    }
}

impl Ord for CodeResourcesRule {
    fn cmp(&self, other: &Self) -> Ordering {
        self.partial_cmp(other).unwrap()
    }
}

impl CodeResourcesRule {
    pub fn new(pattern: impl ToString) -> Result<Self, AppleCodesignError> {
        Ok(Self {
            pattern: pattern.to_string(),
            exclude: false,
            nested: false,
            omit: false,
            optional: false,
            weight: None,
            re: regex::Regex::new(&pattern.to_string())
                .map_err(|e| AppleCodesignError::ResourcesBadRegex(pattern.to_string(), e))?,
        })
    }

    /// Mark this as an exclusion rule.
    ///
    /// Exclusion rules are internal to the builder and not materialized in the
    /// `CodeResources` file.
    #[must_use]
    pub fn exclude(mut self) -> Self {
        self.exclude = true;
        self
    }

    /// Mark the rule as nested.
    #[must_use]
    pub fn nested(mut self) -> Self {
        self.nested = true;
        self
    }

    /// Set the omit field.
    #[must_use]
    pub fn omit(mut self) -> Self {
        self.omit = true;
        self
    }

    /// Mark the files matched by this rule are optional.
    #[must_use]
    pub fn optional(mut self) -> Self {
        self.optional = true;
        self
    }

    /// Set the weight of this rule.
    #[must_use]
    pub fn weight(mut self, v: u32) -> Self {
        self.weight = Some(v);
        self
    }
}

/// Represents a `_CodeSignature/CodeResources` XML plist.
///
/// This file/type represents a collection of file-based resources whose
/// content is digested and captured in this file.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CodeResources {
    files: BTreeMap<String, FilesValue>,
    files2: BTreeMap<String, Files2Value>,
    rules: BTreeMap<String, RulesValue>,
    rules2: BTreeMap<String, Rules2Value>,
}

impl CodeResources {
    /// Construct an instance by parsing an XML plist.
    pub fn from_xml(xml: &[u8]) -> Result<Self, AppleCodesignError> {
        let plist = Value::from_reader_xml(xml).map_err(AppleCodesignError::ResourcesPlist)?;

        let dict = plist.into_dictionary().ok_or_else(|| {
            AppleCodesignError::ResourcesPlistParse(
                "plist root element should be a <dict>".to_string(),
            )
        })?;

        let mut files = BTreeMap::new();
        let mut files2 = BTreeMap::new();
        let mut rules = BTreeMap::new();
        let mut rules2 = BTreeMap::new();

        for (key, value) in dict.iter() {
            match key.as_ref() {
                "files" => {
                    let dict = value.as_dictionary().ok_or_else(|| {
                        AppleCodesignError::ResourcesPlistParse(format!(
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
                        AppleCodesignError::ResourcesPlistParse(format!(
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
                        AppleCodesignError::ResourcesPlistParse(format!(
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
                        AppleCodesignError::ResourcesPlistParse(format!(
                            "expecting rules2 to be a dict, got {:?}",
                            value
                        ))
                    })?;

                    for (key, value) in dict {
                        rules2.insert(key.to_string(), Rules2Value::try_from(value)?);
                    }
                }
                key => {
                    return Err(AppleCodesignError::ResourcesPlistParse(format!(
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

    /// Serialize an instance to XML.
    pub fn to_writer_xml(&self, mut writer: impl Write) -> Result<(), AppleCodesignError> {
        let value = Value::from(self);

        // Ideally we'd write direct to the output. However, Apple's XML writer doesn't
        // emit a space for empty elements. e.g. we do `<true />` and Apple does `<true/>`.
        // In addition, our writer doesn't emit a trailing newline. To make it easier to
        // diff generated files with the canonical output, we normalize to Apple's format.
        let mut data = Vec::<u8>::new();
        value
            .to_writer_xml(&mut data)
            .map_err(AppleCodesignError::ResourcesPlist)?;

        let data = String::from_utf8(data).expect("XML should be valid UTF-8");
        let data = data.replace("<dict />", "<dict/>");
        let data = data.replace("<true />", "<true/>");

        writer.write_all(data.as_bytes())?;
        writer.write_all(b"\n")?;

        Ok(())
    }

    /// Add a rule to this instance in the `<rules>` section.
    pub fn add_rule(&mut self, rule: CodeResourcesRule) {
        self.rules.insert(
            rule.pattern,
            RulesValue {
                omit: rule.omit,
                required: !rule.optional,
                weight: rule.weight.map(|x| x as f64),
            },
        );
    }

    /// Add a rule to this instance in the `<rules2>` section.
    pub fn add_rule2(&mut self, rule: CodeResourcesRule) {
        self.rules2.insert(
            rule.pattern,
            Rules2Value {
                nested: if rule.nested { Some(true) } else { None },
                omit: if rule.omit { Some(true) } else { None },
                optional: if rule.optional { Some(true) } else { None },
                weight: rule.weight.map(|x| x as f64),
            },
        );
    }

    /// Seal a regular file.
    ///
    /// This will digest the content specified and record that digest in the files list.
    ///
    /// To seal a symlink, call [CodeResources::seal_symlink] instead. If the file
    /// is a Mach-O file, call [CodeResources::seal_macho] instead.
    pub fn seal_regular_file(
        &mut self,
        path: impl ToString,
        content: impl AsRef<[u8]>,
        optional: bool,
    ) -> Result<(), AppleCodesignError> {
        let sha1 = DigestType::Sha1.digest(content.as_ref())?;
        let sha256 = DigestType::Sha256.digest(content.as_ref())?;

        let path = path.to_string();

        self.files.insert(
            path.clone(),
            if optional {
                FilesValue::Optional(sha1)
            } else {
                FilesValue::Required(sha1)
            },
        );
        self.files2.insert(
            path,
            Files2Value {
                cdhash: None,
                hash2: Some(sha256),
                optional: if optional { Some(true) } else { None },
                requirement: None,
                symlink: None,
            },
        );

        Ok(())
    }

    /// Seal a symlink file.
    ///
    /// `path` is the path of the symlink and `target` is the path it points to.
    pub fn seal_symlink(&mut self, path: impl ToString, target: impl ToString) {
        self.files2.insert(
            path.to_string(),
            Files2Value {
                cdhash: None,
                hash2: None,
                optional: None,
                requirement: None,
                symlink: Some(target.to_string()),
            },
        );
    }

    /// Record metadata of a previously signed Mach-O binary.
    ///
    /// If sealing a fat/universal binary, pass in metadata for the first Mach-O within in.
    pub fn seal_macho(
        &mut self,
        path: impl ToString,
        info: &SignedMachOInfo,
        optional: bool,
    ) -> Result<(), AppleCodesignError> {
        self.files2.insert(
            path.to_string(),
            Files2Value {
                cdhash: Some(DigestType::Sha256Truncated.digest(&info.code_directory_blob)?),
                hash2: None,
                optional: if optional { Some(true) } else { None },
                requirement: info.designated_code_requirement.clone(),
                symlink: None,
            },
        );

        Ok(())
    }
}

impl From<&CodeResources> for Value {
    fn from(cr: &CodeResources) -> Self {
        let mut dict = Dictionary::new();

        dict.insert(
            "files".to_string(),
            Value::Dictionary(
                cr.files
                    .iter()
                    .map(|(key, value)| (key.to_string(), Value::from(value)))
                    .collect::<Dictionary>(),
            ),
        );

        dict.insert(
            "files2".to_string(),
            Value::Dictionary(
                cr.files2
                    .iter()
                    .map(|(key, value)| (key.to_string(), Value::from(value)))
                    .collect::<Dictionary>(),
            ),
        );

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

/// Interface for constructing a `CodeResources` instance.
///
/// This type is used during bundle signing to construct a `CodeResources` instance.
/// It contains logic for validating a file against registered processing rules and
/// handling it accordingly.
#[derive(Clone, Debug, Default)]
pub struct CodeResourcesBuilder {
    rules: Vec<CodeResourcesRule>,
    resources: CodeResources,
}

impl CodeResourcesBuilder {
    /// Obtain an instance with default rules for a bundle with a `Resources/` directory.
    pub fn default_resources_rules() -> Result<Self, AppleCodesignError> {
        let mut slf = Self::default();

        slf.add_rule(CodeResourcesRule::new("^version.plist$")?);
        slf.add_rule(CodeResourcesRule::new("^Resources/")?);
        slf.add_rule(
            CodeResourcesRule::new("^Resources/.*\\.lproj/")?
                .optional()
                .weight(1000),
        );
        slf.add_rule(CodeResourcesRule::new("^Resources/Base\\.lproj/")?.weight(1010));
        slf.add_rule(
            CodeResourcesRule::new("^Resources/.*\\.lproj/locversion.plist$")?
                .omit()
                .weight(1100),
        );

        slf.add_rule2(CodeResourcesRule::new("^.*")?);
        slf.add_rule2(CodeResourcesRule::new("^[^/]+$")?.nested().weight(10));
        slf.add_rule2(CodeResourcesRule::new("^(Frameworks|SharedFrameworks|PlugIns|Plug-ins|XPCServices|Helpers|MacOS|Library/(Automator|Spotlight|LoginItems))/")?
                         .nested().weight(10));
        slf.add_rule2(CodeResourcesRule::new(".*\\.dSYM($|/)")?.weight(11));
        slf.add_rule2(
            CodeResourcesRule::new("^(.*/)?\\.DS_Store$")?
                .omit()
                .weight(2000),
        );
        slf.add_rule2(CodeResourcesRule::new("^Info\\.plist$")?.omit().weight(20));
        slf.add_rule2(CodeResourcesRule::new("^version\\.plist$")?.weight(20));
        slf.add_rule2(CodeResourcesRule::new("^embedded\\.provisionprofile$")?.weight(20));
        slf.add_rule2(CodeResourcesRule::new("^PkgInfo$")?.omit().weight(20));
        slf.add_rule2(CodeResourcesRule::new("^Resources/")?.weight(20));
        slf.add_rule2(
            CodeResourcesRule::new("^Resources/.*\\.lproj/")?
                .optional()
                .weight(1000),
        );
        slf.add_rule2(CodeResourcesRule::new("^Resources/Base\\.lproj/")?.weight(1010));
        slf.add_rule2(
            CodeResourcesRule::new("^Resources/.*\\.lproj/locversion.plist$")?
                .omit()
                .weight(1100),
        );

        Ok(slf)
    }

    /// Obtain an instance with default rules for a bundle without a `Resources/` directory.
    pub fn default_no_resources_rules() -> Result<Self, AppleCodesignError> {
        let mut slf = Self::default();

        slf.add_rule(CodeResourcesRule::new("^version.plist$")?);
        slf.add_rule(CodeResourcesRule::new("^.*")?);
        slf.add_rule(
            CodeResourcesRule::new("^.*\\.lproj")?
                .optional()
                .weight(1000),
        );
        slf.add_rule(CodeResourcesRule::new("^Base\\.lproj")?.weight(1010));
        slf.add_rule(
            CodeResourcesRule::new("^.*\\.lproj/locversion.plist$")?
                .omit()
                .weight(1100),
        );
        slf.add_rule2(CodeResourcesRule::new("^.*")?);
        slf.add_rule2(CodeResourcesRule::new(".*\\.dSYM($|/)")?.weight(11));
        slf.add_rule2(
            CodeResourcesRule::new("^(.*/)?\\.DS_Store$")?
                .omit()
                .weight(2000),
        );
        slf.add_rule2(CodeResourcesRule::new("^Info\\.plist$")?.omit().weight(20));
        slf.add_rule2(CodeResourcesRule::new("^version\\.plist$")?.weight(20));
        slf.add_rule2(CodeResourcesRule::new("^embedded\\.provisionprofile$")?.weight(20));
        slf.add_rule2(CodeResourcesRule::new("^PkgInfo$")?.omit().weight(20));
        slf.add_rule2(
            CodeResourcesRule::new("^.*\\.lproj/")?
                .optional()
                .weight(1000),
        );
        slf.add_rule2(CodeResourcesRule::new("^Base\\.lproj")?.weight(1010));
        slf.add_rule2(
            CodeResourcesRule::new("^.*\\.lproj/locversion.plist$")?
                .omit()
                .weight(1100),
        );

        Ok(slf)
    }

    /// Add a rule to this instance in the `<rules>` section.
    pub fn add_rule(&mut self, rule: CodeResourcesRule) {
        // Don't set internal rules because we only operate in a v2 world.
        self.resources.add_rule(rule);
    }

    /// Add a rule to this instance in the `<rules2>` section.
    pub fn add_rule2(&mut self, rule: CodeResourcesRule) {
        self.rules.push(rule.clone());
        self.rules.sort();
        self.resources.add_rule2(rule);
    }

    /// Add an exclusion rule to the processing rules.
    ///
    /// Exclusion rules are not added to the [CodeResources] because they are
    /// for building only.
    pub fn add_exclusion_rule(&mut self, rule: CodeResourcesRule) {
        self.rules.push(rule);
        self.rules.sort();
    }

    /// Find the first rule matching a given path.
    ///
    /// Rule processing is a bit complicated. Internally, rules are sorted by
    /// decreasing priority. So the first pattern that matches is the rule we use.
    /// However, there are a few special cases.
    ///
    /// If a path begins with `Contents/`, that prefix is ignored when performing the
    /// pattern match.
    ///
    /// Directories are special. If an exclusion rule matches a directory, that directory
    /// tree should be ignored. There are also default rules for handling nested bundles.
    /// However, our bundle scanning code already filters out nested bundles automatically,
    /// so these rules shouldn't be relevant to us. But we handle them anyway, just in
    /// case. These rules take precedence over directory exclusion rules.
    fn find_rule(&self, path: &str) -> Option<CodeResourcesRule> {
        let parts = path.split('/').collect::<Vec<_>>();

        let mut exclude_override = false;

        let rule = self.rules.iter().find(|rule| {
            // Nested rules matching leaf-most directory with `.` result in match.
            // But we treat as exclusion, as these are treated as nested bundles,
            // which are handled externally.
            if rule.nested {
                for last_part in 1..parts.len() - 1 {
                    let parent = parts[0..last_part].join("/");

                    if rule.re.is_match(&parent) && parts[last_part - 1].contains('.') {
                        exclude_override = true;
                        return true;
                    }
                }
            }

            // Directory exclusions match entire directory tree. So walk the parents and yield
            // this rule if matches.
            if rule.exclude {
                for last_part in 1..parts.len() - 1 {
                    let parent = parts[0..last_part].join("/");

                    if rule.re.is_match(&parent) {
                        return true;
                    }
                }
            }

            rule.re.is_match(path)
        });

        if let Some(rule) = rule {
            let mut rule = rule.clone();

            if exclude_override {
                rule.exclude = true;
            }

            Some(rule)
        } else {
            None
        }
    }

    /// Process a file for resource handling.
    ///
    /// This determines whether a file is relevant for inclusion in the CodeResources
    /// file and takes actions to process it, if necessary.
    pub fn process_file(
        &mut self,
        log: &Logger,
        file: &DirectoryBundleFile,
        file_handler: &dyn BundleFileHandler,
    ) -> Result<(), AppleCodesignError> {
        // Always use UNIX style directory separators.
        let relative_path = file.relative_path().to_string_lossy().replace('\\', "/");

        // The Contents/ prefix is also removed for pattern matching and references in the
        // resources file.
        let relative_path = relative_path
            .strip_prefix("Contents/")
            .unwrap_or(&relative_path)
            .to_string();

        info!(log, "processing {}", relative_path);

        let rule = match self.find_rule(relative_path.as_ref()) {
            Some(rule) => {
                debug!(
                    log,
                    "{} matches {} rule {}",
                    relative_path,
                    if rule.exclude || rule.omit {
                        "exclusion"
                    } else {
                        "inclusion"
                    },
                    rule.pattern
                );

                // Excluded files are hard ignored (our caller will handle them if necessary).
                if rule.exclude {
                    return Ok(());
                // Omitted files aren't sealed. But they are installed.
                } else if rule.omit {
                    return file_handler.install_file(log, file);
                }

                rule
            }
            None => {
                debug!(log, "{} doesn't match any rule; processing", relative_path);
                return Ok(());
            }
        };

        if let Some(target) = file
            .symlink_target()
            .map_err(AppleCodesignError::DirectoryBundle)?
        {
            let target = target.to_string_lossy().replace('\\', "/");

            info!(log, "sealing symlink {} -> {}", relative_path, target);
            self.resources.seal_symlink(relative_path, target);
            file_handler.install_file(log, file)?;
        } else {
            let data = std::fs::read(file.absolute_path())?;

            // If nested bit is set, treat as Mach-O binary to be signed.
            if rule.nested {
                let macho_info = file_handler.sign_and_install_macho(log, file)?;
                info!(log, "sealing Mach-O file {}", relative_path);
                self.resources
                    .seal_macho(relative_path, &macho_info, rule.optional)?;
            } else {
                info!(log, "sealing regular file {}", relative_path);
                self.resources
                    .seal_regular_file(relative_path, data, rule.optional)?;
                file_handler.install_file(log, file)?;
            }
        }

        Ok(())
    }

    /// Add metadata for an additional signed Mach-O file.
    ///
    /// This is likely used to register the metadata of a nested bundle. The
    /// metadata likely comes from the first Mach-O binary in the nested bundle's
    /// main executable.
    pub fn add_signed_macho_file(
        &mut self,
        path: &str,
        info: &SignedMachOInfo,
    ) -> Result<(), AppleCodesignError> {
        self.resources.seal_macho(path, info, false)
    }

    /// Write CodeResources XML content to a writer.
    pub fn write_code_resources(&self, writer: impl Write) -> Result<(), AppleCodesignError> {
        self.resources.to_writer_xml(writer)
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
              <key>TestOptional</key>
              <dict>
                <key>hash2</key>
                <data>iMnDHpWkKTI6xLi9Av93eNuIhxXhv3C18D4fljCfw2Y=</data>
                <key>optional</key>
                <true/>
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
              <key>optional</key>
              <dict>
                <key>optional</key>
                <true/>
              </dict>
            </dict>
          </dict>
        </plist>"#;

    #[test]
    fn parse_firefox() {
        let resources = CodeResources::from_xml(FIREFOX_SNIPPET.as_bytes()).unwrap();

        // Serialize back to XML.
        let mut buffer = Vec::<u8>::new();
        resources.to_writer_xml(&mut buffer).unwrap();
        let resources2 = CodeResources::from_xml(&buffer).unwrap();

        assert_eq!(resources, resources2);
    }
}
