// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Debian package dependency syntax handling.

See [https://www.debian.org/doc/debian-policy/ch-relationships.html] for the specification.
 */

use {
    once_cell::sync::Lazy,
    regex::Regex,
    std::{
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
            (?P<arch>[^\]+])
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
    pub dependency: Option<(VersionRelationship, String)>,
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

                Some((relop, version.as_str().to_string()))
            }
            _ => None,
        };
        let architecture = match (caps.name("arch_negate"), caps.name("arch")) {
            (Some(_), Some(arch)) => Some((true, arch.as_str().to_string())),
            _ => None,
        };

        Ok(Self {
            package,
            dependency,
            architecture,
        })
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

/// Represents an ordered list of dependencies.
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
    /// A dependency list is a comma-delimited list of expressions.
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
                dependency: Some((VersionRelationship::LaterOrEqual, "2.4".into())),
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

        Ok(())
    }
}
