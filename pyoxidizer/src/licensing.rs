// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Licensing functionality.

use {
    anyhow::{anyhow, Context, Result},
    cargo_toml::Manifest,
    guppy::{
        graph::{
            cargo::{CargoOptions, CargoResolverVersion},
            feature::{named_feature_filter, FeatureFilter, StandardFeatures},
            DependencyDirection,
        },
        MetadataCommand,
    },
    log::warn,
    python_packaging::licensing::{
        ComponentFlavor, LicenseFlavor, LicensedComponent, LicensedComponents,
    },
    std::path::Path,
};

/// Log a summary of licensing info.
pub fn log_licensing_info(components: &LicensedComponents) {
    for component in components.license_spdx_components() {
        warn!(
            "{} uses SPDX licenses {}",
            component.flavor(),
            component
                .spdx_expression()
                .expect("should have SPDX expression")
        );
    }

    warn!(
        "All SPDX licenses: {}",
        components.all_spdx_license_names().join(", ")
    );
    for component in components.license_missing_components() {
        warn!("{} lacks a known software license", component.flavor());
    }
    for component in components.license_public_domain_components() {
        warn!("{} is in the public domain", component.flavor());
    }
    for component in components.license_unknown_components() {
        warn!("{} has an unknown software license", component.flavor());
    }
    for component in components.license_copyleft_components() {
        warn!("Component has copyleft license: {}", component.flavor());
    }
}

/// Resolve licenses from a cargo manifest.
pub fn licenses_from_cargo_manifest<'a>(
    manifest_path: impl AsRef<Path>,
    all_features: bool,
    features: impl IntoIterator<Item = &'a str>,
    cargo_path: Option<&Path>,
    include_main_package: bool,
) -> Result<LicensedComponents> {
    let manifest_path = manifest_path.as_ref();
    let features = features.into_iter().collect::<Vec<&str>>();

    warn!(
        "evaluating dependencies for {} using features {}",
        manifest_path.display(),
        features.join(",")
    );

    let manifest = Manifest::from_path(manifest_path)?;
    let main_package = manifest
        .package
        .ok_or_else(|| anyhow!("could not find a package in Cargo manifest"))?
        .name;

    let mut command = MetadataCommand::new();

    if let Some(path) = cargo_path {
        command.cargo_path(path);
    }

    command.manifest_path(manifest_path);

    let package_graph = command.build_graph().context("resolving cargo metadata")?;
    let feature_graph = package_graph.feature_graph();

    let main_package_id = package_graph
        .packages()
        .find(|p| p.name() == main_package)
        .ok_or_else(|| anyhow!("could not find package {} in metadata", main_package))?
        .id();

    // Simulate a cargo build using the features specified.
    let mut cargo_options = CargoOptions::new();
    cargo_options.set_resolver(CargoResolverVersion::V2);

    let feature_filter: Box<dyn FeatureFilter> = if all_features {
        Box::new(StandardFeatures::All)
    } else {
        Box::new(named_feature_filter(StandardFeatures::Default, features))
    };

    let cargo_set = feature_graph
        .query_workspace(feature_filter)
        .resolve()
        .into_cargo_set(&cargo_options)?;

    // Turn the cargo set into packages, filtering out build and dev dependencies, since
    // they don't affect run-time licensing.
    let package_set = cargo_set
        .package_graph()
        .query_forward([main_package_id])?
        .resolve_with_fn(|_, link| {
            // Ignore build and dev dependencies since they don't affect run-time licensing.
            !(link.build().is_present() || link.dev_only())
        });

    // Now turn the packages into licensing metadata.

    let mut components = LicensedComponents::default();

    for package in package_set.packages(DependencyDirection::Forward) {
        if package.id() == main_package_id && !include_main_package {
            continue;
        }

        let flavor = ComponentFlavor::RustCrate(package.name().into());

        let component = if let Some(expression) = package.license() {
            // `/` is sometimes used as a delimiter for some reason.
            let expression = expression.replace('/', " OR ");

            LicensedComponent::new_spdx(flavor, &expression)?
        } else {
            LicensedComponent::new(flavor, LicenseFlavor::None)
        };

        components.add_component(component);
    }

    Ok(components)
}
