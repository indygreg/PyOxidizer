// Copyright 2022 Gregory Szorc.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use {
    crate::{package_metadata::PythonPackageMetadata, resource::PythonResource},
    anyhow::{anyhow, Context, Result},
    spdx::{ExceptionId, Expression, LicenseId},
    std::{
        cmp::Ordering,
        collections::{BTreeMap, BTreeSet},
        fmt::{Display, Formatter},
    },
};

/// System libraries that are safe to link against, ignoring copyleft license implications.
pub const SAFE_SYSTEM_LIBRARIES: &[&str] = &[
    "cabinet", "iphlpapi", "msi", "rpcrt4", "rt", "winmm", "ws2_32",
];

fn format_spdx(id: LicenseId, exception: Option<ExceptionId>, full: bool) -> String {
    let name = if full { id.full_name } else { id.name };

    if let Some(exception) = exception {
        format!("{} WITH {}", name, exception.name)
    } else {
        name.to_string()
    }
}

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
#[derive(Clone, Debug)]
pub enum ComponentFlavor {
    /// A Python distribution.
    PythonDistribution(String),
    /// A Python module in the standard library.
    PythonStandardLibraryModule(String),
    /// A compiled Python extension module in the standard library.
    PythonStandardLibraryExtensionModule(String),
    /// A compiled Python extension module.
    PythonExtensionModule(String),
    /// A Python module.
    PythonModule(String),
    /// A generic software library.
    Library(String),
    /// A Rust crate.
    RustCrate(String),
}

impl Display for ComponentFlavor {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PythonDistribution(name) => f.write_str(name),
            Self::PythonStandardLibraryModule(name) => {
                f.write_fmt(format_args!("Python stdlib module {}", name))
            }
            Self::PythonStandardLibraryExtensionModule(name) => {
                f.write_fmt(format_args!("Python stdlib extension {}", name))
            }
            Self::PythonExtensionModule(name) => {
                f.write_fmt(format_args!("Python extension module {}", name))
            }
            Self::PythonModule(name) => f.write_fmt(format_args!("Python module {}", name)),
            Self::Library(name) => f.write_fmt(format_args!("library {}", name)),
            Self::RustCrate(name) => f.write_fmt(format_args!("Rust crate {}", name)),
        }
    }
}

impl PartialEq for ComponentFlavor {
    fn eq(&self, other: &Self) -> bool {
        // If both entities have a Python module name, equivalence is whether
        // the module names agree, as there can only be a single entity for a given
        // module name.
        match (self.python_module_name(), other.python_module_name()) {
            (Some(a), Some(b)) => a.eq(b),
            // Comparing a module with a non-module is always not equivalent.
            (Some(_), None) => false,
            (None, Some(_)) => false,
            (None, None) => match (self, other) {
                (Self::PythonDistribution(a), Self::PythonDistribution(b)) => a.eq(b),
                (Self::Library(a), Self::Library(b)) => a.eq(b),
                (Self::RustCrate(a), Self::RustCrate(b)) => a.eq(b),
                _ => false,
            },
        }
    }
}

impl Eq for ComponentFlavor {}

impl PartialOrd for ComponentFlavor {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        match (self.python_module_name(), other.python_module_name()) {
            (Some(a), Some(b)) => a.partial_cmp(b),
            _ => {
                let a = (self.ordinal_value(), self.to_string());
                let b = (other.ordinal_value(), other.to_string());

                a.partial_cmp(&b)
            }
        }
    }
}

impl Ord for ComponentFlavor {
    fn cmp(&self, other: &Self) -> Ordering {
        self.partial_cmp(other).unwrap()
    }
}

impl ComponentFlavor {
    fn ordinal_value(&self) -> u8 {
        match self {
            Self::PythonDistribution(_) => 0,
            ComponentFlavor::PythonStandardLibraryModule(_) => 1,
            ComponentFlavor::PythonStandardLibraryExtensionModule(_) => 2,
            ComponentFlavor::PythonExtensionModule(_) => 3,
            ComponentFlavor::PythonModule(_) => 4,
            ComponentFlavor::Library(_) => 5,
            ComponentFlavor::RustCrate(_) => 6,
        }
    }

    /// Whether this component is part of the Python standard library.
    pub fn is_python_standard_library(&self) -> bool {
        match self {
            Self::PythonDistribution(_) => false,
            Self::PythonStandardLibraryModule(_) => true,
            Self::PythonStandardLibraryExtensionModule(_) => true,
            Self::PythonExtensionModule(_) => true,
            Self::PythonModule(_) => false,
            Self::Library(_) => false,
            Self::RustCrate(_) => false,
        }
    }

    pub fn python_module_name(&self) -> Option<&str> {
        match self {
            Self::PythonDistribution(_) => None,
            Self::PythonStandardLibraryModule(name) => Some(name.as_str()),
            Self::PythonStandardLibraryExtensionModule(name) => Some(name.as_str()),
            Self::PythonExtensionModule(name) => Some(name.as_str()),
            Self::PythonModule(name) => Some(name.as_str()),
            Self::Library(_) => None,
            Self::RustCrate(_) => None,
        }
    }

