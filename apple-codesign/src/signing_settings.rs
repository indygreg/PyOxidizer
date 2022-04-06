// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Code signing settings.

use {
    crate::{
        certificate::AppleCertificate,
        code_directory::{CodeSignatureFlags, ExecutableSegmentFlags},
        code_requirement::CodeRequirementExpression,
        embedded_signature::{Blob, DigestType, RequirementBlob},
        error::AppleCodesignError,
        macho::{iter_macho, parse_version_nibbles, AppleSignable},
    },
    glob::Pattern,
    goblin::mach::cputype::{
        CpuType, CPU_TYPE_ARM, CPU_TYPE_ARM64, CPU_TYPE_ARM64_32, CPU_TYPE_X86_64,
    },
    log::info,
    reqwest::{IntoUrl, Url},
    std::{
        collections::{BTreeMap, BTreeSet},
        fmt::Formatter,
    },
    x509_certificate::{CapturedX509Certificate, Sign},
};

/// Denotes the scope for a setting.
///
/// Settings have an associated scope defined by this type. This allows settings
/// to apply to exactly what you want them to apply to.
///
/// Scopes can be converted from a string representation. The following syntax is
/// recognized:
///
/// * `@main` - Maps to [SettingsScope::Main]
/// * `@<int>` - e.g. `@0`. Maps to [SettingsScope::MultiArchIndex].Index
/// * `@[cpu_type=<int>]` - e.g. `@[cpu_type=7]`. Maps to [SettingsScope::MultiArchCpuType].
/// * `@[cpu_type=<string>]` - e.g. `@[cpu_type=x86_64]`. Maps to [SettingsScope::MultiArchCpuType]
///    for recognized string values (see below).
/// * `<string>` - e.g. `path/to/file`. Maps to [SettingsScope::Path].
/// * `<string>@<int>` - e.g. `path/to/file@0`. Maps to [SettingsScope::PathMultiArchIndex].
/// * `<string>@[cpu_type=<int>]` - e.g. `path/to/file@[cpu_type=7]`. Maps to
///   [SettingsScope::PathMultiArchCpuType].
/// * `<string>@[cpu_type=<string>]` - e.g. `path/to/file@[cpu_type=arm64]`. Maps to
///   [SettingsScope::PathMultiArchCpuType] for recognized string values (see below).
///
/// # Recognized cpu_type String Values
///
/// The following `cpu_type=` string values are recognized:
///
/// * `arm` -> [CPU_TYPE_ARM]
/// * `arm64` -> [CPU_TYPE_ARM64]
/// * `arm64_32` -> [CPU_TYPE_ARM64_32]
/// * `x86_64` -> [CPU_TYPE_X86_64]
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SettingsScope {
    // The order of the variants is important. Instance cloning iterates keys in
    // sorted order and last write wins. So the order here should be from widest to
    // most granular.
    /// The main entity being signed.
    ///
    /// Can be a Mach-O file, a bundle, or any other primitive this crate
    /// supports signing.
    ///
    /// When signing a bundle or any primitive with nested elements (such as a
    /// fat/universal Mach-O binary), settings can propagate to nested elements.
    Main,

    /// Filesystem path.
    ///
    /// Can refer to a Mach-O file, a nested bundle, or any other filesystem
    /// based primitive that can be traversed into when performing nested signing.
    ///
    /// The string value refers to the filesystem relative path of the entity
    /// relative to the main entity being signed.
    Path(String),

    /// A single Mach-O binary within a fat/universal Mach-O binary.
    ///
    /// The binary to operate on is defined by its 0-based index within the
    /// fat/universal Mach-O container.
    MultiArchIndex(usize),

    /// A single Mach-O binary within a fat/universal Mach-O binary.
    ///
    /// The binary to operate on is defined by its CPU architecture.
    MultiArchCpuType(CpuType),

    /// Combination of [SettingsScope::Path] and [SettingsScope::MultiArchIndex].
    ///
    /// This refers to a single Mach-O binary within a fat/universal binary at a
    /// given relative path.
    PathMultiArchIndex(String, usize),

    /// Combination of [SettingsScope::Path] and [SettingsScope::MultiArchCpuType].
    ///
    /// This refers to a single Mach-O binary within a fat/universal binary at a
    /// given relative path.
    PathMultiArchCpuType(String, CpuType),
}

impl std::fmt::Display for SettingsScope {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Main => f.write_str("main signing target"),
            Self::Path(path) => f.write_fmt(format_args!("path {}", path)),
            Self::MultiArchIndex(index) => f.write_fmt(format_args!(
                "fat/universal Mach-O binaries at index {}",
                index
            )),
            Self::MultiArchCpuType(cpu_type) => f.write_fmt(format_args!(
                "fat/universal Mach-O binaries for CPU {}",
                cpu_type
            )),
            Self::PathMultiArchIndex(path, index) => f.write_fmt(format_args!(
                "fat/universal Mach-O binaries at index {} under path {}",
                index, path
            )),
            Self::PathMultiArchCpuType(path, cpu_type) => f.write_fmt(format_args!(
                "fat/universal Mach-O binaries for CPU {} under path {}",
                cpu_type, path
            )),
        }
    }
}

