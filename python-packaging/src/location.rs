// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Functionality related to resource locations. */

use std::convert::TryFrom;

/// Describes the location of a Python resource.
///
/// The location is abstract because a concrete location (such as the
/// relative path) is not specified.
#[derive(Clone, Debug, PartialEq)]
pub enum AbstractResourceLocation {
    /// Resource is loaded from memory.
    InMemory,
    /// Resource is loaded from a relative filesystem path.
    RelativePath,
}

impl ToString for &AbstractResourceLocation {
    fn to_string(&self) -> String {
        match self {
            AbstractResourceLocation::InMemory => "in-memory".to_string(),
            AbstractResourceLocation::RelativePath => "filesystem-relative".to_string(),
        }
    }
}

impl TryFrom<&str> for AbstractResourceLocation {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "in-memory" => Ok(Self::InMemory),
            "filesystem-relative" => Ok(Self::RelativePath),
            _ => Err(format!("{} is not a valid resource location", value)),
        }
    }
}

/// Describes the concrete location of a Python resource.
#[derive(Clone, Debug, PartialEq)]
pub enum ConcreteResourceLocation {
    /// Resource is loaded from memory.
    InMemory,
    /// Reosurce is loaded from a relative filesystem path.
    RelativePath(String),
}

impl From<&ConcreteResourceLocation> for AbstractResourceLocation {
    fn from(l: &ConcreteResourceLocation) -> Self {
        match l {
            ConcreteResourceLocation::InMemory => AbstractResourceLocation::InMemory,
            ConcreteResourceLocation::RelativePath(_) => AbstractResourceLocation::RelativePath,
        }
    }
}

impl ToString for ConcreteResourceLocation {
    fn to_string(&self) -> String {
        match self {
            ConcreteResourceLocation::InMemory => "in-memory".to_string(),
            ConcreteResourceLocation::RelativePath(prefix) => {
                format!("filesystem-relative:{}", prefix)
            }
        }
    }
}

impl From<ConcreteResourceLocation> for String {
    fn from(location: ConcreteResourceLocation) -> Self {
        location.to_string()
    }
}

impl TryFrom<&str> for ConcreteResourceLocation {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        if value == "in-memory" {
            Ok(Self::InMemory)
        } else {
            let parts = value.splitn(2, ':').collect::<Vec<_>>();

            if parts.len() != 2 {
                Err(format!("{} is not a valid resource location", value))
            } else {
                let prefix = parts[0];
                let suffix = parts[1];

                if prefix == "filesystem-relative" {
                    Ok(Self::RelativePath(suffix.to_string()))
                } else {
                    Err(format!("{} is not a valid resource location", value))
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use {super::*, anyhow::Result};

    #[test]
    fn test_abstract_from_string() -> Result<()> {
        assert_eq!(
            AbstractResourceLocation::try_from("in-memory"),
            Ok(AbstractResourceLocation::InMemory)
        );
        assert_eq!(
            AbstractResourceLocation::try_from("filesystem-relative"),
            Ok(AbstractResourceLocation::RelativePath)
        );

        Ok(())
    }

    #[test]
    fn test_concrete_from_string() -> Result<()> {
        assert_eq!(
            ConcreteResourceLocation::try_from("in-memory"),
            Ok(ConcreteResourceLocation::InMemory)
        );
        assert_eq!(
            ConcreteResourceLocation::try_from("filesystem-relative:lib"),
            Ok(ConcreteResourceLocation::RelativePath("lib".to_string()))
        );

        Ok(())
    }
}