    /// Whether the component is part of a Python distribution.
    pub fn is_python_distribution_component(&self) -> bool {
        matches!(
            self,
            Self::PythonDistribution(_)
                | Self::PythonStandardLibraryModule(_)
                | Self::PythonStandardLibraryExtensionModule(_)
        )
    }
}

/// Where source code for a component can be obtained from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourceLocation {
    /// Source code is not available.
    NotSet,
    /// Source code is available at a URL.
    Url(String),
}

/// Represents a software component with licensing information.
#[derive(Clone, Debug)]
pub struct LicensedComponent {
    /// Type of component.
    flavor: ComponentFlavor,

    /// The type of license.
    license: LicenseFlavor,

    /// Location where source code for this component can be obtained.
    source_location: SourceLocation,

    /// Homepage for project.
    homepage: Option<String>,

    /// List of authors.
    authors: Vec<String>,

    /// Specified license text for this component.
    ///
    /// If empty, license texts will be derived from SPDX identifiers, if available.
    license_texts: Vec<String>,
}

impl PartialEq for LicensedComponent {
    fn eq(&self, other: &Self) -> bool {
        self.flavor.eq(&other.flavor)
    }
}

impl Eq for LicensedComponent {}

impl PartialOrd for LicensedComponent {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.flavor.partial_cmp(&other.flavor)
    }
}

impl Ord for LicensedComponent {
    fn cmp(&self, other: &Self) -> Ordering {
        self.flavor.cmp(&other.flavor)
    }
}

impl LicensedComponent {
    /// Construct a new instance from parameters.
    pub fn new(flavor: ComponentFlavor, license: LicenseFlavor) -> Self {
        Self {
            flavor,
            license,
            source_location: SourceLocation::NotSet,
            homepage: None,
            authors: vec![],
            license_texts: vec![],
        }
    }

    /// Construct a new instance from an SPDX expression.
    pub fn new_spdx(flavor: ComponentFlavor, spdx_expression: &str) -> Result<Self> {
        let spdx_expression = Expression::parse(spdx_expression).map_err(|e| anyhow!("{}", e))?;

        let license = if spdx_expression.evaluate(|req| req.license.id().is_some()) {
            LicenseFlavor::Spdx(spdx_expression)
        } else {
            LicenseFlavor::OtherExpression(spdx_expression)
        };

        Ok(Self::new(flavor, license))
    }

    /// The type of this component.
    pub fn flavor(&self) -> &ComponentFlavor {
        &self.flavor
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

    /// Obtain the homepage / URL of this component.
    pub fn homepage(&self) -> Option<&str> {
        self.homepage.as_deref()
    }

    /// Set the homepage of this component.
    pub fn set_homepage(&mut self, value: impl ToString) {
        self.homepage = Some(value.to_string());
    }

    /// Obtain the annotated authors of this component.
    pub fn authors(&self) -> &[String] {
        &self.authors
    }

    /// Define an author of this component.
    pub fn add_author(&mut self, value: impl ToString) {
        self.authors.push(value.to_string());
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
                .map(|req| (req.req.license.id().unwrap(), req.req.exception))
                .collect::<BTreeSet<_>>(),
            LicenseFlavor::OtherExpression(expression) => expression
                .requirements()
                .filter_map(|req| req.req.license.id().map(|id| (id, req.req.exception)))
                .collect::<BTreeSet<_>>(),
            LicenseFlavor::None | LicenseFlavor::PublicDomain | LicenseFlavor::Unknown(_) => {
                BTreeSet::new()
            }
        }
    }

    /// Obtain all SPDX license names.
    pub fn all_spdx_license_names(&self, full: bool) -> Vec<String> {
        self.all_spdx_licenses()
            .into_iter()
            .map(|(id, exception)| format_spdx(id, exception, full))
            .collect::<Vec<_>>()
    }

    /// Obtain all the distinct [LicenseId] in this component.
    ///
    /// Unlike [Self::all_spdx_licenses()], this returns just the license IDs without exceptions.
    pub fn all_spdx_license_ids(&self) -> BTreeSet<LicenseId> {
        self.all_spdx_licenses()
            .into_iter()
            .map(|(lid, _)| lid)
            .collect::<BTreeSet<_>>()
    }

    /// Obtain all the [ExceptionId] present in this component.
    pub fn all_spdx_exception_ids(&self) -> BTreeSet<ExceptionId> {
        self.all_spdx_licenses()
            .into_iter()
            .filter_map(|(_, id)| id)
            .collect::<BTreeSet<_>>()
    }

    /// Whether the component has any copyleft licenses.
    pub fn has_copyleft(&self) -> bool {
        self.all_spdx_licenses()
            .into_iter()
            .any(|(id, _)| id.is_copyleft())
    }

    /// Whether all licenses are copyleft.
    pub fn is_always_copyleft(&self) -> bool {
        let licenses = self.all_spdx_licenses();

        if licenses.is_empty() {
            false
        } else {
            licenses.into_iter().all(|(id, _)| id.is_copyleft())
        }
    }

