// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Licensing functionality.

use {
    crate::environment::{canonicalize_path, RustEnvironment},
    anyhow::{anyhow, Context, Result},
    cargo_toml::Manifest,
    guppy::{
        graph::{
            cargo::{CargoOptions, CargoResolverVersion, CargoSet},
            feature::{named_feature_filter, StandardFeatures},
            DependencyDirection,
        },
        platform::{Platform, PlatformSpec, TargetFeatures, Triple},
        MetadataCommand,
    },
    log::{info, warn},
    python_packaging::licensing::{
        ComponentFlavor, LicenseFlavor, LicensedComponent, LicensedComponents, SourceLocation,
    },
    std::{path::Path, sync::Arc},
};

/// Log a summary of licensing info.
pub fn log_licensing_info(components: &LicensedComponents) {
    for line in components.license_summary().lines() {
        warn!("{}", line);
    }
    warn!("");

    if let Some(report) = components.interesting_report() {
        for line in report.lines() {
            warn!("{}", line);
        }
        warn!("");
    }

    for line in components.spdx_license_breakdown().lines() {
        info!("{}", line);
    }
    info!("");
}

/// Resolve licenses from a cargo manifest.
pub fn licenses_from_cargo_manifest<'a>(
    manifest_path: impl AsRef<Path>,
    all_features: bool,
    features: impl IntoIterator<Item = &'a str>,
    target_triple: Option<impl Into<String>>,
    rust_environment: &RustEnvironment,
    include_main_package: bool,
) -> Result<LicensedComponents> {
    let manifest_path = canonicalize_path(manifest_path.as_ref())?;
    let features = features.into_iter().collect::<Vec<&str>>();

    let manifest_dir = manifest_path
        .parent()
        .ok_or_else(|| anyhow!("could not determine parent director of manifest"))?;

    if all_features {
        warn!(
            "evaluating dependencies for {} using all features",
            manifest_path.display()
        );
    } else {
        warn!(
            "evaluating dependencies for {} using features: {}",
            manifest_path.display(),
            features.join(", ")
        );
    }

    let manifest = Manifest::from_path(&manifest_path)?;
    let main_package = manifest
        .package
        .ok_or_else(|| anyhow!("could not find a package in Cargo manifest"))?
        .name;

    let mut command = MetadataCommand::new();

    command.cargo_path(&rust_environment.cargo_exe);

    command.current_dir(manifest_dir);

    // We need to set RUSTC so things work with our managed Rust toolchain. But
    // guppy doesn't have an API for that. So reinvent this wheel.
    let mut command = command.cargo_command();

    command.env("RUSTC", &rust_environment.rustc_exe);

    let output = command.output().context("invoking cargo metadata")?;
    if !output.status.success() {
        return Err(anyhow!(
            "error running cargo: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let stdout = String::from_utf8(output.stdout).context("converting output to UTF-8")?;

    let json = stdout
        .lines()
        .find(|line| line.starts_with('{'))
        .ok_or_else(|| anyhow!("could not find JSON output"))?;

    let metadata = guppy::CargoMetadata::parse_json(json)?;

    let package_graph = metadata.build_graph()?;

    let main_package_id = package_graph
        .packages()
        .find(|p| p.name() == main_package)
        .ok_or_else(|| anyhow!("could not find package {} in metadata", main_package))?
        .id();

    let workspace_package_set = package_graph.resolve_workspace();
    let main_package_set = package_graph.query_forward([main_package_id])?.resolve();

    // Simulate a cargo build from the current platform targeting a specified platform or the current.
    let mut cargo_options = CargoOptions::new();
    cargo_options.set_resolver(CargoResolverVersion::V2);
    cargo_options.set_host_platform(PlatformSpec::Platform(Arc::new(Platform::current()?)));
    cargo_options.set_target_platform(if let Some(triple) = target_triple {
        PlatformSpec::Platform(Arc::new(Platform::from_triple(
            Triple::new(triple.into())?,
            TargetFeatures::Unknown,
        )))
    } else {
        PlatformSpec::current()?
    });

    // Apply our desired features settings.
    let initials = workspace_package_set.to_feature_set(named_feature_filter(
        if all_features {
            StandardFeatures::All
        } else {
            StandardFeatures::Default
        },
        features,
    ));

    // This is always empty because we don't use the functionality.
    let features_only = package_graph
        .resolve_none()
        .to_feature_set(StandardFeatures::All);

    let cargo_set = CargoSet::new(initials, features_only, &cargo_options)?;

    // The meaningful packages for licensing are those that are built for the target
    // unioned with proc macro crates for the host. It is important we capture the host
    // proc macro crates because those can generate code that end up in the final binary.
    let target_features = cargo_set.target_features();

    let proc_macro_feature_set = package_graph
        .resolve_ids(cargo_set.proc_macro_links().map(|link| link.to().id()))?
        .to_feature_set(StandardFeatures::All);
    let proc_macro_host_feature_set = cargo_set
        .host_features()
        .intersection(&proc_macro_feature_set);

    let relevant_feature_set = target_features.union(&proc_macro_host_feature_set);

    // Turn it into packages.
    //
    // Note: this has packages for the entire workspace. We still need to intersect
    // with the packages set relevant to the main package!
    let feature_list = relevant_feature_set.packages_with_features(DependencyDirection::Forward);

    // Now turn the packages into licensing metadata.
    let mut components = LicensedComponents::default();

    for feature_list in feature_list {
        let package = feature_list.package();

        if !main_package_set.contains(package.id())? {
            continue;
        }

        if package.id() == main_package_id && !include_main_package {
            continue;
        }

        let flavor = ComponentFlavor::RustCrate(package.name().into());

        let mut component = if let Some(expression) = package.license() {
            // `/` is sometimes used as a delimiter for some reason.
            let expression = expression.replace('/', " OR ");

            LicensedComponent::new_spdx(flavor, &expression)?
        } else {
            LicensedComponent::new(flavor, LicenseFlavor::None)
        };

        for author in package.authors() {
            component.add_author(author);
        }
        if let Some(value) = package.homepage() {
            component.set_homepage(value);
        }
        if let Some(value) = package.repository() {
            component.set_source_location(SourceLocation::Url(value.to_string()));
        }

        components.add_component(component);
    }

    Ok(components)
}
