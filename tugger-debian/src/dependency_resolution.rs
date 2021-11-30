// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Package dependency resolution. */

use {
    crate::{
        binary_package_control::{BinaryPackageControlError, BinaryPackageControlFile},
        dependency::{
            BinaryDependency, DependencyVersionConstraint, PackageDependencyFields,
            SingleDependency,
        },
        package_version::PackageVersion,
    },
    std::collections::HashMap,
    thiserror::Error,
};

#[derive(Debug, Error)]
pub enum DependencyResolutionError {
    #[error("binary package control file error: {0:?}")]
    BinaryPackageControl(#[from] BinaryPackageControlError),
}

pub type Result<T> = std::result::Result<T, DependencyResolutionError>;

/// Holds [BinaryPackageControlFile] references satisfying a single dependency expression.
#[derive(Clone, Debug)]
pub struct BinaryPackageSingleDependencyResolution<'file, 'data: 'file> {
    pub expression: SingleDependency,
    pub candidates: Vec<&'file BinaryPackageControlFile<'data>>,
}

/// A collection of [BinaryPackageSingleDependencyResolution] satisfying a set of alternative expressions.
#[derive(Clone, Debug, Default)]
pub struct BinaryPackageAlternativesResolution<'file, 'data: 'file> {
    pub alternatives: Vec<BinaryPackageSingleDependencyResolution<'file, 'data>>,
}

/// A collection of [BinaryPackageAlternativesResolution] satisfying a list of independent constraints.
#[derive(Clone, Debug, Default)]
pub struct BinaryPackageDependenciesResolution<'file, 'data: 'file> {
    pub parts: Vec<BinaryPackageAlternativesResolution<'file, 'data>>,
}

#[derive(Clone, Debug)]
struct BinaryPackageEntry<'file, 'data: 'file> {
    file: &'file BinaryPackageControlFile<'data>,
    name: String,
    version: PackageVersion,
    arch: String,
    deps: PackageDependencyFields,
}

#[derive(Clone, Debug)]
struct VirtualBinaryPackageEntry<'file, 'data: 'file> {
    file: &'file BinaryPackageControlFile<'data>,

    /// The version of the virtual package being provided.
    provided_version: Option<DependencyVersionConstraint>,

    /// The package providing it.
    name: String,

    /// The version of the package providing it.
    version: PackageVersion,
}

/// An entity for resolving dependencies between packages.
#[derive(Clone, Debug, Default)]
pub struct DependencyResolver<'file, 'data: 'file> {
    /// Map of package name to entries for each package
    binary_packages: HashMap<String, Vec<BinaryPackageEntry<'file, 'data>>>,

    /// Map of provided package name to packages that provide.
    virtual_binary_packages: HashMap<String, Vec<VirtualBinaryPackageEntry<'file, 'data>>>,
}

impl<'file, 'data: 'file> DependencyResolver<'file, 'data> {
    /// Load an iterable of binary packages into the resolver.
    ///
    /// This effectively indexes the given binary package definitions and enables them to
    /// be discovered during subsequent dependency resolution.
    pub fn load_binary_packages(
        &mut self,
        files: impl Iterator<Item = &'file BinaryPackageControlFile<'data>>,
    ) -> Result<()> {
        for cf in files {
            let package = cf.package()?;

            let entry = BinaryPackageEntry {
                file: cf,
                name: package.to_string(),
                version: cf.version()?,
                arch: cf.architecture()?.to_string(),
                deps: cf.package_dependency_fields()?,
            };

            if let Some(provides) = &entry.deps.provides {
                for variants in provides.requirements() {
                    for dep in variants.iter() {
                        let virtual_entry = VirtualBinaryPackageEntry {
                            file: cf,
                            provided_version: dep.version_constraint.clone(),
                            name: entry.name.clone(),
                            version: entry.version.clone(),
                        };

                        self.virtual_binary_packages
                            .entry(dep.package.clone())
                            .or_default()
                            .push(virtual_entry);
                    }
                }
            }

            self.binary_packages
                .entry(package.to_string())
                .or_default()
                .push(entry);
        }

        Ok(())
    }

    /// Find direct dependencies given a binary control file and a dependency field.
    ///
    /// This will resolve the specified [BinaryDependency] field to a list of constraints
    /// and then find candidate [BinaryPackageControlFile] satisfying all requirements within.
    pub fn find_direct_binary_file_dependencies(
        &self,
        cf: &BinaryPackageControlFile,
        dep: BinaryDependency,
    ) -> Result<BinaryPackageDependenciesResolution> {
        let fields = cf.package_dependency_fields()?;

        let mut res = BinaryPackageDependenciesResolution::default();

        if let Some(deps) = fields.binary_dependency(dep) {
            for req in deps.requirements() {
                let mut variants_res = BinaryPackageAlternativesResolution::default();

                for alt in req.iter() {
                    let mut deps_res = BinaryPackageSingleDependencyResolution {
                        expression: alt.clone(),
                        candidates: vec![],
                    };

                    // Look for concrete packages with this name satisfying the constraints.
                    if let Some(entries) = self.binary_packages.get(&alt.package) {
                        for entry in entries {
                            if alt.package_satisfies(&entry.name, &entry.version, &entry.arch) {
                                deps_res.candidates.push(entry.file);
                            }
                        }
                    }

                    // Look for virtual packages with this name satisfying the constraints.
                    if let Some(entries) = self.virtual_binary_packages.get(&alt.package) {
                        for entry in entries {
                            if alt.package_satisfies_virtual(
                                &alt.package,
                                entry.provided_version.as_ref(),
                            ) {
                                deps_res.candidates.push(entry.file);
                            }
                        }
                    }

                    variants_res.alternatives.push(deps_res);
                }

                res.parts.push(variants_res);
            }
        }

        Ok(res)
    }
}
