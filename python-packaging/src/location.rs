// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Functionality related to resource locations. */

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

impl Into<String> for ConcreteResourceLocation {
    fn into(self) -> String {
        match self {
            ConcreteResourceLocation::InMemory => "in-memory".to_string(),
            ConcreteResourceLocation::RelativePath(prefix) => {
                format!("filesystem-relative:{}", prefix)
            }
        }
    }
}
