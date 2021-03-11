// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    anyhow::{anyhow, Context, Result},
    cargo_toml::Manifest,
    clap::{App, AppSettings, Arg, ArgMatches, SubCommand},
    duct::cmd,
    git2::Repository,
    once_cell::sync::Lazy,
    serde::Deserialize,
    std::{
        collections::{BTreeMap, BTreeSet},
        ffi::OsString,
        fmt::Write,
        io::{BufRead, BufReader},
        path::Path,
    },
};

/// Packages we should disable in the workspace before releasing.
static DISABLE_PACKAGES: Lazy<Vec<&'static str>> = Lazy::new(|| vec!["oxidized-importer"]);

/// Packages in the workspace we should ignore.
static IGNORE_PACKAGES: Lazy<Vec<&'static str>> = Lazy::new(|| vec!["release"]);

/// Order that packages should be released in.
static RELEASE_ORDER: Lazy<Vec<&'static str>> = Lazy::new(|| {
    vec![
        "crypographic-message-syntax",
        "starlark-dialect-build-targets",
        "tugger-common",
        "tugger-file-manifest",
        "tugger-binary-analysis",
        "tugger-debian",
        "tugger-licensing",
        "tugger-licensing-net",
        "tugger-rpm",
        "tugger-snapcraft",
        "tugger-apple-bundle",
        "tugger-apple-codesign",
        "tugger-apple",
        "tugger-windows",
        "tugger-wix",
        "tugger",
        "text-stub-library",
        "python-packed-resources",
        "python-packaging",
        "pyembed",
        "pyoxidizer",
    ]
});

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
            if !seen_dependency_section && line.ends_with(&format!("dependencies.{}]", package)) {
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

enum Location {
    LocalPath,
    Remote,
}

fn update_cargo_toml_dependency_package_location(
    path: &Path,
    package: &str,
    location: Location,
) -> Result<bool> {
    let local_path = format!("path = \"../{}\"", package);

    let mut lines = Vec::new();

    let fh = std::fs::File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let reader = BufReader::new(fh);

    let mut seen_dependency_section = false;
    let mut seen_path = false;
    let mut changed = false;
    for line in reader.lines() {
        let line = line?;

        lines.push(
            if !seen_dependency_section && line.ends_with(&format!("dependencies.{}]", package)) {
                seen_dependency_section = true;
                line
            } else if seen_dependency_section
                && !seen_path
                && (line.starts_with("path = \"") || line.starts_with("# path = \""))
            {
                seen_path = true;

                let new_line = match location {
                    Location::LocalPath => local_path.clone(),
                    Location::Remote => format!("# {}", local_path),
                };

                if new_line != line {
                    changed = true;
                }

                new_line
            } else {
                line
            },
        );
    }
    lines.push("".to_string());

    let data = lines.join("\n");
    std::fs::write(path, data)?;

    Ok(changed)
}

fn run_cmd<S>(
    package: &str,
    dir: &Path,
    program: &str,
    args: S,
    ignore_errors: Vec<String>,
) -> Result<i32>
where
    S: IntoIterator,
    S::Item: Into<OsString>,
{
    let mut found_ignore_string = false;

    let command = cmd(program, args)
        .dir(dir)
        .stderr_to_stdout()
        .unchecked()
        .reader()
        .context("launching command")?;
    {
        let reader = BufReader::new(&command);
        for line in reader.lines() {
            let line = line?;

            for s in ignore_errors.iter() {
                if line.contains(s) {
                    found_ignore_string = true;
                }
            }
            println!("{}: {}", package, line);
        }
    }
    let output = command
        .try_wait()
        .context("waiting on process")?
        .ok_or_else(|| anyhow!("unable to wait on command"))?;

    let code = output.status.code().unwrap_or(1);

    if output.status.success() || found_ignore_string {
        Ok(code)
    } else {
        Err(anyhow!(
            "command exited {}",
            output.status.code().unwrap_or(1)
        ))
    }
}

fn release_package(root: &Path, workspace_packages: &[String], package: &str) -> Result<()> {
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

    // We need to ensure Cargo.lock reflects any version changes.
    println!(
        "{}: running cargo update to ensure proper version string reflected",
        package
    );
    run_cmd(
        package,
        &root,
        "cargo",
        vec!["update", "-p", package],
        vec![],
    )
    .context("running cargo update")?;

    // We need to perform a Git commit to ensure the working directory is clean, otherwise
    // Cargo complains. We could run with --allow-dirty. But that exposes us to other dangers,
    // such as packaging files in the source directory we don't want to package.
    println!("{}: creating Git commit to reflect release", package);
    run_cmd(
        package,
        root,
        "git",
        vec![
            "commit".to_string(),
            "-a".to_string(),
            "-m".to_string(),
            format!("{}: release version {}", package, release_version),
        ],
        vec![],
    )
    .context("creating Git commit")?;

    if run_cmd(
        package,
        &root.join(package),
        "cargo",
        vec!["publish"],
        vec![format!(
            "crate version `{}` is already uploaded",
            release_version
        )],
    )
    .context("running cargo publish")?
        == 0
    {
        println!("{}: sleeping to wait for crates index to update", package);
        std::thread::sleep(std::time::Duration::from_secs(30));
    };

    println!(
        "{}: checking workspace packages for package location updates",
        package
    );
    for other_package in workspace_packages {
        let cargo_toml = root.join(other_package).join("Cargo.toml");
        println!(
            "{}: {} {}",
            package,
            cargo_toml.display(),
            if update_cargo_toml_dependency_package_location(
                &cargo_toml,
                package,
                Location::Remote
            )? {
                "updated"
            } else {
                "unchanged"
            }
        );
    }

    println!(
        "{}: running cargo update to ensure proper location reflected",
        package
    );
    run_cmd(
        package,
        &root,
        "cargo",
        vec!["update", "-p", package],
        vec![],
    )
    .context("running cargo update")?;

    println!("{}: amending Git commit to reflect release", package);
    run_cmd(
        package,
        root,
        "git",
        vec![
            "commit".to_string(),
            "-a".to_string(),
            "--amend".to_string(),
            "-m".to_string(),
            format!("{}: release version {}", package, release_version),
        ],
        vec![],
    )
    .context("creating Git commit")?;

    let tag = format!("{}/{}", package, release_version);
    run_cmd(
        package,
        root,
        "git",
        vec!["tag".to_string(), "-f".to_string(), tag.clone()],
        vec![],
    )
    .context("creating Git tag")?;

    run_cmd(
        package,
        root,
        "git",
        vec![
            "push".to_string(),
            "-f".to_string(),
            "--tag".to_string(),
            "origin".to_string(),
            tag,
        ],
        vec![],
    )
    .context("pushing git tag")?;

    Ok(())
}

fn update_package_version(
    root: &Path,
    workspace_packages: &[String],
    package: &str,
    version_bump: VersionBump,
) -> Result<()> {
    println!("updating package version for {}", package);

    let manifest_path = root.join(package).join("Cargo.toml");
    let manifest = Manifest::from_path(&manifest_path)
        .with_context(|| format!("reading {}", manifest_path.display()))?;

    let version = &manifest
        .package
        .ok_or_else(|| anyhow!("no [package]"))?
        .version;

    println!("{}: existing Cargo.toml version: {}", package, version);
    let mut next_version = semver::Version::parse(version).context("parsing package version")?;

    match version_bump {
        VersionBump::Minor => next_version.increment_minor(),
        VersionBump::Patch => next_version.increment_patch(),
    }

    next_version.pre = vec![semver::AlphaNumeric("pre".to_string())];

    update_cargo_toml_package_version(&manifest_path, &next_version.to_string())
        .context("updating Cargo.toml package version")?;

    println!(
        "{}: checking workspace packages for version update",
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
                &next_version.to_string()
            )? {
                "updated"
            } else {
                "unchanged"
            }
        );
        println!(
            "{}: {} {}",
            package,
            cargo_toml.display(),
            if update_cargo_toml_dependency_package_location(
                &cargo_toml,
                package,
                Location::LocalPath
            )? {
                "updated"
            } else {
                "unchanged"
            }
        );
    }

    println!(
        "{}: running cargo update to reflect version increment",
        package
    );
    run_cmd(package, &root, "cargo", vec!["update"], vec![]).context("running cargo update")?;

    println!("{}: creating Git commit to reflect version bump", package);
    run_cmd(
        package,
        root,
        "git",
        vec![
            "commit".to_string(),
            "-a".to_string(),
            "-m".to_string(),
            format!("{}: bump version to {}", package, next_version),
        ],
        vec![],
    )
    .context("creating Git commit")?;

    Ok(())
}

