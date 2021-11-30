// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Debian package dependency syntax handling.

See [https://www.debian.org/doc/debian-policy/ch-relationships.html] for the specification.
 */

use {
    crate::{
        control::ControlParagraph,
        package_version::{PackageVersion, VersionError},
    },
    once_cell::sync::Lazy,
    regex::Regex,
    std::{
        cmp::Ordering,
        fmt::{Display, Formatter},
        ops::{Deref, DerefMut},
    },
    thiserror::Error,
};

/// Regular expression to parse dependency expressions.
pub static RE_DEPENDENCY: Lazy<Regex> = Lazy::new(|| {
    // TODO <> is a legacy syntax.
    Regex::new(
        r#"(?x)
        # Package name is alphanumeric, terminating at whitespace, [ or (
        (?P<package>[^\s\[(]+)
        # Any number of optional spaces.
        \s*
        # Relationships are within an optional parenthesis.
        (?:\(
            # Optional spaces after (
            \s*
            # The relationship operator.
            (?P<relop>(<<|<=|=|>=|>>))
            # Optional spaces after the operator.
            \s*
            # Version string is everything up to space or closing parenthesis.
            (?P<version>[^\s)]+)
            # Trailing space before ).
            \s*
        \))?
        # Any amount of space after optional relationship definition.
        \s*
        # Architecture restrictions are within an optional [..] field.
        (?:\[
            # Optional whitespace after [
            \s*
            # Optional negation operator.
            (?P<arch_negate>!)?
            \s*
            # The architecture. May have spaces.
            (?P<arch>[^\]]+)
        \])?
        "#,
    )
    .unwrap()
});

/// Errors related to dependency handling.
#[derive(Debug, Error)]
pub enum DependencyError {
    #[error("failed to parse dependency expression: {0}")]
    DependencyParse(String),

    #[error("version parsing error: {0:?}")]
    Version(#[from] VersionError),
}

/// Result type for dependency handling.
pub type Result<T> = std::result::Result<T, DependencyError>;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VersionRelationship {
    StrictlyEarlier,
    EarlierOrEqual,
    ExactlyEqual,
    LaterOrEqual,
    StrictlyLater,
}

impl Display for VersionRelationship {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        match self {
            Self::StrictlyEarlier => write!(f, "<<"),
            Self::EarlierOrEqual => write!(f, "<="),
            Self::ExactlyEqual => write!(f, "="),
            Self::LaterOrEqual => write!(f, ">="),
            Self::StrictlyLater => write!(f, ">>"),
        }
    }
}

/// A dependency of a package.
#[derive(Clone, Debug, PartialEq)]
pub struct SingleDependency {
    /// Package the dependency is on.
    pub package: String,
    pub dependency: Option<(VersionRelationship, PackageVersion)>,
    pub architecture: Option<(bool, String)>,
}

impl Display for SingleDependency {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        write!(f, "{}", self.package)?;
        if let Some((rel, version)) = &self.dependency {
            write!(f, " ({} {})", rel, version)?;
        }
        if let Some((negate, arch)) = &self.architecture {
            write!(f, " [{}{}]", if *negate { "!" } else { "" }, arch)?;
        }

        Ok(())
    }
}

impl SingleDependency {
    /// Parse a single package dependency expression into a [SingleDependency].
    pub fn parse(s: &str) -> Result<Self> {
        let caps = RE_DEPENDENCY
            .captures(s)
            .ok_or_else(|| DependencyError::DependencyParse(s.to_string()))?;

        let package = caps["package"].to_string();
        let dependency = match (caps.name("relop"), caps.name("version")) {
            (Some(relop), Some(version)) => {
                let relop = match relop.as_str() {
                    "<<" => VersionRelationship::StrictlyEarlier,
                    "<=" => VersionRelationship::EarlierOrEqual,
                    "=" => VersionRelationship::ExactlyEqual,
                    ">=" => VersionRelationship::LaterOrEqual,
                    ">>" => VersionRelationship::StrictlyLater,
                    v => panic!("unexpected version relationship: {}", v),
                };

                let version = PackageVersion::parse(version.as_str())?;

                Some((relop, version))
            }
            _ => None,
        };

        let architecture = match (caps.name("arch_negate"), caps.name("arch")) {
            (Some(_), Some(arch)) => Some((true, arch.as_str().to_string())),
            (None, Some(arch)) => Some((false, arch.as_str().to_string())),
            _ => None,
        };

        Ok(Self {
            package,
            dependency,
            architecture,
        })
    }

