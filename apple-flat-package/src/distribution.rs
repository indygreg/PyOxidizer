// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Distribution XML file format.
//!
//! See https://developer.apple.com/library/archive/documentation/DeveloperTools/Reference/DistributionDefinitionRef/Chapters/Distribution_XML_Ref.html
//! for Apple's documentation of this file format.

use {
    crate::PkgResult,
    serde::{Deserialize, Serialize},
    std::io::Read,
};

/// Represents a distribution XML file.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename = "installer-gui-script", rename_all = "kebab-case")]
pub struct Distribution {
    #[serde(rename = "minSpecVersion")]
    pub min_spec_version: u8,

    // maxSpecVersion and verifiedSpecVersion are reserved attributes but not yet defined.
    pub background: Option<Background>,
    pub choice: Vec<Choice>,
    pub choices_outline: ChoicesOutline,
    pub conclusion: Option<Conclusion>,
    pub domains: Option<Domains>,
    pub installation_check: Option<InstallationCheck>,
    pub license: Option<License>,
    #[serde(default)]
    pub locator: Vec<Locator>,
    pub options: Option<Options>,
    #[serde(default)]
    pub pkg_ref: Vec<PkgRef>,
    pub product: Option<Product>,
    pub readme: Option<Readme>,
    pub script: Option<Script>,
    pub title: Option<Title>,
    pub volume_check: Option<VolumeCheck>,
    pub welcome: Option<Welcome>,
}

impl Distribution {
    /// Parse Distribution XML from a reader.
    pub fn from_reader(reader: impl Read) -> PkgResult<Self> {
        let mut de =
            serde_xml_rs::Deserializer::new_from_reader(reader).non_contiguous_seq_elements(true);

        Ok(Self::deserialize(&mut de)?)
    }

