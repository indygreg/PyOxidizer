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
    std::collections::{HashMap, HashSet, VecDeque},
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

impl<'file, 'data: 'file> BinaryPackageSingleDependencyResolution<'file, 'data> {
    /// Whether the set of packages satisfying the constraint is empty.
    pub fn is_empty(&self) -> bool {
        self.candidates.is_empty()
    }

    /// Iterate over packages satisfying this dependency expression.
    pub fn packages(&self) -> impl Iterator<Item = &'file BinaryPackageControlFile<'data>> + '_ {
        self.candidates.iter().copied()
    }

    /// Iterate over packages while also emitting the expression being satisfied.
    pub fn packages_with_expression(
        &self,
    ) -> impl Iterator<Item = (&'_ SingleDependency, &'file BinaryPackageControlFile<'data>)> + '_
    {
        self.candidates.iter().map(|p| (&self.expression, *p))
    }

    /// Obtain all candidates in this data structure, indexed by package name.
    pub fn group_by_package_name(
        &self,
    ) -> HashMap<&'file str, Vec<&'file BinaryPackageControlFile<'data>>> {
        let mut h: HashMap<&str, Vec<&BinaryPackageControlFile>> = HashMap::new();

        for cf in self.candidates.iter() {
            let entry = h
                .entry(cf.package().expect(
                    "Package field should have been validated during dependency resolution",
                ))
                .or_default();

            entry.push(cf);
        }

        h
    }
}

/// A collection of [BinaryPackageSingleDependencyResolution] satisfying a set of alternative expressions.
#[derive(Clone, Debug, Default)]
pub struct BinaryPackageAlternativesResolution<'file, 'data: 'file> {
    pub alternatives: Vec<BinaryPackageSingleDependencyResolution<'file, 'data>>,
}

impl<'file, 'data: 'file> BinaryPackageAlternativesResolution<'file, 'data> {
    /// Whether no packages satisfy constraints from this list of dependency expressions.
    ///
    /// Returns true if the set of dependency expressions is empty or if all expressions have
    /// empty packages lists.
    pub fn is_empty(&self) -> bool {
        self.alternatives.is_empty() || self.alternatives.iter().any(|x| x.is_empty())
    }

    /// Obtain alternative dependency constraints for this set.
    pub fn alternative_constraints(&self) -> impl Iterator<Item = &'_ SingleDependency> {
        self.alternatives.iter().map(|alt| &alt.expression)
    }

    /// Iterate over all packages in this set of alternatives.
    ///
    /// There may be duplicates in the output stream.
    pub fn packages(&self) -> impl Iterator<Item = &'file BinaryPackageControlFile<'data>> + '_ {
        self.alternatives.iter().map(|alt| alt.packages()).flatten()
    }

    /// Iterate over packages while also emitting the expression being satisfied.
    pub fn packages_with_expression(
        &self,
    ) -> impl Iterator<Item = (&'_ SingleDependency, &'file BinaryPackageControlFile<'data>)> + '_
    {
        self.alternatives
            .iter()
            .map(|alt| alt.packages_with_expression())
            .flatten()
    }

    /// Prune empty alternatives from this data structure.
    ///
    /// Dependency expressions not having any satisfying packages will be removed.
    pub fn prune_empty(&mut self) {
        self.alternatives = self
            .alternatives
            .drain(..)
            .filter(|alt| !alt.is_empty())
            .collect::<Vec<_>>();
    }
}

/// A collection of [BinaryPackageAlternativesResolution] satisfying a list of independent constraints.
#[derive(Clone, Debug, Default)]
pub struct BinaryPackageDependenciesResolution<'file, 'data: 'file> {
    pub parts: Vec<BinaryPackageAlternativesResolution<'file, 'data>>,
}

impl<'file, 'data: 'file> BinaryPackageDependenciesResolution<'file, 'data> {
    /// Iterate over all packages referenced by this instance.
    ///
    /// This returns all packages satisfying all alternatives in the list of expressions.
    ///
    /// There may be duplicates in the output stream.
    pub fn packages(&self) -> impl Iterator<Item = &'file BinaryPackageControlFile<'data>> + '_ {
        self.parts.iter().map(|req| req.packages()).flatten()
    }

    /// Iterate over packages while also emitting the expression being satisfied.
    pub fn packages_with_expression(
        &self,
    ) -> impl Iterator<Item = (&'_ SingleDependency, &'file BinaryPackageControlFile<'data>)> + '_
    {
        self.parts
            .iter()
            .map(|req| req.packages_with_expression())
            .flatten()
    }

    /// Iterate over dependency alternates that have no satisfying packages.
    pub fn empty_requirements(
        &self,
    ) -> impl Iterator<Item = &'_ BinaryPackageAlternativesResolution<'file, 'data>> {
        self.parts.iter().filter(|alts| alts.is_empty())
    }

    /// Whether there are unsatisfied dependency constraints in this result.
    ///
    /// Returns true if any of the dependency requirements sets are empty.
    pub fn has_unsatisfied(&self) -> bool {
        self.empty_requirements().next().is_none()
    }
}