    /// Obtain a textual licensing summary of this component.
    pub fn licensing_summary(&self) -> String {
        let mut lines = vec![];

        if !self.authors().is_empty() {
            lines.push(format!("Authors: {}", self.authors().join(", ")));
        }
        if let Some(value) = self.homepage() {
            lines.push(format!("Homepage: {}", value));
        }
        match self.source_location() {
            SourceLocation::NotSet => {}
            SourceLocation::Url(value) => {
                lines.push(format!("Source location: {}", value));
            }
        }

        match self.license() {
            LicenseFlavor::None => {
                lines.push("No licensing information available.".into());
            }
            LicenseFlavor::Spdx(expression) | LicenseFlavor::OtherExpression(expression) => {
                lines.push(format!(
                    "Licensed according to SPDX expression: {}",
                    expression
                ));
            }
            LicenseFlavor::PublicDomain => {
                lines.push("Licensed to the public domain.".into());
            }
            LicenseFlavor::Unknown(terms) => {
                lines.push(format!("Licensed according to {}", terms.join(", ")));
            }
        }

        lines.join("\n")
    }
}

/// A collection of licensed components.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LicensedComponents {
    /// The collection of components, indexed by its flavor.
    components: BTreeMap<ComponentFlavor, LicensedComponent>,
}

impl LicensedComponents {
    /// Obtain an iterator over the components, consuming self.
    pub fn into_components(self) -> impl Iterator<Item = LicensedComponent> {
        self.components.into_values()
    }

    /// Iterate over components in this collection.
    pub fn iter_components(&self) -> impl Iterator<Item = &LicensedComponent> {
        self.components.values()
    }