impl SettingsScope {
    fn parse_at_expr(
        at_expr: &str,
    ) -> Result<(Option<usize>, Option<CpuType>), AppleCodesignError> {
        match at_expr.parse::<usize>() {
            Ok(index) => Ok((Some(index), None)),
            Err(_) => {
                if at_expr.starts_with('[') && at_expr.ends_with(']') {
                    let v = &at_expr[1..at_expr.len() - 1];
                    let parts = v.split('=').collect::<Vec<_>>();

                    if parts.len() == 2 {
                        let (key, value) = (parts[0], parts[1]);

                        if key != "cpu_type" {
                            return Err(AppleCodesignError::ParseSettingsScope(format!(
                                "in '@{}', {} not recognized; must be cpu_type",
                                at_expr, key
                            )));
                        }

                        if let Some(cpu_type) = match value {
                            "arm" => Some(CPU_TYPE_ARM),
                            "arm64" => Some(CPU_TYPE_ARM64),
                            "arm64_32" => Some(CPU_TYPE_ARM64_32),
                            "x86_64" => Some(CPU_TYPE_X86_64),
                            _ => None,
                        } {
                            return Ok((None, Some(cpu_type)));
                        }

                        match value.parse::<u32>() {
                            Ok(cpu_type) => Ok((None, Some(cpu_type as CpuType))),
                            Err(_) => Err(AppleCodesignError::ParseSettingsScope(format!(
                                "in '@{}', cpu_arch value {} not recognized",
                                at_expr, value
                            ))),
                        }
                    } else {
                        Err(AppleCodesignError::ParseSettingsScope(format!(
                            "'{}' sub-expression isn't of form <key>=<value>",
                            v
                        )))
                    }
                } else {
                    Err(AppleCodesignError::ParseSettingsScope(format!(
                        "in '{}', @ expression not recognized",
                        at_expr
                    )))
                }
            }
        }
    }
}

impl AsRef<SettingsScope> for SettingsScope {
    fn as_ref(&self) -> &SettingsScope {
        self
    }
}

impl TryFrom<&str> for SettingsScope {
    type Error = AppleCodesignError;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        if s == "@main" {
            Ok(Self::Main)
        } else if let Some(at_expr) = s.strip_prefix('@') {
            match Self::parse_at_expr(at_expr)? {
                (Some(index), None) => Ok(Self::MultiArchIndex(index)),
                (None, Some(cpu_type)) => Ok(Self::MultiArchCpuType(cpu_type)),
                _ => panic!("this shouldn't happen"),
            }
        } else {
            // Looks like a path.
            let parts = s.rsplitn(2, '@').collect::<Vec<_>>();

            match parts.len() {
                1 => Ok(Self::Path(s.to_string())),
                2 => {
                    // Parts are reversed since splitting at end.
                    let (at_expr, path) = (parts[0], parts[1]);

                    match Self::parse_at_expr(at_expr)? {
                        (Some(index), None) => {
                            Ok(Self::PathMultiArchIndex(path.to_string(), index))
                        }
                        (None, Some(cpu_type)) => {
                            Ok(Self::PathMultiArchCpuType(path.to_string(), cpu_type))
                        }
                        _ => panic!("this shouldn't happen"),
                    }
                }
                _ => panic!("this shouldn't happen"),
            }
        }
    }
}

/// Describes how to derive designated requirements during signing.
#[derive(Clone, Debug)]
pub enum DesignatedRequirementMode {
    /// Automatically attempt to derive an appropriate expression given the
    /// code signing certificate and entity being signed.
    Auto,

    /// Provide an explicit designated requirement.
    Explicit(Vec<Vec<u8>>),
}

/// Represents code signing settings.
///
/// This type holds settings related to a single logical signing operation.
/// Some settings (such as the signing key-pair are global). Other settings
/// (such as the entitlements or designated requirement) can be applied on a
/// more granular, scoped basis. The scoping of these lower-level settings is
/// controlled via [SettingsScope]. If a setting is specified with a scope, it
/// only applies to that scope. See that type's documentation for more.
///
/// An instance of this type is bound to a signing operation. When the
/// signing operation traverses into nested primitives (e.g. when traversing
/// into the individual Mach-O binaries in a fat/universal binary or when
/// traversing into nested bundles or non-main binaries within a bundle), a
/// new instance of this type is transparently constructed by merging global
/// settings with settings for the target scope. This allows granular control
/// over which signing settings apply to which entity and enables a signing
/// operation over a complex primitive to be configured/performed via a single
/// [SigningSettings] and signing operation.
#[derive(Clone, Default)]
pub struct SigningSettings<'key> {
    // Global settings.
    signing_key: Option<(&'key dyn Sign, CapturedX509Certificate)>,
    certificates: Vec<CapturedX509Certificate>,
    time_stamp_url: Option<Url>,
    digest_type: DigestType,
    path_exclusion_patterns: Vec<Pattern>,

    // Scope-specific settings.
    // These are BTreeMap so when we filter the keys, keys with higher precedence come
    // last and last write wins.
    team_id: BTreeMap<SettingsScope, String>,
    identifiers: BTreeMap<SettingsScope, String>,
    entitlements: BTreeMap<SettingsScope, plist::Value>,
    designated_requirement: BTreeMap<SettingsScope, DesignatedRequirementMode>,
    code_signature_flags: BTreeMap<SettingsScope, CodeSignatureFlags>,
    executable_segment_flags: BTreeMap<SettingsScope, ExecutableSegmentFlags>,
    runtime_version: BTreeMap<SettingsScope, semver::Version>,
    info_plist_data: BTreeMap<SettingsScope, Vec<u8>>,
    code_resources_data: BTreeMap<SettingsScope, Vec<u8>>,
    extra_digests: BTreeMap<SettingsScope, BTreeSet<DigestType>>,
}

impl<'key> SigningSettings<'key> {
    /// Obtain the digest type to use.
    pub fn digest_type(&self) -> &DigestType {
        &self.digest_type
    }

    /// Set the content digest to use.
    ///
    /// The default is SHA-256. Changing this to SHA-1 can weaken security of digital
    /// signatures and may prevent the binary from running in environments that enforce
    /// more modern signatures.
    pub fn set_digest_type(&mut self, digest_type: DigestType) {
        self.digest_type = digest_type;
    }

    /// Obtain the signing key to use.
    pub fn signing_key(&self) -> Option<(&'key dyn Sign, &CapturedX509Certificate)> {
        self.signing_key.as_ref().map(|(key, cert)| (*key, cert))
    }

