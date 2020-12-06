// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    anyhow::{anyhow, Result},
    spdx::{ExceptionId, Expression, LicenseId, LicenseReq},
    std::{
        collections::{BTreeMap, BTreeSet},
        fmt::Write,
        io::Read,
    },
    url::Url,
};

const LICENSE_TEXT_URL: &str =
    "https://raw.githubusercontent.com/spdx/license-list-data/master/text/{}.txt";

pub const DEFAULT_LICENSE_PREAMBLE: &str =
    "This product contains software subject to licenses as described below.";

/// Obtain the SPDX license text for a given license ID.
pub fn get_spdx_license_text(
    client: &reqwest::blocking::Client,
    license_id: &str,
) -> Result<String> {
    let url = Url::parse(&LICENSE_TEXT_URL.replace("{}", license_id))?;

    let mut response = client.get(url.clone()).send()?;
    if response.status() != 200 {
        return Err(anyhow!("HTTP {} from {}", response.status(), url));
    }
    let mut license_text = String::new();
    response.read_to_string(&mut license_text)?;

    Ok(license_text)
}

/// Resolve an `spdx::LicenseReq` to license text.
///
/// Only works for valid SPDX licenses. If an exception is present, that exception
/// text will be concatenated to the original license's text.
pub fn license_requirement_to_license_text(
    client: &reqwest::blocking::Client,
    req: &LicenseReq,
) -> Result<String> {
    if let Some(id) = &req.license.id() {
        let mut texts = vec![get_spdx_license_text(client, id.name)?];

        if let Some(exception) = &req.exception {
            texts.push(get_spdx_license_text(client, exception.name)?);
        }

        Ok(texts.join("\n"))
    } else {
        Err(anyhow!("license requirement must have valid SPDX license"))
    }
}

/// Describes the type of a software component.
#[derive(Clone, Debug, PartialEq)]
pub enum ComponentType {
    /// No specific component type.
    Generic,
    /// A generic software library.
    Library,
    /// A Rust crate.
    RustCrate,
    /// A Python package.
    PythonPackage,
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
    flavor: ComponentType,

    /// An SPDX license expression describing the license of this component.
    spdx_expression: Expression,

    /// Location where source code for this component can be obtained.
    source_location: SourceLocation,

    /// Specified license text for this component.
    ///
    /// If not defined, it will be derived from the SPDX expression.
    license_text: Option<String>,
}

impl LicensedComponent {
    /// Construct a new instance from an SPDX expression.
    pub fn new<'a>(name: &str, spdx_expression: &'a str) -> Result<Self, spdx::ParseError<'a>> {
        let spdx_expression = Expression::parse(spdx_expression)?;

        Ok(Self {
            name: name.to_string(),
            flavor: ComponentType::Generic,
            spdx_expression,
            source_location: SourceLocation::NotSet,
            license_text: None,
        })
    }

    /// Obtain the name of this software component.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The type of this component.
    pub fn flavor(&self) -> &ComponentType {
        &self.flavor
    }

    /// Set the flavor of this component.
    pub fn set_flavor(&mut self, flavor: ComponentType) {
        self.flavor = flavor;
    }

    /// Obtain the parsed SPDX expression describing the license of this component.
    pub fn spdx_expression(&self) -> &Expression {
        &self.spdx_expression
    }

    /// Whether the SPDX expression is simple.
    ///
    /// Simple is defined as having at most a single license.
    pub fn is_simple_spdx_expression(&self) -> bool {
        self.spdx_expression.iter().count() < 2
    }

    /// Define where source code for this component can be obtained from.
    pub fn set_source_location(&mut self, location: SourceLocation) {
        self.source_location = location;
    }

    /// Obtain the explicitly set license text for this component.
    pub fn license_text(&self) -> &Option<String> {
        &self.license_text
    }

    /// Define the license text for this component.
    pub fn set_license_text(&mut self, text: impl ToString) {
        self.license_text = Some(text.to_string())
    }

    /// Returns whether all license identifiers are SPDX.
    pub fn is_spdx(&self) -> bool {
        self.spdx_expression
            .requirements()
            .all(|req| req.req.license.id().is_some())
    }

    /// Obtain all SPDX licenses referenced by this component.
    ///
    /// The first element of the returned tuple is the license identifier. The 2nd
    /// is an optional exclusion identifier.
    pub fn all_spdx_licenses(&self) -> BTreeSet<(LicenseId, Option<ExceptionId>)> {
        self.spdx_expression
            .requirements()
            .filter_map(|req| {
                if let Some(id) = req.req.license.id() {
                    Some((id, req.req.exception))
                } else {
                    None
                }
            })
            .collect::<BTreeSet<_>>()
    }

    /// Returns whether the license is considered free by the Free Software Foundation.
    ///
    /// This takes conditional expression into account. If using `OR` and 1 license
    /// meets the requirements, this returns true.
    pub fn is_fsf_free_libre(&self) -> bool {
        self.spdx_expression.evaluate(|req| {
            if let Some(id) = req.license.id() {
                id.is_fsf_free_libre()
            } else {
                false
            }
        })
    }

    /// Returns whether the license is approved by the Open Source Initiative.
    ///
    /// This takes conditional expression into account. If using `OR` and 1 license
    /// meets the requirements, this returns true.
    pub fn is_osi_approved(&self) -> bool {
        self.spdx_expression.evaluate(|req| {
            if let Some(id) = req.license.id() {
                id.is_osi_approved()
            } else {
                false
            }
        })
    }

    /// Returns whether the license is considered copyleft.
    ///
    /// This takes conditional expression into account. If using `OR` and 1 license
    /// meets the requirements, this returns true.
    pub fn is_copyleft(&self) -> bool {
        self.spdx_expression.evaluate(|req| {
            if let Some(id) = req.license.id() {
                id.is_copyleft()
            } else {
                false
            }
        })
    }

    /// Obtain texts of SPDX licenses which apply to this component.
    ///
    /// If non-SPDX license identifiers are present, they are ignored. Consider
    /// calling `is_spdx()` to ensure only SPDX license identifiers are used.
    pub fn spdx_license_texts(&self, client: &reqwest::blocking::Client) -> Result<Vec<String>> {
        self.spdx_expression
            .requirements()
            .filter(|req| req.req.license.id().is_some())
            .map(|req| license_requirement_to_license_text(client, &req.req))
            .collect::<Result<Vec<_>>>()
    }
}