    /// Add a component to this collection.
    pub fn add_component(&mut self, component: LicensedComponent) {
        self.components.insert(component.flavor.clone(), component);
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

    /// Whether a Python module exists in the collection.
    pub fn has_python_module(&self, name: &str) -> bool {
        // ComponentFlavor are equivalent if the Python module name is the same,
        // even if the enum variant is different.
        self.components
            .contains_key(&ComponentFlavor::PythonModule(name.into()))
    }

    /// Adjusts Python modules in the components set.
    ///
    /// Standard library modules that have identical licensing to the Python
    /// distribution are removed.
    ///
    /// Missing top-level packages are added with an unknown license annotation.
    ///
    /// Modules that aren't top-level modules are removed.
    pub fn normalize_python_modules(&self) -> Self {
        let distribution = self
            .components
            .values()
            .find(|c| matches!(c.flavor(), ComponentFlavor::PythonDistribution(_)));

        let mut top_level_names = BTreeSet::new();
        let mut components = Self::default();

        let filtered = self.components.iter().filter(|(k, v)| {
            // Remove standard library modules with licensing identical to the distribution.
            if k.is_python_standard_library() {
                if let Some(distribution) = distribution {
                    if v.license() == distribution.license() {
                        return false;
                    }
                }
            }

            if let Some(name) = k.python_module_name() {
                let top_level_name = if let Some((name, _)) = name.split_once('.') {
                    name
                } else {
                    name
                };

                top_level_names.insert(top_level_name.to_string());
            }

            true
        });

        for (_, component) in filtered {
            components.add_component(component.clone());
        }

        // Ensure top-level modules are present.
        for name in top_level_names {
            if !components.has_python_module(&name) {
                components.add_component(LicensedComponent::new(
                    ComponentFlavor::PythonModule(name.to_string()),
                    LicenseFlavor::None,
                ));
            }
        }

        // Filter non top-levels from the list.
        components.components =
            BTreeMap::from_iter(components.components.into_iter().filter(|(k, _)| {
                if let Some(name) = k.python_module_name() {
                    if name.contains('.') {
                        return false;
                    }
                }

                true
            }));

        components
    }

    /// Obtain all SPDX license identifiers referenced by registered components.
    pub fn all_spdx_licenses(&self) -> BTreeSet<(LicenseId, Option<ExceptionId>)> {
        self.components
            .values()
            .flat_map(|component| component.all_spdx_licenses())
            .collect::<BTreeSet<_>>()
    }

    /// Obtain all SPDX license IDs referenced by all components.
    ///
    /// Unlike [Self::all_spdx_licenses()], this returns just the [LicenseId], without exceptions.
    pub fn all_spdx_license_ids(&self) -> BTreeSet<LicenseId> {
        self.components
            .values()
            .flat_map(|component| component.all_spdx_license_ids())
            .collect::<BTreeSet<_>>()
    }

    /// Obtain all SPDX license names referenced by registered components.
    pub fn all_spdx_license_names(&self, full: bool) -> Vec<String> {
        self.iter_components()
            .flat_map(|c| c.all_spdx_license_names(full))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>()
    }

    /// Obtain a mapping of all SPDX licenses to components having them.
    pub fn components_by_spdx_license(
        &self,
    ) -> BTreeMap<(LicenseId, Option<ExceptionId>), BTreeSet<&LicensedComponent>> {
        let mut res = BTreeMap::new();

        for component in self.iter_components() {
            for key in component.all_spdx_licenses() {
                res.entry(key)
                    .or_insert_with(BTreeSet::new)
                    .insert(component);
            }
        }

        res
    }

    /// Obtain all components with valid SPDX license expressions.
    pub fn license_spdx_components(&self) -> impl Iterator<Item = &LicensedComponent> {
        self.components
            .values()
            .filter(|c| matches!(c.license(), &LicenseFlavor::Spdx(_)))
    }

    /// Obtain components that are missing license annotations.
    pub fn license_missing_components(&self) -> impl Iterator<Item = &LicensedComponent> {
        self.components
            .values()
            .filter(|c| c.license() == &LicenseFlavor::None)
    }

    /// Obtain components that are licensed to the public domain.
    pub fn license_public_domain_components(&self) -> impl Iterator<Item = &LicensedComponent> {
        self.components
            .values()
            .filter(|c| c.license() == &LicenseFlavor::PublicDomain)
    }

    /// Obtain components that have unknown licensing.
    ///
    /// There is a value for the license but that license is not recognized by us.
    pub fn license_unknown_components(&self) -> impl Iterator<Item = &LicensedComponent> {
        self.components.values().filter(|c| {
            matches!(
                c.license(),
                &LicenseFlavor::Unknown(_) | &LicenseFlavor::OtherExpression(_)
            )
        })
    }

    /// Components that have copyleft licenses.
    ///
    /// There may be false negatives if the component doesn't have fully SPDX parsed
    /// licenses.
    pub fn license_copyleft_components(&self) -> impl Iterator<Item = &LicensedComponent> {
        self.components.values().filter(|c| c.has_copyleft())
    }

    /// Generate a text summary of licesning info.
    pub fn license_summary(&self) -> String {
        let mut lines = vec![
            "Software Licensing Summary".to_string(),
            "==========================".to_string(),
            "".to_string(),
        ];

        lines.push(format!(
            "{} distinct software components",
            self.components.len()
        ));
        lines.push(format!(
            "{} lack a known software license",
            self.license_missing_components().count()
        ));
        lines.push(format!(
            "{} have unknown license expressions",
            self.license_unknown_components().count()
        ));
        lines.push(format!(
            "{} distinct SPDX licenses",
            self.all_spdx_licenses().len()
        ));
        lines.push(format!(
            "{} components in the public domain",
            self.license_public_domain_components().count()
        ));
        lines.push(format!(
            "{} have copyleft licenses",
            self.license_copyleft_components().count()
        ));
        let spdx_components = self.components_by_spdx_license();
        if !spdx_components.is_empty() {
            lines.push("".to_string());

            lines.push("Count   OSI   FSF free   Copyleft   SPDX License".to_string());

            for ((lid, exception), components) in spdx_components {
                lines.push(format!(
                    "{:>5}   [{}]     [{}]        [{}]      {}",
                    components.len(),
                    if lid.is_osi_approved() { "x" } else { " " },
                    if lid.is_fsf_free_libre() { "x" } else { " " },
                    if lid.is_copyleft() { "x" } else { " " },
                    format_spdx(lid, exception, true)
                ));
            }
        }

        lines.join("\n")
    }

    /// Generate a text report of noteworthy licensing info.
    ///
    /// This essentially emits license quirks that may warrant user attention.
    pub fn interesting_report(&self) -> Option<String> {
        let mut lines = vec![
            "Noteworthy Licensing Info".to_string(),
            "=========================".to_string(),
            "".to_string(),
        ];

        let mut have_interesting = false;

        for component in self.iter_components() {
            match component.license() {
                LicenseFlavor::None => {
                    lines.push(format!("* {} lacks a known license", component.flavor()));
                    have_interesting = true;
                }
                LicenseFlavor::Spdx(_) => {
                    let copyleft_names = component
                        .all_spdx_licenses()
                        .into_iter()
                        .filter(|(id, _)| id.is_copyleft())
                        .map(|(id, exception)| format_spdx(id, exception, true))
                        .collect::<Vec<_>>();

                    if component.is_always_copyleft() {
                        lines.push(format!(
                            "* {} has copyleft licenses exclusively ({})",
                            component.flavor(),
                            copyleft_names.join(", ")
                        ));
                        have_interesting = true;
                    } else if component.has_copyleft() {
                        lines.push(format!(
                            "* {} has a copyleft license ({})",
                            component.flavor(),
                            copyleft_names.join(", ")
                        ));
                        have_interesting = true;
                    }
                }
                LicenseFlavor::OtherExpression(expr) => {
                    lines.push(format!(
                        "* {} has an unknown SPDX license expression: {}",
                        component.flavor(),
                        expr
                    ));
                    have_interesting = true;
                }
                LicenseFlavor::PublicDomain => {}
                LicenseFlavor::Unknown(terms) => {
                    lines.push(format!(
                        "* {} has unknown license expression: {}",
                        component.flavor(),
                        terms.join(", ")
                    ));
                    have_interesting = true;
                }
            }
        }

        if have_interesting {
            Some(lines.join("\n"))
        } else {
            None
        }
    }

    /// Generate a summary of SPDX licenses in all components.
    pub fn spdx_license_breakdown(&self) -> String {
        let mut lines = vec![
            "SPDX License Breakdown".to_string(),
            "======================".to_string(),
            "".to_string(),
        ];

        for (license, exception) in self.all_spdx_licenses() {
            lines.push(format_spdx(license, exception, true));
            lines.push("-".repeat(format_spdx(license, exception, true).len()));
            lines.push("".to_string());

            lines.push(format!(
                "[{}] OSI approved; [{}] FSF free libre; [{}] copyleft",
                if license.is_osi_approved() { "*" } else { " " },
                if license.is_fsf_free_libre() {
                    "*"
                } else {
                    " "
                },
                if license.is_copyleft() { "*" } else { " " }
            ));
            lines.push("".to_string());

            for component in self.iter_components() {
                if component
                    .all_spdx_licenses()
                    .contains(&(license, exception))
                {
                    lines.push(format!("* {}", component.flavor()));
                }
            }

            lines.push("".to_string());
        }

        lines.join("\n")
    }

    /// Generate a unified text document describing licensing info for the components within.
    #[cfg(feature = "spdx-text")]
    pub fn aggregate_license_document(&self, emit_interesting: bool) -> Result<String> {
        let mut lines = vec![self.license_summary()];
        lines.push("".into());

        if emit_interesting {
            if let Some(value) = self.interesting_report() {
                lines.push(value);
                lines.push("".into());
            }
        }

        lines.push("Software Components".to_string());
        lines.push("===================".to_string());
        lines.push("".into());

        for component in self.iter_components() {
            lines.push(component.flavor().to_string());
            lines.push("-".repeat(component.flavor().to_string().len()));
            lines.push("".into());

            lines.push(component.licensing_summary());
            lines.push("".into());

            if component.spdx_expression().is_some() && component.license_texts.is_empty() {
                lines.push("The license texts for this component are reproduced elsewhere in this document.".into());
            }

            for exception in component.all_spdx_exception_ids() {
                lines.push("".into());
                lines.push(format!(
                        "In addition to the standard SPDX license, this component has the license exception: {}",
                        exception.name
                    ));
                lines.push("The text of that exception follows.".into());
                lines.push("".into());
                lines.push(exception.text().to_string());
                lines.push(format!("(end of exception text for {})", exception.name));
            }

            if !component.license_texts().is_empty() {
                lines.push("".into());
                lines.push("The license text for this component is as follows.".into());
                lines.push("".into());
                lines.push("-".repeat(80).to_string());

                for text in component.license_texts() {
                    lines.push(text.to_string());
                }
                lines.push("".into());
                lines.push("-".repeat(80).to_string());
                lines.push(format!("(end of license text for {})", component.flavor()));
            }

            lines.push("".into());
        }

        lines.push("SPDX License Texts".into());
        lines.push("==================".into());
        lines.push("".into());
        lines.push("The following sections contain license texts for all SPDX licenses".into());
        lines.push("referenced by software components listed above.".into());
        lines.push("".into());

        for license in self.all_spdx_license_ids() {
            let header = format!("{} / {}", license.name, license.full_name);

            lines.push(header.clone());
            lines.push("-".repeat(header.len()));

            lines.push("".into());

            lines.push(license.text().to_string());

            lines.push("".into());
        }

        let text = lines.join("\n");

        Ok(text)
    }
}

/// Defines license information for a Python package.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PackageLicenseInfo {
    /// The Python package who license info is being annotated.
    pub package: String,

