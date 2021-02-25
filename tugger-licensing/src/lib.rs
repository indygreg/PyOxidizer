// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    anyhow::{anyhow, Result},
    spdx::{ExceptionId, Expression, LicenseId},
    std::{
        cmp::Ordering,
        collections::{BTreeMap, BTreeSet},
    },
};

/// The type of a license.
#[derive(Clone, Debug, PartialEq)]
pub enum LicenseFlavor {
    /// No explicit licensing defined.
    None,

    /// An SPDX license expression.
    Spdx(Expression),

    /// An SPDX expression that contain unknown license identifiers.
    OtherExpression(Expression),

    /// License is in the public domain.
    PublicDomain,

    /// Unknown licensing type with available string identifiers.
    Unknown(Vec<String>),
}

/// Describes the type of a software component.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ComponentFlavor {
    /// No specific component type.
    Generic,
    /// A generic software library.
    Library,
    /// A Rust crate.
    RustCrate,
    /// A Python package.
    PythonPackage,
}

impl ToString for ComponentFlavor {
    fn to_string(&self) -> String {
        match self {
            Self::Generic => "generic".to_string(),
            Self::Library => "library".to_string(),
            Self::RustCrate => "Rust crate".to_string(),
            Self::PythonPackage => "Python package".to_string(),
        }
    }
}

impl PartialOrd for ComponentFlavor {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.to_string().partial_cmp(&other.to_string())
    }
}

impl Ord for ComponentFlavor {
    fn cmp(&self, other: &Self) -> Ordering {
        self.partial_cmp(other).unwrap()
    }
}

/// Where source code for a component can be obtained from.
#[derive(Clone, Debug, PartialEq)]
pub enum SourceLocation {
    /// Source code is not available.
    NotSet,
    /// Source code is available at a URL.
    Url(String),
}

/// Represents a software component with licensing information.
#[derive(Clone, Debug, PartialEq)]
pub struct LicensedComponent {
    /// Name of this software component.
    name: String,

    /// Type of component.
    flavor: ComponentFlavor,

    /// The type of license.
    license: LicenseFlavor,

    /// Location where source code for this component can be obtained.
    source_location: SourceLocation,

    /// Specified license text for this component.
    ///
    /// If empty, license texts will be derived from SPDX identifiers, if available.
    license_texts: Vec<String>,
}

impl PartialOrd for LicensedComponent {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        if self.name == other.name {
            self.flavor.partial_cmp(&other.flavor)
        } else {
            self.name.partial_cmp(&other.name)
        }
    }
}

impl LicensedComponent {
    /// Construct a new instance from an SPDX expression.
    pub fn new_spdx(name: &str, spdx_expression: &str) -> Result<Self> {
        let spdx_expression = Expression::parse(spdx_expression).map_err(|e| anyhow!("{}", e))?;

        let license = if spdx_expression.evaluate(|req| req.license.id().is_some()) {
            LicenseFlavor::Spdx(spdx_expression)
        } else {
            LicenseFlavor::OtherExpression(spdx_expression)
        };

        Ok(Self {
            name: name.to_string(),
            flavor: ComponentFlavor::Generic,
            license,
            source_location: SourceLocation::NotSet,
            license_texts: vec![],
        })
    }

    /// Construct a new instance with no licensing defined.
    pub fn new_none(name: &str) -> Self {
        Self {
            name: name.to_string(),
            flavor: ComponentFlavor::Generic,
            license: LicenseFlavor::None,
            source_location: SourceLocation::NotSet,
            license_texts: vec![],
        }
    }

    /// Construct a new instance with a license in the public domain.
    pub fn new_public_domain(name: &str) -> Self {
        Self {
            name: name.to_string(),
            flavor: ComponentFlavor::Generic,
            license: LicenseFlavor::PublicDomain,
            source_location: SourceLocation::NotSet,
            license_texts: vec![],
        }
    }

    /// Construct a new instance with an unknown license.
    pub fn new_unknown(name: &str, terms: Vec<String>) -> Self {
        Self {
            name: name.to_string(),
            flavor: ComponentFlavor::Generic,
            license: LicenseFlavor::Unknown(terms),
            source_location: SourceLocation::NotSet,
            license_texts: vec![],
        }
    }

    /// Obtain the name of this software component.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The type of this component.
    pub fn flavor(&self) -> &ComponentFlavor {
        &self.flavor
    }