/// Describes the source of a dependency between binary packages.
#[derive(Clone, Debug)]
pub struct BinaryPackageDependencySource<'file, 'data> {
    /// The package the dependency came from.
    pub package: &'file BinaryPackageControlFile<'data>,
    /// The control file field the dependency constraint came from.
    pub field: BinaryDependency,
    /// The dependency constraint expression being satisfied.
    pub constraint: SingleDependency,
}

#[derive(Clone, Debug, Default)]
pub struct BinaryPackageTransitiveDependenciesResolution<'file, 'data: 'file> {
    evaluation_order: Vec<&'file BinaryPackageControlFile<'data>>,
    reverse_dependencies: HashMap<
        &'file BinaryPackageControlFile<'data>,
        Vec<BinaryPackageDependencySource<'file, 'data>>,
    >,
}

impl<'file, 'data: 'file> BinaryPackageTransitiveDependenciesResolution<'file, 'data> {
    /// Obtain all packages in this collection.
    ///
    /// Packages are guaranteed to be emitted at most once. However, the uniqueness of each
    /// package is defined by the composition of the control paragraph. So packages with the same
    /// name and version may occur multiple times if their control paragraphs aren't identical.
    pub fn packages(&self) -> impl Iterator<Item = &'file BinaryPackageControlFile<'data>> + '_ {
        self.evaluation_order.iter().rev().copied()
    }

    /// Obtain all packages in this collection along with annotations of its reverse dependencies.
    ///
    /// Packages are emitted in the same order as [packages()]. Associated with each entry
    /// is a list of direct dependency sources that caused this package to be present.
    pub fn packages_with_sources(
        &self,
    ) -> impl Iterator<
        Item = (
            &'file BinaryPackageControlFile<'data>,
            &'_ Vec<BinaryPackageDependencySource<'file, 'data>>,
        ),
    > + '_ {
        self.evaluation_order.iter().rev().map(|key| {
            (
                *key,
                self.reverse_dependencies
                    .get(key)
                    .expect("reverse dependencies should have key for all packages"),
            )
        })
    }
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
    pub fn find_direct_binary_package_dependencies(
        &self,
        cf: &BinaryPackageControlFile,
        dep: BinaryDependency,
    ) -> Result<BinaryPackageDependenciesResolution<'file, 'data>> {
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

    /// Resolve binary package dependencies transitively.
    ///
    /// Given a binary package control file and an iterable of dependency fields
    /// to follow, this function will resolve the complete dependency graph for the
    /// given package.
    ///
    /// It works by resolving direct dependencies. Then for each direct dependency,
    /// it resolves its direct dependencies. And this cycle continues until no new
    /// packages are discovered.
    ///
    /// Only the dependency fields specified by `fields` are searched. This allows
    /// callers to e.g. not include `Recommends` or `Suggests` dependencies in the
    /// returned set. Callers are strongly encouraged to include
    /// [BinaryDependency::Depends] and [BinaryDependency::PreDepends] in this
    /// iterable because the dependency graph will be logically incomplete with them.
    pub fn find_transitive_binary_package_dependencies(
        &self,
        cf: &'file BinaryPackageControlFile<'data>,
        fields: impl Iterator<Item = BinaryDependency>,
    ) -> Result<BinaryPackageTransitiveDependenciesResolution<'file, 'data>> {
        let fields = fields.collect::<Vec<_>>();

        // Dependency evaluation queue. Consume from front. Push discovered items to end.
        let mut remaining = VecDeque::new();
        remaining.push_back(cf);

        // Order the dependencies were evaluated in. Packages earlier in this list
        // are dependent on packages later in this list.
        let mut evaluation_order = vec![];

        let mut seen = HashSet::new();

        let mut reverse_dependencies: HashMap<_, Vec<_>> = HashMap::new();
        reverse_dependencies.insert(cf, vec![]);

        // Evaluate direct dependencies for all unexamined packages.
        while let Some(cf) = remaining.pop_front() {
            // We may have already seen this package. Skip if so.
            if seen.contains(cf) {
                continue;
            }

            for field in &fields {
                let deps = self.find_direct_binary_package_dependencies(cf, *field)?;

                // Here is where we could add logic to prune the candidates set, error if
                // we're not satisfying a constraint, etc.

                // Record reverse dependencies to facilitate fast querying and inspection later.
                for (expression, package) in deps.packages_with_expression() {
                    reverse_dependencies.entry(package).or_default().push(
                        BinaryPackageDependencySource {
                            package: cf,
                            field: *field,
                            constraint: expression.clone(),
                        },
                    );
                }

                remaining.extend(deps.packages());
            }

            evaluation_order.push(cf);
            seen.insert(cf);
        }

        Ok(BinaryPackageTransitiveDependenciesResolution {
            evaluation_order,
            reverse_dependencies,
        })
    }
}
