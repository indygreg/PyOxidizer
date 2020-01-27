// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// THIS MODULE IS DEPRECATED AND THE CODE SHOULD NOT BE USED ANY MORE.
// IT IS AROUND TO SERVE AS A REFERENCE TO HOW THINGS ONCE WERE.

use {
    super::config::Config,
    super::state::BuildContext,
    crate::project_building::HOST,
    anyhow::{anyhow, Context, Result},
    std::fs,
    std::path::Path,
};

impl BuildContext {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        project_path: &Path,
        config: Config,
        host: Option<&str>,
        target: &str,
        release: bool,
        force_artifacts_path: Option<&Path>,
        verbose: bool,
    ) -> Result<Self> {
        let config_parent_path = config
            .config_path
            .parent()
            .with_context(|| "resolving parent path of config")?;

        let host_triple = if let Some(v) = host {
            v.to_string()
        } else {
            HOST.to_string()
        };

        let build_path = config.build_config.build_path.clone();

        // Build Rust artifacts into build path, not wherever Rust chooses.
        let target_base_path = build_path.join("target");

        let apps_base_path = build_path.join("apps");

        // This assumes we invoke as `cargo build --target`, otherwise we don't get the
        // target triple in the directory path unless cross compiling.
        let target_triple_base_path =
            target_base_path
                .join(target)
                .join(if release { "release" } else { "debug" });

        let app_name = config.build_config.application_name.clone();

        let exe_name = if target.contains("pc-windows") {
            format!("{}.exe", &app_name)
        } else {
            app_name.clone()
        };

        let app_target_path = target_triple_base_path.join(&app_name);

        let app_path = apps_base_path
            .join(&app_name)
            .join(target)
            .join(if release { "release" } else { "debug" });
        let app_exe_target_path = target_triple_base_path.join(&exe_name);
        let app_exe_path = app_path.join(&exe_name);

        // Artifacts path is:
        // 1. force_artifacts_path (if defined)
        // 2. A "pyoxidizer" directory in the target directory.
        let pyoxidizer_artifacts_path = match force_artifacts_path {
            Some(path) => path.to_path_buf(),
            None => target_triple_base_path.join("pyoxidizer"),
        };

        let distributions_path = build_path.join("distribution");

        let python_distribution_path = pyoxidizer_artifacts_path.join("python.dummy");

        let cargo_toml_path = project_path.join("Cargo.toml");
        if !cargo_toml_path.exists() {
            return Err(anyhow!("{} does not exist", cargo_toml_path.display()));
        }

        let cargo_toml_data = fs::read(&cargo_toml_path)?;
        let cargo_config = cargo_toml::Manifest::from_slice(&cargo_toml_data)?;

        Ok(BuildContext {
            project_path: project_path.to_path_buf(),
            config_path: config.config_path.clone(),
            config_parent_path: config_parent_path.to_path_buf(),
            cargo_config,
            verbose,
            build_path,
            app_name,
            app_path,
            app_exe_path,
            distributions_path,
            host_triple,
            target_triple: target.to_string(),
            release,
            target_base_path,
            target_triple_base_path,
            app_target_path,
            app_exe_target_path,
            pyoxidizer_artifacts_path,
            python_distribution_path,
        })
    }
}