    /// Set the signing key-pair for producing a cryptographic signature over code.
    ///
    /// If this is not called, signing will lack a cryptographic signature and will only
    /// contain digests of content. This is known as "ad-hoc" mode. Binaries lacking a
    /// cryptographic signature or signed without a key-pair issued/signed by Apple may
    /// not run in all environments.
    pub fn set_signing_key(&mut self, private: &'key dyn Sign, public: CapturedX509Certificate) {
        self.signing_key = Some((private, public));
    }

    /// Obtain the certificate chain.
    pub fn certificate_chain(&self) -> &[CapturedX509Certificate] {
        &self.certificates
    }

    /// Attempt to chain Apple CA certificates from a loaded Apple signed signing key.
    ///
    /// If you are calling `set_signing_key()`, you probably want to call this immediately
    /// afterwards, as it will automatically register Apple CA certificates if you are
    /// using an Apple signed code signing certificate.
    pub fn chain_apple_certificates(&mut self) -> Option<Vec<CapturedX509Certificate>> {
        if let Some((_, cert)) = &self.signing_key {
            if let Some(chain) = cert.apple_root_certificate_chain() {
                // The chain starts with self.
                let chain = chain.into_iter().skip(1).collect::<Vec<_>>();
                self.certificates.extend(chain.clone());
                Some(chain)
            } else {
                None
            }
        } else {
            None
        }
    }

    /// Add a parsed certificate to the signing certificate chain.
    ///
    /// When producing a cryptographic signature (see [SigningSettings::set_signing_key]),
    /// information about the signing key-pair is included in the signature. The signing
    /// key's public certificate is always included. This function can be used to define
    /// additional X.509 public certificates to include. Typically, the signing chain
    /// of the signing key-pair up until the root Certificate Authority (CA) is added
    /// so clients have access to the full certificate chain for validation purposes.
    ///
    /// This setting has no effect if [SigningSettings::set_signing_key] is not called.
    pub fn chain_certificate(&mut self, cert: CapturedX509Certificate) {
        self.certificates.push(cert);
    }

    /// Add a DER encoded X.509 public certificate to the signing certificate chain.
    ///
    /// This is like [Self::chain_certificate] except the certificate data is provided in
    /// its binary, DER encoded form.
    pub fn chain_certificate_der(
        &mut self,
        data: impl AsRef<[u8]>,
    ) -> Result<(), AppleCodesignError> {
        self.chain_certificate(CapturedX509Certificate::from_der(data.as_ref())?);

        Ok(())
    }

    /// Add a PEM encoded X.509 public certificate to the signing certificate chain.
    ///
    /// This is like [Self::chain_certificate] except the certificate is
    /// specified as PEM encoded data. This is a human readable string like
    /// `-----BEGIN CERTIFICATE-----` and is a common method for encoding certificate data.
    /// (PEM is effectively base64 encoded DER data.)
    ///
    /// Only a single certificate is read from the PEM data.
    pub fn chain_certificate_pem(
        &mut self,
        data: impl AsRef<[u8]>,
    ) -> Result<(), AppleCodesignError> {
        self.chain_certificate(CapturedX509Certificate::from_pem(data.as_ref())?);

        Ok(())
    }

    /// Obtain the Time-Stamp Protocol server URL.
    pub fn time_stamp_url(&self) -> Option<&Url> {
        self.time_stamp_url.as_ref()
    }

    /// Set the Time-Stamp Protocol server URL to use to generate a Time-Stamp Token.
    ///
    /// When set and a signing key-pair is defined, the server will be contacted during
    /// signing and a Time-Stamp Token will be embedded in the cryptographic signature.
    /// This Time-Stamp Token is a cryptographic proof that someone in possession of
    /// the signing key-pair produced the cryptographic signature at a given time. It
    /// facilitates validation of the signing time via an independent (presumably trusted)
    /// entity.
    pub fn set_time_stamp_url(&mut self, url: impl IntoUrl) -> Result<(), AppleCodesignError> {
        self.time_stamp_url = Some(url.into_url()?);

        Ok(())
    }

    /// Obtain the team identifier for signed binaries.
    pub fn team_id(&self) -> Option<&str> {
        self.team_id.get(&SettingsScope::Main).map(|x| x.as_str())
    }

    /// Set the team identifier for signed binaries.
    pub fn set_team_id(&mut self, value: impl ToString) {
        self.team_id.insert(SettingsScope::Main, value.to_string());
    }

    /// Attempt to set the team ID from the signing certificate.
    ///
    /// Apple signing certificates have the team ID embedded within the certificate.
    /// By calling this method, the team ID embedded within the certificate will
    /// be propagated to the code signature.
    ///
    /// Callers will typically want to call this after registering the signing
    /// certificate with [Self::set_signing_key()] but before specifying an explicit
    /// team ID via [Self::set_team_id()].
    ///
    /// Calling this will replace a registered team IDs if the signing
    /// certificate contains a team ID. If no signing certificate is registered or
    /// it doesn't contain a team ID, no changes will be made.
    ///
    /// Returns `Some` if a team ID was set from the signing certificate or `None`
    /// otherwise.
    pub fn set_team_id_from_signing_certificate(&mut self) -> Option<&str> {
        if let Some((_, cert)) = &self.signing_key {
            if let Some(team_id) = cert.apple_team_id() {
                self.set_team_id(team_id);
                Some(
                    self.team_id
                        .get(&SettingsScope::Main)
                        .expect("we just set a team id"),
                )
            } else {
                None
            }
        } else {
            None
        }
    }

    /// Return relative paths that should be excluded from signing.
    ///
    /// Values are glob pattern matches as defined the by `glob` crate.
    pub fn path_exclusion_patterns(&self) -> &[Pattern] {
        &self.path_exclusion_patterns
    }

    /// Add a path to the exclusions list.
    pub fn add_path_exclusion(&mut self, v: &str) -> Result<(), AppleCodesignError> {
        self.path_exclusion_patterns.push(Pattern::new(v)?);
        Ok(())
    }