    /// Set the flavor of this component.
    pub fn set_flavor(&mut self, flavor: ComponentFlavor) {
        self.flavor = flavor;
    }

    /// Obtain the flavor of license for this component.
    pub fn license(&self) -> &LicenseFlavor {
        &self.license
    }

    /// Obtain the SPDX expression for this component's license.
    pub fn spdx_expression(&self) -> Option<&Expression> {
        match &self.license {
            LicenseFlavor::Spdx(expression) => Some(expression),
            LicenseFlavor::OtherExpression(expression) => Some(expression),
            LicenseFlavor::None | LicenseFlavor::PublicDomain | LicenseFlavor::Unknown(_) => None,
        }
    }

    /// Whether the SPDX expression is simple.
    ///
    /// Simple is defined as having at most a single license.
    pub fn is_simple_spdx_expression(&self) -> bool {
        if let LicenseFlavor::Spdx(expression) = &self.license {
            expression.iter().count() < 2
        } else {
            false
        }
    }

    /// Obtain the location where the source of this component can be obtained.
    pub fn source_location(&self) -> &SourceLocation {
        &self.source_location
    }

    /// Define where source code for this component can be obtained from.
    pub fn set_source_location(&mut self, location: SourceLocation) {
        self.source_location = location;
    }

    /// Obtain the explicitly set license texts for this component.
    pub fn license_texts(&self) -> &Vec<String> {
        &self.license_texts
    }

    /// Define the license text for this component.
    pub fn add_license_text(&mut self, text: impl ToString) {
        self.license_texts.push(text.to_string());
    }

    /// Returns whether all license identifiers are SPDX.
    pub fn is_spdx(&self) -> bool {
        matches!(self.license, LicenseFlavor::Spdx(_))
    }

    /// Obtain all SPDX licenses referenced by this component.
    ///
    /// The first element of the returned tuple is the license identifier. The 2nd
    /// is an optional exclusion identifier.
    pub fn all_spdx_licenses(&self) -> BTreeSet<(LicenseId, Option<ExceptionId>)> {
        match &self.license {
            LicenseFlavor::Spdx(expression) => expression
                .requirements()
                .map(|req| (req.req.license.id().clone().unwrap(), req.req.exception))
                .collect::<BTreeSet<_>>(),
            LicenseFlavor::OtherExpression(expression) => expression
                .requirements()
                .filter_map(|req| {
                    if let Some(id) = req.req.license.id() {
                        Some((id, req.req.exception))
                    } else {
                        None
                    }
                })
                .collect::<BTreeSet<_>>(),
            LicenseFlavor::None | LicenseFlavor::PublicDomain | LicenseFlavor::Unknown(_) => {
                BTreeSet::new()
            }
        }
    }
}

/// A collection of licensed components.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LicensedComponents {
    /// The collection of components, indexed by name.
    components: BTreeMap<(String, ComponentFlavor), LicensedComponent>,
}

impl LicensedComponents {
    /// Iterate over components in this collection.
    pub fn iter_components(&self) -> impl Iterator<Item = &LicensedComponent> {
        self.components.values()
    }

    /// Add a component to this collection.
    pub fn add_component(&mut self, component: LicensedComponent) {
        self.components.insert(
            (component.name.clone(), component.flavor.clone()),
            component,
        );
    }

    /// Add a component to this collection, but only if it only contains SPDX license identifiers.
    pub fn add_spdx_only_component(&mut self, component: LicensedComponent) -> Result<()> {
        if component.is_spdx() {
            self.add_component(component);
            Ok(())
        } else {
            Err(anyhow!("component has non-SPDX license identifiers"))
        }
    }

    /// Obtain all SPDX license identifiers referenced by registered components.
    pub fn all_spdx_licenses(&self) -> BTreeSet<(LicenseId, Option<ExceptionId>)> {
        self.components
            .values()
            .map(|component| component.all_spdx_licenses())
            .flatten()
            .collect::<BTreeSet<_>>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_advanced() -> Result<()> {
        LicensedComponent::new_spdx("foo", "Apache-2.0 OR MPL-2.0 OR 0BSD")?;
        LicensedComponent::new_spdx("foo", "Apache-2.0 AND MPL-2.0 AND 0BSD")?;
        LicensedComponent::new_spdx("foo", "Apache-2.0 AND MPL-2.0 OR 0BSD")?;
        LicensedComponent::new_spdx("foo", "MIT AND (LGPL-2.1-or-later OR BSD-3-Clause)")?;

        Ok(())
    }
}
