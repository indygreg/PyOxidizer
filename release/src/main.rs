// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    anyhow::{anyhow, Context, Result},
    cargo_toml::Manifest,
    git2::Repository,
    lazy_static::lazy_static,
    std::{
        io::{BufRead, BufReader},
        path::Path,
    },
};

lazy_static! {
    /// Packages we should disable in the workspace before releasing.
    static ref DISABLE_PACKAGES: Vec<&'static str> = vec!["oxidized-importer"];

    /// Packages in the workspace we should ignore.
    static ref IGNORE_PACKAGES: Vec<&'static str> = vec!["release"];

    /// Order that packages should be released in.
    static ref RELEASE_ORDER: Vec<&'static str> = vec![
        "python-packed-resources",
        "python-packaging",
        "pyembed",
        "starlark-dialect-build-targets",
        "tugger",
        "pyoxidizer",
    ];
}

fn get_workspace_members(path: &Path) -> Result<Vec<String>> {
    let manifest = Manifest::from_path(path)?;
    Ok(manifest
        .workspace
        .ok_or_else(|| anyhow!("no [workspace] section"))?
        .members)
}

fn write_workspace_toml(path: &Path, packages: &[String]) -> Result<()> {
    let members = packages
        .iter()
        .map(|x| toml::Value::String(x.to_string()))
        .collect::<Vec<_>>();
    let mut workspace = toml::value::Table::new();
    workspace.insert("members".to_string(), toml::Value::from(members));

    let mut manifest = toml::value::Table::new();
    manifest.insert("workspace".to_string(), toml::Value::Table(workspace));

    let s =
        toml::to_string_pretty(&manifest).context("serializing new workspace TOML to string")?;
    std::fs::write(path, s.as_bytes()).context("writing new workspace Cargo.toml")?;

    Ok(())
}

/// Update the [package] version key in a Cargo.toml file.
fn update_cargo_toml_package_version(path: &Path, version: &str) -> Result<()> {
    let mut lines = Vec::new();

    let fh = std::fs::File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let reader = BufReader::new(fh);

    let mut seen_version = false;
    for line in reader.lines() {
        let line = line?;

        if seen_version {
            lines.push(line);
            continue;
        }

        if line.starts_with("version = \"") {
            seen_version = true;
            lines.push(format!("version = \"{}\"", version));
        } else {
            lines.push(line);
        }
    }
    lines.push("".to_string());

    let data = lines.join("\n");
    std::fs::write(path, data)?;

    Ok(())
}

/// Updates the [dependency.<package] version = field for a workspace package.
fn update_cargo_toml_dependency_package_version(
    path: &Path,
    package: &str,
    new_version: &str,
) -> Result<bool> {
    let mut lines = Vec::new();

    let fh = std::fs::File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let reader = BufReader::new(fh);

    let mut seen_dependency_section = false;
    let mut seen_version = false;
    let mut version_changed = false;
    for line in reader.lines() {
        let line = line?;

        lines.push(
            if !seen_dependency_section && line == format!("[dependencies.{}]", package) {
                seen_dependency_section = true;
                line
            } else if seen_dependency_section && !seen_version && line.starts_with("version = \"") {
                seen_version = true;
                let new_line = format!("version = \"{}\"", new_version);
                version_changed = new_line != line;

                new_line
            } else {
                line
            },
        );
    }
    lines.push("".to_string());

    let data = lines.join("\n");
    std::fs::write(path, data)?;

    Ok(version_changed)
}

fn release_package(
    repo: &Repository,
    root: &Path,
    workspace_packages: &[String],
    package: &str,
) -> Result<()> {
    println!("releasing {}", package);

    let manifest_path = root.join(package).join("Cargo.toml");
    let manifest = Manifest::from_path(&manifest_path)
        .with_context(|| format!("reading {}", manifest_path.display()))?;

    let version = &manifest
        .package
        .ok_or_else(|| anyhow!("no [package]"))?
        .version;

    println!("{}: existing Cargo.toml version: {}", package, version);

    let version = semver::Version::parse(version).context("parsing package version")?;
    let mut release_version = version.clone();
    release_version.pre.clear();
    let mut next_version = version.clone();
    next_version.increment_minor();

    if version.is_prerelease() {
        println!("{}: removing pre-release version", package);
        update_cargo_toml_package_version(&manifest_path, &release_version.to_string())?;
    }

    println!(
        "{}: checking workspace packages for version updated",
        package
    );
    for other_package in workspace_packages {
        let cargo_toml = root.join(other_package).join("Cargo.toml");
        println!(
            "{}: {} {}",
            package,
            cargo_toml.display(),
            if update_cargo_toml_dependency_package_version(
                &cargo_toml,
                package,
                &release_version.to_string(),
            )? {
                "updated"
            } else {
                "unchanged"
            }
        );
    }

    Ok(())
}

fn release() -> Result<()> {
    let cwd = std::env::current_dir()?;

    let repo = Repository::discover(&cwd).context("finding Git repository")?;
    let repo_root = repo
        .workdir()
        .ok_or_else(|| anyhow!("unable to resolve working directory"))?;

    let workspace_toml = repo_root.join("Cargo.toml");
    let workspace_packages =
        get_workspace_members(&workspace_toml).context("parsing workspace Cargo.toml")?;

    let new_workspace_packages = workspace_packages
        .iter()
        .filter(|p| !DISABLE_PACKAGES.contains(&p.as_str()))
        .cloned()
        .collect::<Vec<_>>();

    if new_workspace_packages != workspace_packages {
        println!("removing packages from {}", workspace_toml.display());
        write_workspace_toml(&workspace_toml, &new_workspace_packages)
            .context("writing workspace Cargo.toml")?;
    }

    if !new_workspace_packages
        .iter()
        .all(|p| RELEASE_ORDER.contains(&p.as_str()) || IGNORE_PACKAGES.contains(&p.as_str()))
    {
        return Err(anyhow!(
            "workspace packages does not match expectations in release script"
        ));
    }

    for package in RELEASE_ORDER.iter() {
        release_package(&repo, &repo_root, &new_workspace_packages, *package)
            .with_context(|| format!("releasing {}", package))?;
        break;
    }

    let workspace_packages = get_workspace_members(&workspace_toml)?;
    let workspace_missing_disabled = DISABLE_PACKAGES
        .iter()
        .any(|p| !workspace_packages.contains(&p.to_string()));

    if workspace_missing_disabled {
        println!(
            "re-adding disabled packages from {}",
            workspace_toml.display()
        );
        let mut packages = workspace_packages.clone();
        for p in DISABLE_PACKAGES.iter() {
            packages.push(p.to_string());
        }

        packages.sort();

        write_workspace_toml(&workspace_toml, &packages)?;
    }

    Ok(())
}

fn main() {
    let exit_code = match release() {
        Ok(()) => 0,
        Err(err) => {
            eprintln!("Error: {:?}", err);
            1
        }
    };

    std::process::exit(exit_code);
}