#[derive(Clone, Copy, Debug)]
enum VersionBump {
    Minor,
    Patch,
}

fn command_release(repo_root: &Path, args: &ArgMatches) -> Result<()> {
    let version_bump = if args.is_present("patch") {
        VersionBump::Patch
    } else {
        VersionBump::Minor
    };

    let ignores_pre = if let Some(values) = args.values_of("ignore_pre") {
        values.collect()
    } else {
        vec![]
    };
    let ignores_post = if let Some(values) = args.values_of("ignore_post") {
        values.collect()
    } else {
        vec![]
    };

    ensure_pyembed_license_current(repo_root)?;

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

        println!("running cargo update to reflect workspace change");
        run_cmd("workspace", repo_root, "cargo", vec!["update"], vec![])
            .context("cargo update to reflect workspace changes")?;
        println!("performing git commit to reflect workspace changes");
        run_cmd(
            "workspace",
            repo_root,
            "git",
            vec![
                "commit",
                "-a",
                "-m",
                "release: remove packages from workspace to facilitate release",
            ],
            vec![],
        )
        .context("git commit to reflect workspace changes")?;
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
        if !ignores_pre.contains(package) {
            release_package(&repo_root, &new_workspace_packages, *package)
                .with_context(|| format!("releasing {}", package))?;
        }
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
        let mut packages = workspace_packages;
        for p in DISABLE_PACKAGES.iter() {
            packages.push(p.to_string());
        }

        packages.sort();

        write_workspace_toml(&workspace_toml, &packages)?;
    }

    let workspace_packages = get_workspace_members(&workspace_toml)?;

    for package in RELEASE_ORDER.iter() {
        if !ignores_post.contains(package) {
            update_package_version(repo_root, &workspace_packages, *package, version_bump)
                .with_context(|| format!("incrementing version for {}", package))?;
        }
    }

    Ok(())
}

