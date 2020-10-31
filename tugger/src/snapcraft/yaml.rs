// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    serde::{Deserialize, Serialize},
    std::{borrow::Cow, collections::HashMap, convert::TryFrom},
};

/// Represents the value of the `type` field.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Type {
    Gadget,
    Kernel,
    Base,
}

impl TryFrom<&str> for Type {
    type Error = serde_yaml::Error;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        serde_yaml::from_str(s)
    }
}

/// Represents the value of an architecture in an `architectures` field.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Architecture {
    All,
    S390x,
    Ppc64el,
    Arm64,
    Armhf,
    Amd64,
    I386,
}

impl TryFrom<&str> for Architecture {
    type Error = serde_yaml::Error;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        serde_yaml::from_str(s)
    }
}

/// Represents the value of a `confinement` field.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Confinement {
    Strict,
    Devmode,
    Classic,
}

impl TryFrom<&str> for Confinement {
    type Error = serde_yaml::Error;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        serde_yaml::from_str(s)
    }
}

/// Represents the value of a `grade` field.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Grade {
    Devel,
    Stable,
}

impl TryFrom<&str> for Grade {
    type Error = serde_yaml::Error;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        serde_yaml::from_str(s)
    }
}

/// Represents the value of an `adapter` field.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Adapter {
    None,
    Full,
}

impl TryFrom<&str> for Adapter {
    type Error = serde_yaml::Error;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        serde_yaml::from_str(s)
    }
}

/// Represents the value of a `daemon` field.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Daemon {
    Simple,
    Oneshot,
    Forking,
    Notify,
}

impl TryFrom<&str> for Daemon {
    type Error = serde_yaml::Error;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        serde_yaml::from_str(s)
    }
}

/// Represents the value of a `restart-condition` field.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RestartCondition {
    OnFailure,
    OnSuccess,
    OnAbnormal,
    OnAbort,
    Always,
    Never,
}

impl TryFrom<&str> for RestartCondition {
    type Error = serde_yaml::Error;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        serde_yaml::from_str(s)
    }
}

/// Represents the value of a `source-type` field.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceType {
    Bzr,
    Deb,
    Git,
    Hg,
    Local,
    Mercurial,
    Rpm,
    Subversion,
    Svn,
    Tar,
    Zip,
    #[serde(rename = "7z")]
    SevenZip,
}

impl TryFrom<&str> for SourceType {
    type Error = serde_yaml::Error;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        serde_yaml::from_str(s)
    }
}

/// Represents the values in a `build-attributes` field.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BuildAttribute {
    Debug,
    KeepExecstack,
    NoPatchelf,
    EnablePatchelf,
    NoInstall,
}

impl TryFrom<&str> for BuildAttribute {
    type Error = serde_yaml::Error;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        serde_yaml::from_str(s)
    }
}

/// Represents the value of an `architecture` field.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Architectures {
    pub build_on: Vec<Architecture>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub run_on: Vec<Architecture>,
}

/// Represents the `apps.<app-name>` entries in a `snapcraft.yaml`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct SnapApp<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adapter: Option<Adapter>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub autostart: Option<Cow<'a, str>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<Cow<'a, str>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub command_chain: Vec<Cow<'a, str>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub common_id: Option<Cow<'a, str>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub daemon: Option<Daemon>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub desktop: Option<Cow<'a, str>>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub environment: HashMap<Cow<'a, str>, Cow<'a, str>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extensions: Vec<Cow<'a, str>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub plugs: Vec<Cow<'a, str>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub slots: Vec<Cow<'a, str>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_command: Option<Cow<'a, str>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub post_stop_command: Option<Cow<'a, str>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_timeout: Option<Cow<'a, str>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timer: Option<Cow<'a, str>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub restart_condition: Option<RestartCondition>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub socket: HashMap<Cow<'a, str>, Cow<'a, str>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub socket_mode: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub listen_stream: Option<Cow<'a, str>>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub passthrough: HashMap<Cow<'a, str>, Cow<'a, str>>,
}