    /// Obtain the binary identifier string for a given scope.
    pub fn binary_identifier(&self, scope: impl AsRef<SettingsScope>) -> Option<&str> {
        self.identifiers.get(scope.as_ref()).map(|s| s.as_str())
    }

    /// Set the binary identifier string for a binary at a path.
    ///
    /// This only has an effect when signing an individual Mach-O file (use the `None` path)
    /// or the non-main executable in a bundle: when signing the main executable in a bundle,
    /// the binary's identifier is retrieved from the mandatory `CFBundleIdentifier` value in
    /// the bundle's `Info.plist` file.
    ///
    /// The binary identifier should be a DNS-like name and should uniquely identify the
    /// binary. e.g. `com.example.my_program`
    pub fn set_binary_identifier(&mut self, scope: SettingsScope, value: impl ToString) {
        self.identifiers.insert(scope, value.to_string());
    }

    /// Obtain the entitlements plist as a [plist::Value].
    ///
    /// The value should be a [plist::Value::Dictionary] variant.
    pub fn entitlements_plist(&self, scope: impl AsRef<SettingsScope>) -> Option<&plist::Value> {
        self.entitlements.get(scope.as_ref())
    }

    /// Obtain the entitlements XML string for a given scope.
    pub fn entitlements_xml(
        &self,
        scope: impl AsRef<SettingsScope>,
    ) -> Result<Option<String>, AppleCodesignError> {
        if let Some(value) = self.entitlements_plist(scope) {
            let mut buffer = vec![];
            let writer = std::io::Cursor::new(&mut buffer);
            value
                .to_writer_xml(writer)
                .map_err(AppleCodesignError::PlistSerializeXml)?;

            Ok(Some(
                String::from_utf8(buffer).expect("plist XML serialization should produce UTF-8"),
            ))
        } else {
            Ok(None)
        }
    }

    /// Import existing state from Mach-O data.
    ///
    /// This will synchronize the signing settings with the state in the Mach-O file.
    ///
    /// If existing settings are explicitly set, they will be honored. Otherwise the state from
    /// the Mach-O is imported into the settings.
    pub fn import_settings_from_macho(
        &mut self,
        macho_data: &[u8],
    ) -> Result<(), AppleCodesignError> {
        info!("inferring default signing settings from Mach-O binary");

        for (index, macho) in iter_macho(macho_data)?.enumerate() {
            let scope_main = SettingsScope::Main;
            let scope_index = SettingsScope::MultiArchIndex(index);
            let scope_arch = SettingsScope::MultiArchCpuType(macho.header.cputype());

            // The Mach-O can have embedded Info.plist data. Use it if available and not
            // already defined in settings.
            if let Some(info_plist) = macho.embedded_info_plist()? {
                if self.info_plist_data(&scope_main).is_some()
                    || self.info_plist_data(&scope_index).is_some()
                    || self.info_plist_data(&scope_arch).is_some()
                {
                    info!("using Info.plist data from settings");
                } else {
                    info!("preserving Info.plist data already present in Mach-O");
                    self.set_info_plist_data(scope_index.clone(), info_plist);
                }
            }

            if let Some(sig) = macho.code_signature()? {
                if let Some(cd) = sig.code_directory()? {
                    if self.binary_identifier(&scope_main).is_some()
                        || self.binary_identifier(&scope_index).is_some()
                        || self.binary_identifier(&scope_arch).is_some()
                    {
                        info!("using binary identifier from settings");
                    } else {
                        info!("preserving existing binary identifier in Mach-O");
                        self.set_binary_identifier(scope_index.clone(), cd.ident);
                    }

                    if self.team_id.contains_key(&scope_main)
                        || self.team_id.contains_key(&scope_index)
                        || self.team_id.contains_key(&scope_arch)
                    {
                        info!("using team ID from settings");
                    } else if let Some(team_id) = cd.team_name {
                        info!("preserving team ID in existing Mach-O signature");
                        self.team_id
                            .insert(scope_index.clone(), team_id.to_string());
                    }

                    if self.code_signature_flags(&scope_main).is_some()
                        || self.code_signature_flags(&scope_index).is_some()
                        || self.code_signature_flags(&scope_arch).is_some()
                    {
                        info!("using code signature flags from settings");
                    } else if !cd.flags.is_empty() {
                        info!("preserving code signature flags in existing Mach-O signature");
                        self.set_code_signature_flags(scope_index.clone(), cd.flags);
                    }

                    if self.executable_segment_flags(&scope_main).is_some()
                        || self.executable_segment_flags(&scope_index).is_some()
                        || self.executable_segment_flags(&scope_arch).is_some()
                    {
                        info!("using executable segment flags from settings");
                    } else if let Some(flags) = cd.exec_seg_flags {
                        if !flags.is_empty() {
                            info!(
                                "preserving executable segment flags in existing Mach-O signature"
                            );
                            self.set_executable_segment_flags(scope_index.clone(), flags);
                        }
                    }

                    if self.runtime_version(&scope_main).is_some()
                        || self.runtime_version(&scope_index).is_some()
                        || self.runtime_version(&scope_arch).is_some()
                    {
                        info!("using runtime version from settings");
                    } else if let Some(version) = cd.runtime {
                        info!("preserving runtime version in existing Mach-O signature");
                        self.set_runtime_version(
                            scope_index.clone(),
                            parse_version_nibbles(version),
                        );
                    }
                }

                if let Some(entitlements) = sig.entitlements()? {
                    if self.entitlements_plist(&scope_main).is_some()
                        || self.entitlements_plist(&scope_index).is_some()
                        || self.entitlements_plist(&scope_arch).is_some()
                    {
                        info!("using entitlements from settings");
                    } else {
                        info!("preserving existing entitlements in Mach-O");
                        self.set_entitlements_xml(
                            SettingsScope::MultiArchIndex(index),
                            entitlements.as_str(),
                        )?;
                    }
                }
            }
        }

        Ok(())
    }