    /// Version string of Python package being annotated.
    pub version: String,

    /// `License` entries in package metadata.
    pub metadata_licenses: Vec<String>,

    /// Licenses present in `Classifier: License` entries in package metadata.
    pub classifier_licenses: Vec<String>,

    /// Texts of licenses present in the package.
    pub license_texts: Vec<String>,

    /// Texts of NOTICE files in the package.
    pub notice_texts: Vec<String>,

    /// Special annotation indicating if the license is in the public domain.
    pub is_public_domain: bool,

    /// URL of project home.
    pub homepage: Option<String>,

    /// List of author strings.
    pub authors: Vec<String>,
}

impl TryInto<LicensedComponent> for PackageLicenseInfo {
    type Error = anyhow::Error;

    fn try_into(self) -> Result<LicensedComponent, Self::Error> {
        let component_flavor = ComponentFlavor::PythonModule(self.package.clone());

        let mut component = if self.is_public_domain {
            LicensedComponent::new(component_flavor, LicenseFlavor::PublicDomain)
        } else if !self.metadata_licenses.is_empty() || !self.classifier_licenses.is_empty() {
            let mut spdx_license_ids = BTreeSet::new();
            let mut non_spdx_licenses = BTreeSet::new();

            for s in self
                .metadata_licenses
                .into_iter()
                .chain(self.classifier_licenses.into_iter())
            {
                if let Some(lid) = spdx::license_id(&s) {
                    spdx_license_ids.insert(format!("({})", lid.name));
                } else if spdx::Expression::parse(&s).is_ok() {
                    spdx_license_ids.insert(format!("({})", s));
                } else if let Some(name) = spdx::identifiers::LICENSES
                    .iter()
                    .find_map(|(name, full, _)| if &s == full { Some(name) } else { None })
                {
                    spdx_license_ids.insert(name.to_string());
                } else {
                    non_spdx_licenses.insert(s);
                }
            }

            if non_spdx_licenses.is_empty() {
                let expression = spdx_license_ids
                    .into_iter()
                    .collect::<Vec<_>>()
                    .join(" OR ");
                LicensedComponent::new_spdx(component_flavor, &expression)?
            } else {
                LicensedComponent::new(
                    component_flavor,
                    LicenseFlavor::Unknown(non_spdx_licenses.into_iter().collect::<Vec<_>>()),
                )
            }
        } else {
            LicensedComponent::new(component_flavor, LicenseFlavor::None)
        };

        for text in self
            .license_texts
            .into_iter()
            .chain(self.notice_texts.into_iter())
        {
            component.add_license_text(text);
        }

        if let Some(value) = self.homepage {
            component.set_homepage(value);
        }
        for value in self.authors {
            component.add_author(value);
        }

        Ok(component)
    }
}