/// Represents the `parts.<part-name>` entries in a `snapcraft.yaml`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct SnapPart<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugin: Option<Cow<'a, str>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<Cow<'a, str>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_type: Option<SourceType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_checksum: Option<Cow<'a, str>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_depth: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_branch: Option<Cow<'a, str>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_commit: Option<Cow<'a, str>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_tag: Option<Cow<'a, str>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_subdir: Option<Cow<'a, str>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub after: Vec<Cow<'a, str>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub build_environment: Vec<HashMap<Cow<'a, str>, Cow<'a, str>>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub build_snaps: Vec<Cow<'a, str>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub build_packages: Vec<Cow<'a, str>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stage_packages: Vec<Cow<'a, str>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stage_snaps: Vec<Cow<'a, str>>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub organize: HashMap<Cow<'a, str>, Cow<'a, str>>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub filesets: HashMap<Cow<'a, str>, Vec<Cow<'a, str>>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stage: Vec<Cow<'a, str>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parse_info: Option<Cow<'a, str>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub prime: Vec<Cow<'a, str>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub override_build: Option<Cow<'a, str>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub override_prime: Option<Cow<'a, str>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub override_pull: Option<Cow<'a, str>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub override_stage: Option<Cow<'a, str>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub build_attributes: Vec<BuildAttribute>,
}

/// Represents a `snapcraft.yaml` file content.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Snapcraft<'a> {
    pub name: Cow<'a, str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<Cow<'a, str>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base: Option<Cow<'a, str>>,
    pub version: Cow<'a, str>,
    pub summary: Cow<'a, str>,
    pub description: Cow<'a, str>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub snap_type: Option<Type>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confinement: Option<Confinement>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<Cow<'a, str>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub license: Option<Cow<'a, str>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grade: Option<Grade>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adopt_info: Option<Cow<'a, str>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub architectures: Option<Architectures>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assumes: Vec<Cow<'a, str>>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub passthrough: HashMap<Cow<'a, str>, Cow<'a, str>>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub apps: HashMap<Cow<'a, str>, SnapApp<'a>>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub parts: HashMap<Cow<'a, str>, SnapPart<'a>>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub plugs: HashMap<Cow<'a, str>, HashMap<Cow<'a, str>, Cow<'a, str>>>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub slots: HashMap<Cow<'a, str>, HashMap<Cow<'a, str>, Cow<'a, str>>>,
}

impl<'a> Snapcraft<'a> {
    pub fn new(
        name: Cow<'a, str>,
        version: Cow<'a, str>,
        summary: Cow<'a, str>,
        description: Cow<'a, str>,
    ) -> Self {
        Self {
            name,
            version,
            summary,
            description,
            title: None,
            base: None,
            snap_type: None,
            confinement: None,
            icon: None,
            license: None,
            grade: None,
            adopt_info: None,
            architectures: None,
            assumes: vec![],
            passthrough: HashMap::new(),
            apps: HashMap::new(),
            parts: HashMap::new(),
            plugs: HashMap::new(),
            slots: HashMap::new(),
        }
    }

    /// Add a named application to this instance.
    pub fn add_app(&mut self, name: Cow<'a, str>, app: SnapApp<'a>) {
        self.apps.insert(name, app);
    }

    /// Add a named part to this instance.
    pub fn add_part(&mut self, name: Cow<'a, str>, part: SnapPart<'a>) {
        self.parts.insert(name, part);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_type_from_str() -> Result<(), serde_yaml::Error> {
        let t = Type::try_from("gadget")?;
        assert_eq!(t, Type::Gadget);

        Ok(())
    }

    #[test]
    fn test_architecture_from_str() -> Result<(), serde_yaml::Error> {
        assert_eq!(Architecture::try_from("all")?, Architecture::All);
        assert_eq!(Architecture::try_from("s390x")?, Architecture::S390x);
        assert_eq!(Architecture::try_from("ppc64el")?, Architecture::Ppc64el);

        Ok(())
    }

    #[test]
    fn test_source_type_from_str() -> Result<(), serde_yaml::Error> {
        assert_eq!(SourceType::try_from("7z")?, SourceType::SevenZip);

        Ok(())
    }
}