    /// Set the entitlements to sign via an XML string.
    ///
    /// The value should be an XML plist. The value is parsed and stored as
    /// a native plist value.
    pub fn set_entitlements_xml(
        &mut self,
        scope: SettingsScope,
        value: impl ToString,
    ) -> Result<(), AppleCodesignError> {
        let cursor = std::io::Cursor::new(value.to_string().into_bytes());
        let value =
            plist::Value::from_reader_xml(cursor).map_err(AppleCodesignError::PlistParseXml)?;

        self.entitlements.insert(scope, value);

        Ok(())
    }

    /// Obtain the designated requirements for a given scope.
    pub fn designated_requirement(
        &self,
        scope: impl AsRef<SettingsScope>,
    ) -> &DesignatedRequirementMode {
        self.designated_requirement
            .get(scope.as_ref())
            .unwrap_or(&DesignatedRequirementMode::Auto)
    }

    /// Set the designated requirement for a Mach-O binary given a [CodeRequirementExpression].
    ///
    /// The designated requirement (also known as "code requirements") specifies run-time
    /// requirements for the binary. e.g. you can stipulate that the binary must be
    /// signed by a certificate issued/signed/chained to Apple. The designated requirement
    /// is embedded in Mach-O binaries and signed.
    pub fn set_designated_requirement_expression(
        &mut self,
        scope: SettingsScope,
        expr: &CodeRequirementExpression,
    ) -> Result<(), AppleCodesignError> {
        self.designated_requirement.insert(
            scope,
            DesignatedRequirementMode::Explicit(vec![expr.to_bytes()?]),
        );

        Ok(())
    }

    /// Set the designated requirement expression for a Mach-O binary given serialized bytes.
    ///
    /// This is like [SigningSettings::set_designated_requirement_expression] except the
    /// designated requirement expression is given as serialized bytes. The bytes passed are
    /// the value that would be produced by compiling a code requirement expression via
    /// `csreq -b`.
    pub fn set_designated_requirement_bytes(
        &mut self,
        scope: SettingsScope,
        data: impl AsRef<[u8]>,
    ) -> Result<(), AppleCodesignError> {
        let blob = RequirementBlob::from_blob_bytes(data.as_ref())?;

        self.designated_requirement.insert(
            scope,
            DesignatedRequirementMode::Explicit(
                blob.parse_expressions()?
                    .iter()
                    .map(|x| x.to_bytes())
                    .collect::<Result<Vec<_>, AppleCodesignError>>()?,
            ),
        );

        Ok(())
    }

    /// Set the designated requirement mode to auto, which will attempt to derive requirements
    /// automatically.
    ///
    /// This setting recognizes when code signing is being performed with Apple issued code signing
    /// certificates and automatically applies appropriate settings for the certificate being
    /// used and the entity being signed.
    ///
    /// Not all combinations may be supported. If you get an error, you will need to
    /// provide your own explicit requirement expression.
    pub fn set_auto_designated_requirement(&mut self, scope: SettingsScope) {
        self.designated_requirement
            .insert(scope, DesignatedRequirementMode::Auto);
    }

    /// Obtain the code signature flags for a given scope.
    pub fn code_signature_flags(
        &self,
        scope: impl AsRef<SettingsScope>,
    ) -> Option<CodeSignatureFlags> {
        self.code_signature_flags.get(scope.as_ref()).copied()
    }

    /// Set code signature flags for signed Mach-O binaries.
    ///
    /// The incoming flags will replace any already-defined flags.
    pub fn set_code_signature_flags(&mut self, scope: SettingsScope, flags: CodeSignatureFlags) {
        self.code_signature_flags.insert(scope, flags);
    }

    /// Add code signature flags.
    ///
    /// The incoming flags will be ORd with any existing flags for the path
    /// specified. The new flags will be returned.
    pub fn add_code_signature_flags(
        &mut self,
        scope: SettingsScope,
        flags: CodeSignatureFlags,
    ) -> CodeSignatureFlags {
        let existing = self
            .code_signature_flags
            .get(&scope)
            .copied()
            .unwrap_or_else(CodeSignatureFlags::empty);

        let new = existing | flags;

        self.code_signature_flags.insert(scope, new);

        new
    }

    /// Remove code signature flags.
    ///
    /// The incoming flags will be removed from any existing flags for the path
    /// specified. The new flags will be returned.
    pub fn remove_code_signature_flags(
        &mut self,
        scope: SettingsScope,
        flags: CodeSignatureFlags,
    ) -> CodeSignatureFlags {
        let existing = self
            .code_signature_flags
            .get(&scope)
            .copied()
            .unwrap_or_else(CodeSignatureFlags::empty);

        let new = existing - flags;

        self.code_signature_flags.insert(scope, new);

        new
    }

    /// Obtain the executable segment flags for a given scope.
    pub fn executable_segment_flags(
        &self,
        scope: impl AsRef<SettingsScope>,
    ) -> Option<ExecutableSegmentFlags> {
        self.executable_segment_flags.get(scope.as_ref()).copied()
    }

    /// Set executable segment flags for Mach-O binaries.
    ///
    /// The incoming flags will replace any already defined flags for the path.
    pub fn set_executable_segment_flags(
        &mut self,
        scope: SettingsScope,
        flags: ExecutableSegmentFlags,
    ) {
        self.executable_segment_flags.insert(scope, flags);
    }

    /// Obtain the `Info.plist` data registered to a given scope.
    pub fn info_plist_data(&self, scope: impl AsRef<SettingsScope>) -> Option<&[u8]> {
        self.info_plist_data
            .get(scope.as_ref())
            .map(|x| x.as_slice())
    }

    /// Obtain the runtime version for a given scope.
    ///
    /// The runtime version represents an OS version.
    pub fn runtime_version(&self, scope: impl AsRef<SettingsScope>) -> Option<&semver::Version> {
        self.runtime_version.get(scope.as_ref())
    }