impl PartialOrd for PackageLicenseInfo {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        if self.package == other.package {
            self.version.partial_cmp(&other.version)
        } else {
            self.package.partial_cmp(&other.package)
        }
    }
}

impl Ord for PackageLicenseInfo {
    fn cmp(&self, other: &Self) -> Ordering {
        if self.package == other.package {
            self.version.cmp(&other.version)
        } else {
            self.package.cmp(&other.package)
        }
    }
}

/// Obtain Python package license information from an iterable of Python resources.
///
/// This will look at `PythonPackageDistributionResource` entries and attempt
/// to find license information within. It looks for license info in `METADATA`
/// and `PKG-INFO` files (both the `License` key and the trove classifiers) as
/// well as well-named files.
pub fn derive_package_license_infos<'a>(
    resources: impl Iterator<Item = &'a PythonResource<'a>>,
) -> Result<Vec<PackageLicenseInfo>> {
    let mut packages = BTreeMap::new();

    let resources = resources.filter_map(|resource| {
        if let PythonResource::PackageDistributionResource(resource) = resource {
            Some(resource)
        } else {
            None
        }
    });

    for resource in resources {
        let key = (resource.package.clone(), resource.version.clone());

        let entry = packages.entry(key).or_insert(PackageLicenseInfo {
            package: resource.package.clone(),
            version: resource.version.clone(),
            ..Default::default()
        });

        // This is a special metadata file. Parse it and attempt to extract license info.
        if resource.name == "METADATA" || resource.name == "PKG-INFO" {
            let metadata = PythonPackageMetadata::from_metadata(&resource.data.resolve_content()?)
                .context("parsing package metadata")?;

            if let Some(value) = metadata.find_first_header("Home-page") {
                entry.homepage = Some(value.to_string());
            }
            for value in metadata.find_all_headers("Author") {
                entry.authors.push(value.to_string());
            }
            for value in metadata.find_all_headers("Maintainer") {
                entry.authors.push(value.to_string());
            }

            for value in metadata.find_all_headers("License") {
                entry.metadata_licenses.push(value.to_string());
            }

            for value in metadata.find_all_headers("Classifier") {
                if value.starts_with("License ") {
                    if let Some(license) = value.split(" :: ").last() {
                        // In case they forget the part after this.
                        if license != "OSI Approved" {
                            entry.classifier_licenses.push(license.to_string());
                        }
                    }
                }
            }
        }
        // This looks like a license file.
        else if resource.name.starts_with("LICENSE")
            || resource.name.starts_with("LICENSE")
            || resource.name.starts_with("COPYING")
        {
            let data = resource.data.resolve_content()?;
            let license_text = String::from_utf8_lossy(&data);

            entry.license_texts.push(license_text.to_string());
        }
        // This looks like a NOTICE file.
        else if resource.name.starts_with("NOTICE") {
            let data = resource.data.resolve_content()?;
            let notice_text = String::from_utf8_lossy(&data);

            entry.notice_texts.push(notice_text.to_string());
        }
        // Else we don't know what to do with this file. Just ignore it.
    }

    Ok(packages.into_iter().map(|(_, v)| v).collect::<Vec<_>>())
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        crate::resource::{
            PythonPackageDistributionResource, PythonPackageDistributionResourceFlavor,
        },
        simple_file_manifest::FileData,
        std::borrow::Cow,
    };

    #[test]
    fn component_flavor_equivalence() {
        assert_eq!(
            ComponentFlavor::PythonDistribution("foo".to_string()),
            ComponentFlavor::PythonDistribution("foo".to_string())
        );
        assert_ne!(
            ComponentFlavor::PythonDistribution("foo".to_string()),
            ComponentFlavor::PythonStandardLibraryModule("foo".into())
        );
        assert_eq!(
            ComponentFlavor::PythonStandardLibraryModule("foo".into()),
            ComponentFlavor::PythonStandardLibraryModule("foo".into())
        );
        assert_eq!(
            ComponentFlavor::PythonStandardLibraryModule("foo".into()),
            ComponentFlavor::PythonStandardLibraryExtensionModule("foo".into())
        );
        assert_eq!(
            ComponentFlavor::PythonStandardLibraryModule("foo".into()),
            ComponentFlavor::PythonExtensionModule("foo".into())
        );
        assert_eq!(
            ComponentFlavor::PythonStandardLibraryModule("foo".into()),
            ComponentFlavor::PythonModule("foo".into())
        );

        assert_ne!(
            ComponentFlavor::PythonStandardLibraryModule("foo".into()),
            ComponentFlavor::PythonStandardLibraryModule("bar".into())
        );
        assert_ne!(
            ComponentFlavor::PythonStandardLibraryModule("foo".into()),
            ComponentFlavor::PythonStandardLibraryExtensionModule("bar".into())
        );
        assert_ne!(
            ComponentFlavor::PythonStandardLibraryModule("foo".into()),
            ComponentFlavor::PythonExtensionModule("bar".into())
        );
        assert_ne!(
            ComponentFlavor::PythonStandardLibraryModule("foo".into()),
            ComponentFlavor::PythonModule("bar".into())
        );
    }

    #[test]
    fn parse_advanced() -> Result<()> {
        LicensedComponent::new_spdx(
            ComponentFlavor::PythonDistribution("foo".into()),
            "Apache-2.0 OR MPL-2.0 OR 0BSD",
        )?;
        LicensedComponent::new_spdx(
            ComponentFlavor::PythonDistribution("foo".into()),
            "Apache-2.0 AND MPL-2.0 AND 0BSD",
        )?;
        LicensedComponent::new_spdx(
            ComponentFlavor::PythonDistribution("foo".into()),
            "Apache-2.0 AND MPL-2.0 OR 0BSD",
        )?;
        LicensedComponent::new_spdx(
            ComponentFlavor::PythonDistribution("foo".into()),
            "MIT AND (LGPL-2.1-or-later OR BSD-3-Clause)",
        )?;

        Ok(())
    }

    #[test]
    fn test_derive_package_license_infos_empty() -> Result<()> {
        let infos = derive_package_license_infos(vec![].iter())?;
        assert!(infos.is_empty());

        Ok(())
    }

    #[test]
    fn test_derive_package_license_infos_license_file() -> Result<()> {
        let resources = vec![PythonResource::PackageDistributionResource(Cow::Owned(
            PythonPackageDistributionResource {
                location: PythonPackageDistributionResourceFlavor::DistInfo,
                package: "foo".to_string(),
                version: "1.0".to_string(),
                name: "LICENSE".to_string(),
                data: FileData::Memory(vec![42]),
            },
        ))];

        let infos = derive_package_license_infos(resources.iter())?;
        assert_eq!(infos.len(), 1);

        assert_eq!(
            infos[0],
            PackageLicenseInfo {
                package: "foo".to_string(),
                version: "1.0".to_string(),
                license_texts: vec!["*".to_string()],
                ..Default::default()
            }
        );

        Ok(())
    }

    #[test]
    fn test_derive_package_license_infos_metadata_licenses() -> Result<()> {
        let resources = vec![PythonResource::PackageDistributionResource(Cow::Owned(
            PythonPackageDistributionResource {
                location: PythonPackageDistributionResourceFlavor::DistInfo,
                package: "foo".to_string(),
                version: "1.0".to_string(),
                name: "METADATA".to_string(),
                data: FileData::Memory(
                    "Name: foo\nLicense: BSD-1-Clause\nLicense: BSD-2-Clause\n"
                        .as_bytes()
                        .to_vec(),
                ),
            },
        ))];

        let infos = derive_package_license_infos(resources.iter())?;
        assert_eq!(infos.len(), 1);

        assert_eq!(
            infos[0],
            PackageLicenseInfo {
                package: "foo".to_string(),
                version: "1.0".to_string(),
                metadata_licenses: vec!["BSD-1-Clause".to_string(), "BSD-2-Clause".to_string()],
                ..Default::default()
            }
        );

        Ok(())
    }

    #[test]
    fn test_derive_package_license_infos_metadata_classifiers() -> Result<()> {
        let resources = vec![PythonResource::PackageDistributionResource(Cow::Owned(
            PythonPackageDistributionResource {
                location: PythonPackageDistributionResourceFlavor::DistInfo,
                package: "foo".to_string(),
                version: "1.0".to_string(),
                name: "METADATA".to_string(),
                data: FileData::Memory(
                    "Name: foo\nClassifier: License :: OSI Approved\nClassifier: License :: OSI Approved :: BSD-1-Clause\n"
                        .as_bytes()
                        .to_vec(),
                ),
            },
        ))];

        let infos = derive_package_license_infos(resources.iter())?;
        assert_eq!(infos.len(), 1);

        assert_eq!(
            infos[0],
            PackageLicenseInfo {
                package: "foo".to_string(),
                version: "1.0".to_string(),
                classifier_licenses: vec!["BSD-1-Clause".to_string()],
                ..Default::default()
            }
        );

        Ok(())
    }

    #[test]
    fn license_info_to_component_empty() -> Result<()> {
        let li = PackageLicenseInfo {
            package: "foo".to_string(),
            version: "0.1".to_string(),
            ..Default::default()
        };

        let c: LicensedComponent = li.try_into()?;
        let wanted = LicensedComponent::new(
            ComponentFlavor::PythonModule("foo".to_string()),
            LicenseFlavor::None,
        );
        assert_eq!(c, wanted);

        Ok(())
    }

    #[test]
    fn license_info_to_component_single_metadata_spdx() -> Result<()> {
        let li = PackageLicenseInfo {
            package: "foo".to_string(),
            version: "0.1".to_string(),
            metadata_licenses: vec!["MIT".to_string()],
            ..Default::default()
        };

        let c: LicensedComponent = li.try_into()?;
        let wanted =
            LicensedComponent::new_spdx(ComponentFlavor::PythonModule("foo".to_string()), "MIT")?;
        assert_eq!(c, wanted);

        Ok(())
    }

    #[test]
    fn license_info_to_component_single_classifier_spdx() -> Result<()> {
        let li = PackageLicenseInfo {
            package: "foo".to_string(),
            version: "0.1".to_string(),
            classifier_licenses: vec!["Apache-2.0".to_string()],
            ..Default::default()
        };

        let c: LicensedComponent = li.try_into()?;
        let wanted = LicensedComponent::new_spdx(
            ComponentFlavor::PythonModule("foo".to_string()),
            "Apache-2.0",
        )?;
        assert_eq!(c, wanted);

        Ok(())
    }

    #[test]
    fn license_info_to_component_multiple_metadata_spdx() -> Result<()> {
        let li = PackageLicenseInfo {
            package: "foo".to_string(),
            version: "0.1".to_string(),
            metadata_licenses: vec!["MIT".to_string(), "Apache-2.0".to_string()],
            ..Default::default()
        };

        let c: LicensedComponent = li.try_into()?;
        let wanted = LicensedComponent::new_spdx(
            ComponentFlavor::PythonModule("foo".to_string()),
            "Apache-2.0 OR MIT",
        )?;
        assert_eq!(c, wanted);

        Ok(())
    }

    #[test]
    fn license_info_to_component_multiple_classifier_spdx() -> Result<()> {
        let li = PackageLicenseInfo {
            package: "foo".to_string(),
            version: "0.1".to_string(),
            classifier_licenses: vec!["Apache-2.0".to_string(), "MIT".to_string()],
            ..Default::default()
        };

        let c: LicensedComponent = li.try_into()?;
        let wanted = LicensedComponent::new_spdx(
            ComponentFlavor::PythonModule("foo".to_string()),
            "Apache-2.0 OR MIT",
        )?;
        assert_eq!(c, wanted);

        Ok(())
    }

    #[test]
    fn license_info_to_component_spdx_expression() -> Result<()> {
        let li = PackageLicenseInfo {
            package: "foo".to_string(),
            version: "0.1".to_string(),
            metadata_licenses: vec!["MIT OR Apache-2.0".to_string()],
            ..Default::default()
        };

        let c: LicensedComponent = li.try_into()?;
        let wanted = LicensedComponent::new_spdx(
            ComponentFlavor::PythonModule("foo".to_string()),
            "MIT OR Apache-2.0",
        )?;
        assert_eq!(c, wanted);

        Ok(())
    }

    #[test]
    fn license_info_to_component_spdx_fullname() -> Result<()> {
        let li = PackageLicenseInfo {
            package: "foo".to_string(),
            version: "0.1".to_string(),
            metadata_licenses: vec!["MIT License".to_string()],
            ..Default::default()
        };

        let c: LicensedComponent = li.try_into()?;
        let wanted =
            LicensedComponent::new_spdx(ComponentFlavor::PythonModule("foo".to_string()), "MIT")?;
        assert_eq!(c, wanted);

        Ok(())
    }

    #[test]
    fn license_info_to_component_unknown() -> Result<()> {
        let terms = vec!["Unknown".to_string(), "Unknown 2".to_string()];

        let li = PackageLicenseInfo {
            package: "foo".to_string(),
            version: "0.1".to_string(),
            metadata_licenses: terms.clone(),
            ..Default::default()
        };

        let c: LicensedComponent = li.try_into()?;
        let wanted = LicensedComponent::new(
            ComponentFlavor::PythonModule("foo".to_string()),
            LicenseFlavor::Unknown(terms),
        );
        assert_eq!(c, wanted);

        Ok(())
    }
}