    /// Parse Distribution XML from a string.
    pub fn from_xml(s: &str) -> PkgResult<Self> {
        let mut de = serde_xml_rs::Deserializer::new_from_reader(s.as_bytes())
            .non_contiguous_seq_elements(true);

        Ok(Self::deserialize(&mut de)?)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AllowedOsVersions {
    #[serde(rename = "os-version")]
    os_versions: Vec<OsVersion>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct App {
    pub id: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct Background {
    // TODO convert to enum.
    pub alignment: Option<String>,
    pub file: String,
    pub mime_type: Option<String>,
    // TODO convert to enum
    pub scaling: Option<String>,
    pub uti: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Bundle {
    #[serde(rename = "CFBundleShortVersionString")]
    pub cf_bundle_short_version_string: Option<String>,
    #[serde(rename = "CFBundleVersion")]
    pub cf_bundle_version: Option<String>,
    pub id: String,
    pub path: String,
    pub search: Option<bool>,
    // BuildVersion, SourceVersion reserved attributes.
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BundleVersion {
    #[serde(default)]
    pub bundle: Vec<Bundle>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Choice {
    // The naming format on this element is all over the place.
    #[serde(rename = "customLocation")]
    pub custom_location: Option<String>,
    #[serde(rename = "customLocationAllowAlternateVolumes")]
    pub custom_location_allow_alternative_volumes: Option<bool>,
    pub description: Option<String>,
    #[serde(rename = "description-mime-type")]
    pub description_mime_type: Option<String>,
    pub enabled: Option<bool>,
    pub id: String,
    pub selected: Option<bool>,
    pub start_enabled: Option<bool>,
    pub start_selected: Option<bool>,
    pub start_visible: Option<bool>,
    // Supposed to be required. But there are elements with only `id` attribute in wild.
    pub title: Option<String>,
    pub visible: Option<bool>,
    // bundle, customLocationIsSelfContained, tooltip, and versStr are reserved attributes.
    #[serde(default, rename = "pkg-ref")]
    pub pkg_ref: Vec<PkgRef>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ChoicesOutline {
    // ui is a reserved attribute.
    pub line: Vec<Line>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Conclusion {
    pub file: String,
    #[serde(rename = "mime-type")]
    pub mime_type: Option<String>,
    pub uti: Option<String>,
    // language is a reserved attribute.
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Domains {
    pub enable_anywhere: bool,
    #[serde(rename = "enable_currentUserHome")]
    pub enable_current_user_home: bool,
    #[serde(rename = "enable_localSystem")]
    pub enable_local_system: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct InstallationCheck {
    pub script: Option<bool>,
    pub ram: Option<Ram>,
    #[serde(rename = "required-graphics")]
    pub required_graphics: Option<RequiredGraphics>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct License {
    pub file: String,
    pub mime_type: Option<String>,
    pub uti: Option<String>,
    // auto, language, and sla are reserved but not defined.
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Line {
    pub choice: String,
    #[serde(default, rename = "line")]
    pub lines: Vec<Line>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Locator {
    #[serde(rename = "search")]
    pub searches: Vec<Search>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MustClose {
    pub app: Vec<App>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Options {
    #[serde(rename = "allow-external-scripts")]
    pub allow_external_scripts: Option<bool>,
    pub customize: Option<String>,
    #[serde(rename = "hostArchitectures")]
    pub host_architecutres: Option<String>,
    pub mpkg: Option<String>,
    #[serde(rename = "require-scripts")]
    pub require_scripts: Option<bool>,
    #[serde(rename = "rootVolumeOnly")]
    pub root_volume_only: Option<bool>,
    // type, visibleOnlyForPredicate are reserved attributes.
}

/// Defines a range of supported OS versions.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct OsVersion {
    pub before: Option<String>,
    pub min: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PkgRef {
    pub active: Option<bool>,
    pub auth: Option<String>,
    pub id: String,
    #[serde(rename = "installKBytes")]
    pub install_kbytes: Option<u64>,
    // TODO make enum
    #[serde(rename = "onConclusion")]
    pub on_conclusion: Option<String>,
    #[serde(rename = "onConclusionScript")]
    pub on_conclusion_script: Option<String>,
    pub version: Option<String>,
    // archiveKBytes, packageIdentifier reserved attributes.
    #[serde(rename = "must-close")]
    pub must_close: Option<MustClose>,
    #[serde(rename = "bundle-version")]
    pub bundle_version: Option<BundleVersion>,
    #[serde(default)]
    pub relocate: Vec<Relocate>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Product {
    pub id: String,
    pub version: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Ram {
    #[serde(rename = "min-gb")]
    pub min_gb: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Readme {
    pub file: String,
    pub mime_type: Option<String>,
    pub uti: Option<String>,
    // language is reserved.
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Relocate {
    #[serde(rename = "search-id")]
    pub search_id: String,
    pub bundle: Bundle,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RequiredBundles {
    pub all: Option<bool>,
    pub description: Option<String>,
    #[serde(rename = "bundle")]
    pub bundles: Vec<Bundle>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RequiredClDevice {
    #[serde(rename = "$value")]
    pub predicate: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RequiredGlRenderer {
    #[serde(rename = "$value")]
    pub predicate: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct RequiredGraphics {
    pub description: Option<String>,
    pub single_device: Option<bool>,
    pub required_cl_device: Option<RequiredClDevice>,
    pub required_gl_renderer: Option<RequiredGlRenderer>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Script {
    // language is a reserved attribute.
    #[serde(rename = "$value")]
    pub script: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum SearchValue {
    #[serde(rename = "bundle")]
    Bundle(Bundle),
    #[serde(rename = "script")]
    Script(Script),
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct Search {
    pub id: String,
    pub script: Option<String>,
    pub search_id: Option<String>,
    pub search_path: Option<String>,
    #[serde(rename = "type")]
    pub search_type: String,
    pub value: SearchValue,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Title {
    #[serde(rename = "$value")]
    pub title: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename = "kebab-case")]
pub struct VolumeCheck {
    pub script: bool,
    pub allowed_os_versions: Option<AllowedOsVersions>,
    pub required_bundles: Option<RequiredBundles>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct Welcome {
    pub file: String,
    pub mime_type: Option<String>,
    pub uti: Option<String>,
    // language reserved attribute.
}