    /// Set the runtime version to use in the code directory for a given scope.
    ///
    /// The runtime version corresponds to an OS version. The runtime version is usually
    /// derived from the SDK version used to build the binary.
    pub fn set_runtime_version(&mut self, scope: SettingsScope, version: semver::Version) {
        self.runtime_version.insert(scope, version);
    }

    /// Define the `Info.plist` content.
    ///
    /// Signatures can reference the digest of an external `Info.plist` file in
    /// the bundle the binary is located in.
    ///
    /// This function registers the raw content of that file is so that the
    /// content can be digested and the digest can be included in the code directory.
    ///
    /// The value passed here should be the raw content of the `Info.plist` XML file.
    ///
    /// When signing bundles, this function is called automatically with the `Info.plist`
    /// from the bundle. This function exists for cases where you are signing
    /// individual Mach-O binaries and the `Info.plist` cannot be automatically
    /// discovered.
    pub fn set_info_plist_data(&mut self, scope: SettingsScope, data: Vec<u8>) {
        self.info_plist_data.insert(scope, data);
    }

    /// Obtain the `CodeResources` XML file data registered to a given scope.
    pub fn code_resources_data(&self, scope: impl AsRef<SettingsScope>) -> Option<&[u8]> {
        self.code_resources_data
            .get(scope.as_ref())
            .map(|x| x.as_slice())
    }

    /// Define the `CodeResources` XML file content for a given scope.
    ///
    /// Bundles may contain a `CodeResources` XML file which defines additional
    /// resource files and binaries outside the bundle's main executable. The code
    /// directory of the main executable contains a digest of this file to establish
    /// a chain of trust of the content of this XML file.
    ///
    /// This function defines the content of this external file so that the content
    /// can be digested and that digest included in the code directory of the
    /// binary being signed.
    ///
    /// When signing bundles, this function is called automatically with the content
    /// of the `CodeResources` XML file, if present. This function exists for cases
    /// where you are signing individual Mach-O binaries and the `CodeResources` XML
    /// file cannot be automatically discovered.
    pub fn set_code_resources_data(&mut self, scope: SettingsScope, data: Vec<u8>) {
        self.code_resources_data.insert(scope, data);
    }

    /// Obtain extra digests to include in signatures.
    pub fn extra_digests(&self, scope: impl AsRef<SettingsScope>) -> Option<&BTreeSet<DigestType>> {
        self.extra_digests.get(scope.as_ref())
    }

    /// Register an addition content digest to use in signatures.
    ///
    /// Extra digests supplement the primary registered digest when the signer supports
    /// it. Calling this likely results in an additional code directory being included
    /// in embedded signatures.
    ///
    /// A common use case for this is to have the primary digest contain a legacy
    /// digest type (namely SHA-1) but include stronger digests as well. This enables
    /// signatures to have compatibility with older operating systems but still be modern.
    pub fn add_extra_digest(&mut self, scope: SettingsScope, digest_type: DigestType) {
        self.extra_digests
            .entry(scope)
            .or_default()
            .insert(digest_type);
    }

    /// Obtain all configured digests for a scope.
    pub fn all_digests(&self, scope: SettingsScope) -> Vec<DigestType> {
        let mut res = vec![self.digest_type];

        if let Some(extra) = self.extra_digests(scope) {
            res.extend(extra.into_iter());
        }

        res
    }

    /// Convert this instance to settings appropriate for a nested bundle.
    #[must_use]
    pub fn as_nested_bundle_settings(&self, bundle_path: &str) -> Self {
        self.clone_strip_prefix(bundle_path, format!("{}/", bundle_path))
    }

    /// Convert this instance to settings appropriate for a Mach-O binary in a bundle.
    #[must_use]
    pub fn as_bundle_macho_settings(&self, path: &str) -> Self {
        self.clone_strip_prefix(path, path.to_string())
    }

    /// Convert this instance to settings appropriate for a nested Mach-O binary.
    ///
    /// It is assumed the main scope of these settings is already targeted for
    /// a Mach-O binary. Any scoped settings for the Mach-O binary index and CPU type
    /// will be applied. CPU type settings take precedence over index scoped settings.
    #[must_use]
    pub fn as_nested_macho_settings(&self, index: usize, cpu_type: CpuType) -> Self {
        self.clone_with_filter_map(|key| {
            if key == SettingsScope::Main
                || key == SettingsScope::MultiArchCpuType(cpu_type)
                || key == SettingsScope::MultiArchIndex(index)
            {
                Some(SettingsScope::Main)
            } else {
                None
            }
        })
    }

    // Clones this instance, promoting `main_path` to the main scope and stripping
    // a prefix from other keys.
    fn clone_strip_prefix(&self, main_path: &str, prefix: String) -> Self {
        self.clone_with_filter_map(|key| match key {
            SettingsScope::Main => Some(SettingsScope::Main),
            SettingsScope::Path(path) => {
                if path == main_path {
                    Some(SettingsScope::Main)
                } else {
                    path.strip_prefix(&prefix)
                        .map(|path| SettingsScope::Path(path.to_string()))
                }
            }
            SettingsScope::MultiArchIndex(index) => Some(SettingsScope::MultiArchIndex(index)),
            SettingsScope::MultiArchCpuType(cpu_type) => {
                Some(SettingsScope::MultiArchCpuType(cpu_type))
            }
            SettingsScope::PathMultiArchIndex(path, index) => {
                if path == main_path {
                    Some(SettingsScope::MultiArchIndex(index))
                } else {
                    path.strip_prefix(&prefix)
                        .map(|path| SettingsScope::PathMultiArchIndex(path.to_string(), index))
                }
            }
            SettingsScope::PathMultiArchCpuType(path, cpu_type) => {
                if path == main_path {
                    Some(SettingsScope::MultiArchCpuType(cpu_type))
                } else {
                    path.strip_prefix(&prefix)
                        .map(|path| SettingsScope::PathMultiArchCpuType(path.to_string(), cpu_type))
                }
            }
        })
    }