#[derive(Debug, Deserialize)]
struct CargoDenyLicenseList {
    licenses: Vec<(String, Vec<String>)>,
    unlicensed: Vec<String>,
}

fn generate_pyembed_license(repo_root: &Path) -> Result<String> {
    let pyembed_manifest_path = repo_root.join("pyembed").join("Cargo.toml");

    let output = cmd(
        "cargo",
        vec![
            "deny".to_string(),
            "--features".to_string(),
            "jemalloc,mimalloc,snmalloc".to_string(),
            "--manifest-path".to_string(),
            pyembed_manifest_path.display().to_string(),
            "list".to_string(),
            "-f".to_string(),
            "Json".to_string(),
        ],
    )
    .stdout_capture()
    .run()?;

    let deny: CargoDenyLicenseList = serde_json::from_slice(&output.stdout)?;

    let mut crates = BTreeMap::new();

    for (license, entries) in &deny.licenses {
        for entry in entries {
            let crate_name = entry.split(' ').next().unwrap();

            crates
                .entry(crate_name.to_string())
                .or_insert_with(BTreeSet::new)
                .insert(license.clone());
        }
    }

    let mut text = String::new();

    writeln!(
        &mut text,
        "// This Source Code Form is subject to the terms of the Mozilla Public"
    )?;
    writeln!(
        &mut text,
        "// License, v. 2.0. If a copy of the MPL was not distributed with this"
    )?;
    writeln!(
        &mut text,
        "// file, You can obtain one at https://mozilla.org/MPL/2.0/."
    )?;
    writeln!(&mut text)?;

    writeln!(
        &mut text,
        "pub fn pyembed_licenses() -> anyhow::Result<Vec<tugger_licensing::LicensedComponent>> {{"
    )?;
    writeln!(&mut text, "    let mut res = vec![];")?;
    writeln!(&mut text)?;

    for (crate_name, licenses) in crates {
        let expression = licenses.into_iter().collect::<Vec<_>>().join(" OR ");

        writeln!(
            &mut text,
            "    let mut component = tugger_licensing::LicensedComponent::new_spdx(\"{}\", \"{}\")?;",
            crate_name, expression
        )?;
        writeln!(
            &mut text,
            "    component.set_flavor(tugger_licensing::ComponentFlavor::RustCrate);"
        )?;
        writeln!(&mut text, "    res.push(component);")?;
        writeln!(&mut text)?;
    }

    writeln!(&mut text, "    Ok(res)")?;
    writeln!(&mut text, "}}")?;

    let mut text = cmd("rustfmt", &vec!["--emit", "stdout"])
        .dir(repo_root)
        .stdout_capture()
        .stdin_bytes(text.as_bytes())
        .read()?;

    text.push('\n');

    Ok(text)
}