    /// Evaluate whether a package satisfies the requirements of this parsed expression.
    ///
    /// This takes as arguments the low-level package components needed for checking.
    pub fn package_satisfies(
        &self,
        package: &str,
        version: &PackageVersion,
        architecture: &str,
    ) -> bool {
        if self.package == package {
            if let Some((negate, arch)) = &self.architecture {
                // Requesting an arch mismatch.
                if (*negate && arch == architecture) || (!*negate && arch != architecture) {
                    return false;
                }
            }

            // Package and arch requirements match. Go on to version compare.
            if let Some((wanted_relationship, wanted_version)) = &self.dependency {
                matches!(
                    (version.cmp(wanted_version), *wanted_relationship),
                    (
                        Ordering::Equal,
                        VersionRelationship::ExactlyEqual
                            | VersionRelationship::LaterOrEqual
                            | VersionRelationship::EarlierOrEqual,
                    ) | (
                        Ordering::Less,
                        VersionRelationship::StrictlyEarlier | VersionRelationship::EarlierOrEqual,
                    ) | (
                        Ordering::Greater,
                        VersionRelationship::StrictlyLater | VersionRelationship::LaterOrEqual,
                    )
                )
            } else {
                // No version constraint means yes.
                true
            }
        } else {
            false
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DependencyVariants(Vec<SingleDependency>);

impl Display for DependencyVariants {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        write!(
            f,
            "{}",
            self.0
                .iter()
                .map(|x| format!("{}", x))
                .collect::<Vec<_>>()
                .join(" | ")
        )
    }
}

impl Deref for DependencyVariants {
    type Target = Vec<SingleDependency>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for DependencyVariants {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl DependencyVariants {
    /// Evaluate whether a package satisfies the requirements of this set of variants.
    ///
    /// This calls [SingleDependency.satisfies()] for each tracked variant. Returns true
    /// if the given package satisfies any variant.
    pub fn package_satisfies(&self, package: &str, version: &PackageVersion, arch: &str) -> bool {
        self.0
            .iter()
            .any(|variant| variant.package_satisfies(package, version, arch))
    }
}

/// Represents an ordered list of dependencies, delimited by commas (`,`).
#[derive(Clone, Debug, PartialEq)]
pub struct DependencyList {
    dependencies: Vec<DependencyVariants>,
}

impl Display for DependencyList {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        write!(
            f,
            "{}",
            self.dependencies
                .iter()
                .map(|x| format!("{}", x))
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

impl DependencyList {
    /// Parse a dependency list from a string.
    ///
    /// A dependency list is a comma-delimited list of expressions. Each expression is a
    /// `|` delimited list of expressions of the form
    /// `package (version_relationship version) [arch]`.
    pub fn parse(s: &str) -> Result<Self> {
        let mut els = vec![];

        for el in s.split(',') {
            // Interior whitespace doesn't matter.
            let el = el.trim();

            // Each dependency consists of alternatives split by |.
            let mut variants = DependencyVariants::default();

            for alt in el.split('|') {
                let alt = alt.trim();

                variants.push(SingleDependency::parse(alt)?);
            }

            els.push(variants);
        }

        Ok(Self { dependencies: els })
    }

    /// Evaluate whether a package satisfies at least one expression in this list.
    pub fn package_satisfies(&self, package: &str, version: &PackageVersion, arch: &str) -> bool {
        self.dependencies
            .iter()
            .any(|variants| variants.package_satisfies(package, version, arch))
    }

    /// Obtain the individual requirements constituting this list of dependencies.
    ///
    /// Each requirement is itself a set of expressions to match against. The length of
    /// this set is commonly 1.
    pub fn requirements(&self) -> impl Iterator<Item = &DependencyVariants> {
        self.dependencies.iter()
    }
}

/// Holds all fields related to package dependency metadata.
///
/// Instances of this type effectively describe the relationships between the package it
/// describes and other packages.
///
/// See [https://www.debian.org/doc/debian-policy/ch-relationships.html] for a list of all the
/// fields and what they mean.
#[derive(Clone, Debug)]
pub struct PackageDependencyFields {
    /// `Depends`.
    pub depends: Option<DependencyList>,

    /// `Recommends`.
    pub recommends: Option<DependencyList>,

    /// `Suggests`.
    pub suggests: Option<DependencyList>,

    /// `Enhances`.
    pub enhances: Option<DependencyList>,

    /// `Pre-Depends`.
    pub pre_depends: Option<DependencyList>,

    /// `Breaks`.
    pub breaks: Option<DependencyList>,

    /// `Conflicts`.
    pub conflicts: Option<DependencyList>,

    /// `Provides`.
    pub provides: Option<DependencyList>,

    /// `Replaces`.
    pub replaces: Option<DependencyList>,

    /// `Build-Depends`.
    pub build_depends: Option<DependencyList>,

    /// `Build-Depends-Indep`.
    pub build_depends_indep: Option<DependencyList>,

    /// `Build-Depends-Arch`.
    pub build_depends_arch: Option<DependencyList>,

    /// `Build-Conflicts`.
    pub build_conflicts: Option<DependencyList>,

    /// `Build-Conflicts-Indep`.
    pub build_conflicts_indep: Option<DependencyList>,

    /// `Build-Conflicts-Arch`.
    pub build_conflicts_arch: Option<DependencyList>,

    /// `Built-Using`.
    pub built_using: Option<DependencyList>,
}

impl PackageDependencyFields {
    /// Construct an instance from a control paragraph.
    pub fn from_paragraph(para: &ControlParagraph) -> Result<Self> {
        let get_field = |field| -> Result<Option<DependencyList>> {
            if let Some(value) = para.first_field_str(field) {
                Ok(Some(DependencyList::parse(value)?))
            } else {
                Ok(None)
            }
        };

        Ok(Self {
            depends: get_field("Depends")?,
            recommends: get_field("Recommends")?,
            suggests: get_field("Suggests")?,
            enhances: get_field("Enhances")?,
            pre_depends: get_field("Pre-Depends")?,
            breaks: get_field("Breaks")?,
            conflicts: get_field("Conflicts")?,
            provides: get_field("Provides")?,
            replaces: get_field("Replaces")?,
            build_depends: get_field("Build-Depends")?,
            build_depends_indep: get_field("Build-Depends-Indep")?,
            build_depends_arch: get_field("Build-Depends-Arch")?,
            build_conflicts: get_field("Build-Conflicts")?,
            build_conflicts_indep: get_field("Build-Conflicts-Indep")?,
            build_conflicts_arch: get_field("Build-Conflicts-Arch")?,
            built_using: get_field("Built-Using")?,
        })
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn parse_depends() -> Result<()> {
        let dl = DependencyList::parse("libc6 (>= 2.4), libx11-6")?;
        assert_eq!(dl.dependencies.len(), 2);
        assert_eq!(dl.dependencies[0].0.len(), 1);
        assert_eq!(dl.dependencies[1].0.len(), 1);

        assert_eq!(
            dl.dependencies[0].0[0],
            SingleDependency {
                package: "libc6".into(),
                dependency: Some((
                    VersionRelationship::LaterOrEqual,
                    PackageVersion::parse("2.4").unwrap(),
                )),
                architecture: None,
            }
        );
        assert_eq!(
            dl.dependencies[1].0[0],
            SingleDependency {
                package: "libx11-6".into(),
                dependency: None,
                architecture: None,
            }
        );

        let dl = DependencyList::parse("libc [amd64]")?;
        assert_eq!(dl.dependencies.len(), 1);
        assert_eq!(dl.dependencies[0].0.len(), 1);
        assert_eq!(
            dl.dependencies[0].0[0],
            SingleDependency {
                package: "libc".into(),
                dependency: None,
                architecture: Some((false, "amd64".into())),
            }
        );

        let dl = DependencyList::parse("libc [!amd64]")?;
        assert_eq!(dl.dependencies.len(), 1);
        assert_eq!(dl.dependencies[0].0.len(), 1);
        assert_eq!(
            dl.dependencies[0].0[0],
            SingleDependency {
                package: "libc".into(),
                dependency: None,
                architecture: Some((true, "amd64".into())),
            }
        );

        Ok(())
    }

    #[test]
    fn satisfies_version_constraints() -> Result<()> {
        let dl = DependencyList::parse("libc (= 2.4)")?;
        assert!(dl.dependencies[0].package_satisfies(
            "libc",
            &PackageVersion::parse("2.4")?,
            "ignored"
        ));
        assert!(!dl.dependencies[0].package_satisfies(
            "libc",
            &PackageVersion::parse("2.3")?,
            "ignored"
        ));
        assert!(!dl.dependencies[0].package_satisfies(
            "libc",
            &PackageVersion::parse("2.5")?,
            "ignored"
        ));
        assert!(!dl.dependencies[0].package_satisfies(
            "other",
            &PackageVersion::parse("2.4")?,
            "ignored"
        ));

        let dl = DependencyList::parse("libc (<= 2.4)")?;
        assert!(dl.dependencies[0].package_satisfies(
            "libc",
            &PackageVersion::parse("2.3")?,
            "ignored"
        ));
        assert!(dl.dependencies[0].package_satisfies(
            "libc",
            &PackageVersion::parse("2.4")?,
            "ignored"
        ));
        assert!(!dl.dependencies[0].package_satisfies(
            "libc",
            &PackageVersion::parse("2.5")?,
            "ignored"
        ));
        assert!(!dl.dependencies[0].package_satisfies(
            "other",
            &PackageVersion::parse("2.4")?,
            "ignored"
        ));

        let dl = DependencyList::parse("libc (>= 2.4)")?;
        assert!(!dl.dependencies[0].package_satisfies(
            "libc",
            &PackageVersion::parse("2.3")?,
            "ignored"
        ));
        assert!(dl.dependencies[0].package_satisfies(
            "libc",
            &PackageVersion::parse("2.4")?,
            "ignored"
        ));
        assert!(dl.dependencies[0].package_satisfies(
            "libc",
            &PackageVersion::parse("2.5")?,
            "ignored"
        ));
        assert!(!dl.dependencies[0].package_satisfies(
            "other",
            &PackageVersion::parse("2.4")?,
            "ignored"
        ));

        let dl = DependencyList::parse("libc (<< 2.4)")?;
        assert!(dl.dependencies[0].package_satisfies(
            "libc",
            &PackageVersion::parse("2.3")?,
            "ignored"
        ));
        assert!(!dl.dependencies[0].package_satisfies(
            "libc",
            &PackageVersion::parse("2.4")?,
            "ignored"
        ));
        assert!(!dl.dependencies[0].package_satisfies(
            "libc",
            &PackageVersion::parse("2.5")?,
            "ignored"
        ));
        assert!(!dl.dependencies[0].package_satisfies(
            "other",
            &PackageVersion::parse("2.3")?,
            "ignored"
        ));

        let dl = DependencyList::parse("libc (>> 2.4)")?;
        assert!(!dl.dependencies[0].package_satisfies(
            "libc",
            &PackageVersion::parse("2.3")?,
            "ignored"
        ));
        assert!(!dl.dependencies[0].package_satisfies(
            "libc",
            &PackageVersion::parse("2.4")?,
            "ignored"
        ));
        assert!(dl.dependencies[0].package_satisfies(
            "libc",
            &PackageVersion::parse("2.5")?,
            "ignored"
        ));
        assert!(!dl.dependencies[0].package_satisfies(
            "other",
            &PackageVersion::parse("2.5")?,
            "ignored"
        ));

        Ok(())
    }

    #[test]
    fn satisfies_architecture_constraints() -> Result<()> {
        let dl = DependencyList::parse("libc [amd64]")?;

        assert!(dl.dependencies[0].package_satisfies(
            "libc",
            &PackageVersion::parse("2.4")?,
            "amd64"
        ));
        assert!(!dl.dependencies[0].package_satisfies(
            "libc",
            &PackageVersion::parse("2.3")?,
            "x86"
        ));

        let dl = DependencyList::parse("libc [!amd64]")?;
        assert!(!dl.dependencies[0].package_satisfies(
            "libc",
            &PackageVersion::parse("2.4")?,
            "amd64"
        ));
        assert!(dl.dependencies[0].package_satisfies(
            "libc",
            &PackageVersion::parse("2.3")?,
            "x86"
        ));

        Ok(())
    }
}