    fn clone_with_filter_map(
        &self,
        key_map: impl Fn(SettingsScope) -> Option<SettingsScope>,
    ) -> Self {
        Self {
            signing_key: self.signing_key.clone(),
            certificates: self.certificates.clone(),
            time_stamp_url: self.time_stamp_url.clone(),
            team_id: self.team_id.clone(),
            digest_type: self.digest_type,
            path_exclusion_patterns: self.path_exclusion_patterns.clone(),
            identifiers: self
                .identifiers
                .clone()
                .into_iter()
                .filter_map(|(key, value)| key_map(key).map(|key| (key, value)))
                .collect::<BTreeMap<_, _>>(),
            entitlements: self
                .entitlements
                .clone()
                .into_iter()
                .filter_map(|(key, value)| key_map(key).map(|key| (key, value)))
                .collect::<BTreeMap<_, _>>(),
            designated_requirement: self
                .designated_requirement
                .clone()
                .into_iter()
                .filter_map(|(key, value)| key_map(key).map(|key| (key, value)))
                .collect::<BTreeMap<_, _>>(),
            code_signature_flags: self
                .code_signature_flags
                .clone()
                .into_iter()
                .filter_map(|(key, value)| key_map(key).map(|key| (key, value)))
                .collect::<BTreeMap<_, _>>(),
            executable_segment_flags: self
                .executable_segment_flags
                .clone()
                .into_iter()
                .filter_map(|(key, value)| key_map(key).map(|key| (key, value)))
                .collect::<BTreeMap<_, _>>(),
            runtime_version: self
                .runtime_version
                .clone()
                .into_iter()
                .filter_map(|(key, value)| key_map(key).map(|key| (key, value)))
                .collect::<BTreeMap<_, _>>(),
            info_plist_data: self
                .info_plist_data
                .clone()
                .into_iter()
                .filter_map(|(key, value)| key_map(key).map(|key| (key, value)))
                .collect::<BTreeMap<_, _>>(),
            code_resources_data: self
                .code_resources_data
                .clone()
                .into_iter()
                .filter_map(|(key, value)| key_map(key).map(|key| (key, value)))
                .collect::<BTreeMap<_, _>>(),
            extra_digests: self
                .extra_digests
                .clone()
                .into_iter()
                .filter_map(|(key, value)| key_map(key).map(|key| (key, value)))
                .collect::<BTreeMap<_, _>>(),
        }
    }
}

#[cfg(test)]
mod tests {
    use {super::*, indoc::indoc};