/// Ensures the `pyembed-license.rs` file in source control is up to date with reality.
fn ensure_pyembed_license_current(repo_root: &Path) -> Result<()> {
    let path = repo_root
        .join("pyoxidizer")
        .join("src")
        .join("pyembed-license.rs");

    let file_text = std::fs::read_to_string(&path)?;
    let wanted_text = generate_pyembed_license(repo_root)?;

    if file_text == wanted_text {
        Ok(())
    } else {
        Err(anyhow!(
            "{} does not match expected content",
            path.display()
        ))
    }
}

fn command_generate_pyembed_license(repo_root: &Path, _args: &ArgMatches) -> Result<()> {
    print!("{}", generate_pyembed_license(repo_root)?);

    Ok(())
}

fn main_impl() -> Result<()> {
    let cwd = std::env::current_dir()?;

    let repo = Repository::discover(&cwd).context("finding Git repository")?;
    let repo_root = repo
        .workdir()
        .ok_or_else(|| anyhow!("unable to resolve working directory"))?;

    let matches = App::new("PyOxidizer Releaser")
        .setting(AppSettings::ArgRequiredElseHelp)
        .version("0.1")
        .author("Gregory Szorc <gregory.szorc@gmail.com>")
        .about("Perform releases from the PyOxidizer repository")
        .subcommand(
            SubCommand::with_name("release")
                .about("Perform release actions")
                .arg(
                    Arg::with_name("patch")
                        .help("Bump the patch version instead of the minor version"),
                )
                .arg(
                    Arg::with_name("ignore_pre")
                        .long("ignore-pre")
                        .takes_value(true)
                        .multiple(true)
                        .help("Name of package to ignore when releasing"),
                )
                .arg(
                    Arg::with_name("ignore_post")
                        .long("ignore-post")
                        .takes_value(true)
                        .multiple(true)
                        .help("Name of package to ignore post releases"),
                ),
        )
        .subcommand(
            SubCommand::with_name("generate-pyembed-license")
                .about("Emit license information for the pyembed crate"),
        )
        .get_matches();

    match matches.subcommand() {
        ("release", Some(args)) => command_release(repo_root, args),
        ("generate-pyembed-license", Some(args)) => {
            command_generate_pyembed_license(repo_root, args)
        }
        _ => Err(anyhow!("invalid sub-command")),
    }
}

fn main() {
    let exit_code = match main_impl() {
        Ok(()) => 0,
        Err(err) => {
            eprintln!("Error: {:?}", err);
            1
        }
    };

    std::process::exit(exit_code);
}