/// A collection of licensed components.
#[derive(Clone, Debug, Default)]
pub struct LicensedComponents {
    /// The collection of components, indexed by name.
    components: BTreeMap<String, LicensedComponent>,
}

impl LicensedComponents {
    /// Add a component to this collection.
    pub fn add_component(&mut self, component: LicensedComponent) {
        self.components.insert(component.name.clone(), component);
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

    /// Generates text content describing the licensing of all components.
    ///
    /// `preamble` is introductory text that will be printed before the automatically
    /// generated text.
    pub fn generate_aggregate_license_text(
        &self,
        client: &reqwest::blocking::Client,
        preamble: &str,
    ) -> Result<String> {
        let mut text = preamble.to_string();
        writeln!(&mut text)?;
        writeln!(&mut text)?;

        for component in self.components.values() {
            let title = format!("{} License", component.name);
            writeln!(&mut text, "{}", title)?;
            writeln!(&mut text, "{}", "=".repeat(title.len()))?;
            writeln!(&mut text)?;
            writeln!(
                &mut text,
                "This product contains the {} {}.",
                component.name,
                match component.flavor {
                    ComponentType::Generic => "software",
                    ComponentType::Library => "library",
                    ComponentType::RustCrate => "Rust crate",
                    ComponentType::PythonPackage => "Python package",
                }
            )?;
            writeln!(&mut text)?;
            match &component.source_location {
                SourceLocation::NotSet => {}
                SourceLocation::Url(url) => {
                    writeln!(
                        &mut text,
                        "The source code for {} can be found at\n{}",
                        component.name, url
                    )?;
                    writeln!(&mut text)?;
                }
            }
            writeln!(&mut text, "The SPDX license expression of this component is\n\"{}\". Its license text is as follows:", component.spdx_expression)?;
            writeln!(&mut text)?;
            if let Some(explicit) = &component.license_text {
                writeln!(&mut text, "{}", explicit)?;
            } else {
                writeln!(
                    &mut text,
                    "{}",
                    component.spdx_license_texts(&client)?.join("\n")
                )?;
            }
        }

        Ok(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_advanced() -> Result<()> {
        LicensedComponent::new("foo", "Apache-2.0 OR MPL-2.0 OR 0BSD")?;
        LicensedComponent::new("foo", "Apache-2.0 AND MPL-2.0 AND 0BSD")?;
        LicensedComponent::new("foo", "Apache-2.0 AND MPL-2.0 OR 0BSD")?;
        LicensedComponent::new("foo", "MIT AND (LGPL-2.1-or-later OR BSD-3-Clause)")?;

        Ok(())
    }

    #[test]
    fn spdx_license_texts() -> Result<()> {
        let client = tugger_common::http::get_http_client()?;

        let c = LicensedComponent::new("foo", "Apache-2.0")?;
        assert_eq!(c.spdx_license_texts(&client)?.len(), 1);

        let c = LicensedComponent::new("foo", "Apache-2.0 OR MPL-2.0")?;
        assert_eq!(c.spdx_license_texts(&client)?.len(), 2);

        let c = LicensedComponent::new("foo", "Apache-2.0 AND MPL-2.0")?;
        assert_eq!(c.spdx_license_texts(&client)?.len(), 2);

        let c = LicensedComponent::new("foo", "Apache-2.0 WITH LLVM-exception")?;
        assert_eq!(c.spdx_license_texts(&client)?.len(), 1);

        Ok(())
    }

    #[test]
    fn generate_aggregate_license_text() -> Result<()> {
        let client = tugger_common::http::get_http_client()?;

        let mut c = LicensedComponents::default();
        c.add_spdx_only_component(LicensedComponent::new("foo", "Apache-2.0")?)?;
        c.add_spdx_only_component(LicensedComponent::new("bar", "MIT")?)?;

        c.generate_aggregate_license_text(&client, DEFAULT_LICENSE_PREAMBLE)?;

        Ok(())
    }
}