    const ENTITLEMENTS_XML: &str = indoc! {r#"
        <?xml version="1.0" encoding="UTF-8"?>
        <!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
        <plist version="1.0">
        <dict>
            <key>application-identifier</key>
            <string>appid</string>
            <key>com.apple.developer.team-identifier</key>
            <string>ABCDEF</string>
        </dict>
        </plist>
    "#};

    #[test]
    fn parse_settings_scope() {
        assert_eq!(
            SettingsScope::try_from("@main").unwrap(),
            SettingsScope::Main
        );
        assert_eq!(
            SettingsScope::try_from("@0").unwrap(),
            SettingsScope::MultiArchIndex(0)
        );
        assert_eq!(
            SettingsScope::try_from("@42").unwrap(),
            SettingsScope::MultiArchIndex(42)
        );
        assert_eq!(
            SettingsScope::try_from("@[cpu_type=7]").unwrap(),
            SettingsScope::MultiArchCpuType(7)
        );
        assert_eq!(
            SettingsScope::try_from("@[cpu_type=arm]").unwrap(),
            SettingsScope::MultiArchCpuType(CPU_TYPE_ARM)
        );
        assert_eq!(
            SettingsScope::try_from("@[cpu_type=arm64]").unwrap(),
            SettingsScope::MultiArchCpuType(CPU_TYPE_ARM64)
        );
        assert_eq!(
            SettingsScope::try_from("@[cpu_type=arm64_32]").unwrap(),
            SettingsScope::MultiArchCpuType(CPU_TYPE_ARM64_32)
        );
        assert_eq!(
            SettingsScope::try_from("@[cpu_type=x86_64]").unwrap(),
            SettingsScope::MultiArchCpuType(CPU_TYPE_X86_64)
        );
        assert_eq!(
            SettingsScope::try_from("foo/bar").unwrap(),
            SettingsScope::Path("foo/bar".into())
        );
        assert_eq!(
            SettingsScope::try_from("foo/bar@0").unwrap(),
            SettingsScope::PathMultiArchIndex("foo/bar".into(), 0)
        );
        assert_eq!(
            SettingsScope::try_from("foo/bar@[cpu_type=7]").unwrap(),
            SettingsScope::PathMultiArchCpuType("foo/bar".into(), 7_u32)
        );
    }

    #[test]
    fn as_nested_macho_settings() {
        let mut main_settings = SigningSettings::default();
        main_settings.set_binary_identifier(SettingsScope::Main, "ident");
        main_settings
            .set_code_signature_flags(SettingsScope::Main, CodeSignatureFlags::FORCE_EXPIRATION);

        main_settings.set_code_signature_flags(
            SettingsScope::MultiArchIndex(0),
            CodeSignatureFlags::FORCE_HARD,
        );
        main_settings.set_code_signature_flags(
            SettingsScope::MultiArchCpuType(CPU_TYPE_X86_64),
            CodeSignatureFlags::RESTRICT,
        );
        main_settings.set_info_plist_data(SettingsScope::MultiArchIndex(0), b"index_0".to_vec());
        main_settings.set_info_plist_data(
            SettingsScope::MultiArchCpuType(CPU_TYPE_X86_64),
            b"cpu_x86_64".to_vec(),
        );

        let macho_settings = main_settings.as_nested_macho_settings(0, CPU_TYPE_ARM64);
        assert_eq!(
            macho_settings.binary_identifier(SettingsScope::Main),
            Some("ident")
        );
        assert_eq!(
            macho_settings.code_signature_flags(SettingsScope::Main),
            Some(CodeSignatureFlags::FORCE_HARD)
        );
        assert_eq!(
            macho_settings.info_plist_data(SettingsScope::Main),
            Some(b"index_0".as_ref())
        );

        let macho_settings = main_settings.as_nested_macho_settings(0, CPU_TYPE_X86_64);
        assert_eq!(
            macho_settings.binary_identifier(SettingsScope::Main),
            Some("ident")
        );
        assert_eq!(
            macho_settings.code_signature_flags(SettingsScope::Main),
            Some(CodeSignatureFlags::RESTRICT)
        );
        assert_eq!(
            macho_settings.info_plist_data(SettingsScope::Main),
            Some(b"cpu_x86_64".as_ref())
        );
    }

    #[test]
    fn as_bundle_macho_settings() {
        let mut main_settings = SigningSettings::default();
        main_settings.set_info_plist_data(SettingsScope::Main, b"main".to_vec());
        main_settings.set_info_plist_data(
            SettingsScope::Path("Contents/MacOS/main".into()),
            b"main_exe".to_vec(),
        );
        main_settings.set_info_plist_data(
            SettingsScope::PathMultiArchIndex("Contents/MacOS/main".into(), 0),
            b"main_exe_index_0".to_vec(),
        );
        main_settings.set_info_plist_data(
            SettingsScope::PathMultiArchCpuType("Contents/MacOS/main".into(), CPU_TYPE_X86_64),
            b"main_exe_x86_64".to_vec(),
        );

        let macho_settings = main_settings.as_bundle_macho_settings("Contents/MacOS/main");
        assert_eq!(
            macho_settings.info_plist_data(SettingsScope::Main),
            Some(b"main_exe".as_ref())
        );
        assert_eq!(
            macho_settings.info_plist_data,
            [
                (SettingsScope::Main, b"main_exe".to_vec()),
                (
                    SettingsScope::MultiArchIndex(0),
                    b"main_exe_index_0".to_vec()
                ),
                (
                    SettingsScope::MultiArchCpuType(CPU_TYPE_X86_64),
                    b"main_exe_x86_64".to_vec()
                ),
            ]
            .iter()
            .cloned()
            .collect::<BTreeMap<SettingsScope, Vec<u8>>>()
        );
    }

    #[test]
    fn as_nested_bundle_settings() {
        let mut main_settings = SigningSettings::default();
        main_settings.set_info_plist_data(SettingsScope::Main, b"main".to_vec());
        main_settings.set_info_plist_data(
            SettingsScope::Path("Contents/MacOS/main".into()),
            b"main_exe".to_vec(),
        );
        main_settings.set_info_plist_data(
            SettingsScope::Path("Contents/MacOS/nested.app".into()),
            b"bundle".to_vec(),
        );
        main_settings.set_info_plist_data(
            SettingsScope::PathMultiArchIndex("Contents/MacOS/nested.app".into(), 0),
            b"bundle_index_0".to_vec(),
        );
        main_settings.set_info_plist_data(
            SettingsScope::PathMultiArchCpuType(
                "Contents/MacOS/nested.app".into(),
                CPU_TYPE_X86_64,
            ),
            b"bundle_x86_64".to_vec(),
        );
        main_settings.set_info_plist_data(
            SettingsScope::Path("Contents/MacOS/nested.app/Contents/MacOS/nested".into()),
            b"nested_main_exe".to_vec(),
        );
        main_settings.set_info_plist_data(
            SettingsScope::PathMultiArchIndex(
                "Contents/MacOS/nested.app/Contents/MacOS/nested".into(),
                0,
            ),
            b"nested_main_exe_index_0".to_vec(),
        );
        main_settings.set_info_plist_data(
            SettingsScope::PathMultiArchCpuType(
                "Contents/MacOS/nested.app/Contents/MacOS/nested".into(),
                CPU_TYPE_X86_64,
            ),
            b"nested_main_exe_x86_64".to_vec(),
        );

        let bundle_settings = main_settings.as_nested_bundle_settings("Contents/MacOS/nested.app");
        assert_eq!(
            bundle_settings.info_plist_data(SettingsScope::Main),
            Some(b"bundle".as_ref())
        );
        assert_eq!(
            bundle_settings.info_plist_data(SettingsScope::Path("Contents/MacOS/nested".into())),
            Some(b"nested_main_exe".as_ref())
        );
        assert_eq!(
            bundle_settings.info_plist_data,
            [
                (SettingsScope::Main, b"bundle".to_vec()),
                (SettingsScope::MultiArchIndex(0), b"bundle_index_0".to_vec()),
                (
                    SettingsScope::MultiArchCpuType(CPU_TYPE_X86_64),
                    b"bundle_x86_64".to_vec()
                ),
                (
                    SettingsScope::Path("Contents/MacOS/nested".into()),
                    b"nested_main_exe".to_vec()
                ),
                (
                    SettingsScope::PathMultiArchIndex("Contents/MacOS/nested".into(), 0),
                    b"nested_main_exe_index_0".to_vec()
                ),
                (
                    SettingsScope::PathMultiArchCpuType(
                        "Contents/MacOS/nested".into(),
                        CPU_TYPE_X86_64
                    ),
                    b"nested_main_exe_x86_64".to_vec()
                ),
            ]
            .iter()
            .cloned()
            .collect::<BTreeMap<SettingsScope, Vec<u8>>>()
        );
    }

    #[test]
    fn entitlements_handling() -> Result<(), AppleCodesignError> {
        let mut settings = SigningSettings::default();
        settings.set_entitlements_xml(SettingsScope::Main, ENTITLEMENTS_XML)?;

        let s = settings.entitlements_xml(SettingsScope::Main)?;
        assert_eq!(s, Some("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\">\n<dict>\n\t<key>application-identifier</key>\n\t<string>appid</string>\n\t<key>com.apple.developer.team-identifier</key>\n\t<string>ABCDEF</string>\n</dict>\n</plist>".into()));

        Ok(())
    }
}
