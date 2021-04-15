// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    anyhow::{anyhow, Context, Result},
    cargo_toml::Manifest,
    clap::{App, AppSettings, Arg, ArgMatches, SubCommand},
    duct::cmd,
    git2::{Repository, Status},
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
        "cryptographic-message-syntax",
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
        "tugger-windows-codesign",
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

/// Update version string in pyoxidizer.bzl file.
fn update_pyoxidizer_bzl_version(root: &Path, version: &semver::Version) -> Result<()> {
    // Version string in file does not have pre-release component.
    let mut version = version.clone();
    version.pre.clear();

    let path = root.join("pyoxidizer.bzl");

    let mut lines = Vec::new();

    let fh = std::fs::File::open(&path).with_context(|| format!("opening {}", path.display()))?;
    let reader = BufReader::new(fh);

    let mut seen_version = false;
    for line in reader.lines() {
        let line = line?;

        lines.push(if line.starts_with("PYOXIDIZER_VERSION = ") {
            seen_version = true;

            format!("PYOXIDIZER_VERSION = \"{}\"", version)
        } else {
            line
        });
    }
    lines.push("".to_string());

    if !seen_version {
        return Err(anyhow!(
            "PYOXIDIZER_VERSION line not found in {}",
            path.display()
        ));
    }

    std::fs::write(&path, lines.join("\n"))?;

    Ok(())
}

// Reflect version changes to a given package.
fn reflect_package_version_change(
    root: &Path,
    package: &str,
    version: &semver::Version,
) -> Result<()> {
    if package == "pyoxidizer" {
        update_pyoxidizer_bzl_version(root, version)?;
    }

    Ok(())
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

fn run_cargo_update_package(root: &Path, package: &str) -> Result<i32> {
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
    .context("running cargo update")
}

fn release_package(
    root: &Path,
    repo: &Repository,
    workspace_packages: &[&str],
    package: &str,
    publish: bool,
) -> Result<()> {
    println!("releasing {}", package);
    println!(
        "(to resume from this position use --start-at=pre:{})",
        package
    );

    let manifest_path = root.join(package).join("Cargo.toml");
    let manifest = Manifest::from_path(&manifest_path)
        .with_context(|| format!("reading {}", manifest_path.display()))?;

    let version = &manifest
        .package
        .ok_or_else(|| anyhow!("no [package]"))?
        .version;

    println!("{}: existing Cargo.toml version: {}", package, version);

    let current_version = semver::Version::parse(version).context("parsing package version")?;

    // Find previous tags for this package so we can see if there are any
    // meaningful changes to the package since the last tag.
    let mut package_tags = vec![];
    repo.tag_foreach(|oid, name| {
        let name = String::from_utf8_lossy(name);

        if let Some(tag) = name.strip_prefix(&format!("refs/tags/{}/", package)) {
            println!("{}: found previous release tag {}@{}", package, tag, oid);
            package_tags.push((tag.to_string(), oid));
        }

        true
    })?;

    let restore_version = if package_tags.is_empty() {
        None
    } else {
        // Find the last tag and see if there are file changes.
        let mut walker = repo.revwalk()?;

        walker.set_sorting(git2::Sort::TOPOLOGICAL)?;
        walker.push_head()?;

        for (_, oid) in &package_tags {
            walker.push(*oid)?;
        }

        let mut restore_version = None;

        for oid in walker {
            let oid = oid?;

            // Stop traversal when we get to a prior tag.
            if let Some((tag, _)) = package_tags.iter().find(|(_, tag_oid)| &oid == tag_oid) {
                restore_version = Some(tag.clone());
                break;
            }

            let commit = repo.find_commit(oid)?;

            let old_tree = commit.parent(0)?.tree()?;
            let new_tree = commit.tree()?;

            let diff = repo.diff_tree_to_tree(Some(&old_tree), Some(&new_tree), None)?;

            let relevant = diff.deltas().any(|delta| {
                if let Some(old_path) = delta.old_file().path_bytes() {
                    if String::from_utf8_lossy(old_path).starts_with(&format!("{}/", package)) {
                        return true;
                    }
                }

                if let Some(new_path) = delta.new_file().path_bytes() {
                    if String::from_utf8_lossy(new_path).starts_with(&format!("{}/", package)) {
                        return true;
                    }
                }

                false
            });

            // Commit didn't touch this package. Ignore it.
            if !relevant {
                continue;
            }

            // Commit messages beginning with releasebot: belong to us and are special.
            // Other messages are meaningful commits and result in a release.
            if let Some(message) = commit.message_bytes().strip_prefix(b"releasebot: ") {
                // Ignore commits that should have no bearing on this package.
                if message.starts_with(b"pre-release-workspace-normalize")
                    || message.starts_with(b"post-release-workspace-normalize")
                    || message.starts_with(b"post-release-version-change ")
                {
                    println!(
                        "{}: ignoring releasebot commit: {} ({})",
                        package,
                        oid,
                        String::from_utf8_lossy(message)
                    );
                    continue;
                } else if let Some(s) = message.strip_prefix(b"release-version-change ") {
                    // This commit updated the version of a package. We need to look at the package
                    // and version change to see if it impacts us.

                    let message = String::from_utf8_lossy(s);
                    let parts = message
                        .strip_suffix("\n")
                        .unwrap_or(&*message)
                        .split(' ')
                        .collect::<Vec<_>>();

                    if parts.len() != 4 {
                        return Err(anyhow!(
                            "malformed release-version-change commit message: {}",
                            message
                        ));
                    }

                    let (changed_package, old_version, new_version) =
                        (parts[0], parts[1], parts[3]);

                    let old_version =
                        semver::Version::parse(old_version).context("parsing old version")?;
                    let new_version =
                        semver::Version::parse(new_version).context("parsing new version")?;

                    // Restored an earlier version. Not meaningful to us.
                    if new_version <= old_version {
                        println!(
                            "{}: ignoring commit downgrading {} from {} to {}: {}",
                            package, changed_package, old_version, new_version, oid
                        );
                        continue;
                    } else {
                        println!("{}: commit necessitates package release: {}", package, oid);
                        break;
                    }
                } else {
                    return Err(anyhow!("unhandled releasebot: commit: {}", oid));
                }
            // TODO remove this block after next release cycle.
            } else if commit.message_bytes().starts_with(b"release: update ")
                || commit.message_bytes().starts_with(b"release: bump ")
            {
                println!("{}: ignoring legacy release commit: {}", package, oid);
            } else {
                println!(
                    "{}: found meaningful commit touching this package; release needed: {}",
                    package, oid
                );
                break;
            }
        }

        restore_version
    };

    // If there were no meaningful changes, the release version is the last tag.
    // Otherwise we strip the pre component from the version string and release it.
    let release_version = if let Some(restore_version) = &restore_version {
        println!(
            "{}: no meaningful commits since last release; restoring version {}",
            package, restore_version
        );
        semver::Version::parse(restore_version).context("parsing old released version")?
    } else {
        let mut v = current_version.clone();
        v.pre.clear();

        v
    };

    println!(
        "{}: current version: {}; new version: {}",
        package, current_version, release_version
    );

    let commit_message = format!(
        "releasebot: release-version-change {} {} -> {}",
        package, current_version, release_version
    );

    if current_version == release_version {
        println!(
            "{}: calculated release version identical to current version; not changing anything",
            package
        );
    } else {
        println!("{}: updating version to {}", package, release_version);
        update_cargo_toml_package_version(&manifest_path, &release_version.to_string())?;

        println!(
            "{}: checking workspace packages for version updates",
            package
        );
        for other_package in workspace_packages {
            // Reflect new dependency version in all packages in this repo.
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
                    "updated version"
                } else {
                    "unchanged unchanged version"
                }
            );

            // If this was a downgrade, update dependency location to remote.
            if release_version < current_version {
                println!(
                    "{}: {} {}",
                    package,
                    cargo_toml.display(),
                    if update_cargo_toml_dependency_package_location(
                        &cargo_toml,
                        package,
                        Location::Remote
                    )? {
                        "updated location"
                    } else {
                        "unchanged location"
                    }
                );
            }
        }

        // We need to ensure Cargo.lock reflects any version changes.
        run_cargo_update_package(root, package)?;

        reflect_package_version_change(root, package, &release_version)?;

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
                commit_message.clone(),
            ],
            vec![],
        )
        .context("creating Git commit")?;
    }

    if release_version <= current_version {
        println!(
            "{}: release version not newer than current version; not performing release",
            package
        );
    } else if publish {
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
                commit_message,
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
    } else {
        println!(
            "{}: publishing disabled; would have released {}",
            package, release_version
        );
    }

    Ok(())
}

fn update_package_version(
    root: &Path,
    workspace_packages: &[&str],
    package: &str,
    version_bump: VersionBump,
) -> Result<()> {
    println!("updating package version for {}", package);
    println!(
        "(to resume from this position use --start-at=post:{})",
        package
    );

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
                "updated version"
            } else {
                "unchanged version"
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
                "updated location"
            } else {
                "unchanged location"
            }
        );
    }

    println!(
        "{}: running cargo update to reflect version increment",
        package
    );
    run_cmd(package, &root, "cargo", vec!["update"], vec![]).context("running cargo update")?;

    reflect_package_version_change(root, package, &next_version)?;

    println!("{}: creating Git commit to reflect version bump", package);
    run_cmd(
        package,
        root,
        "git",
        vec![
            "commit".to_string(),
            "-a".to_string(),
            "-m".to_string(),
            format!(
                "releasebot: post-release-version-change {} {} -> {}",
                package, version, next_version
            ),
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

fn update_workspace_toml(
    repo_root: &Path,
    path: &Path,
    workspace_packages: &[String],
    commit_message: &str,
) -> Result<()> {
    write_workspace_toml(path, workspace_packages).context("writing workspace Cargo.toml")?;
    println!("running cargo update to reflect workspace change");
    run_cmd("workspace", repo_root, "cargo", vec!["update"], vec![])
        .context("cargo update to reflect workspace changes")?;
    println!("performing git commit to reflect workspace changes");
    run_cmd(
        "workspace",
        repo_root,
        "git",
        vec!["commit", "-a", "-m", commit_message],
        vec![],
    )
    .context("git commit to reflect workspace changes")?;

    Ok(())
}

fn command_release(repo_root: &Path, args: &ArgMatches, repo: &Repository) -> Result<()> {
    let publish = !args.is_present("no_publish");

    let version_bump = if args.is_present("patch") {
        VersionBump::Patch
    } else {
        VersionBump::Minor
    };

    let (do_pre, pre_start_name, post_start_name) =
        if let Some(start_at) = args.value_of("start_at") {
            let mut parts = start_at.splitn(2, ':');

            let prefix = parts
                .next()
                .ok_or_else(|| anyhow!("start_at value must contain a :"))?;
            let suffix = parts
                .next()
                .ok_or_else(|| anyhow!("start_at value must contain a value after :"))?;

            match prefix {
                "pre" => (true, Some(suffix), None),
                "post" => (false, None, Some(suffix)),
                _ => {
                    return Err(anyhow!(
                        "illegal start_at value: must begin with `pre:` or `post:`"
                    ))
                }
            }
        } else {
            (true, None, None)
        };

    let head_commit = repo.head()?.peel_to_commit()?;
    println!(
        "HEAD at {}; to abort release, run `git reset --hard {}`",
        head_commit.id(),
        head_commit.id()
    );

    let statuses = repo.statuses(None)?;
    let mut extra_files = vec![];
    let mut repo_dirty = false;

    for status in statuses.iter() {
        match status.status() {
            Status::WT_NEW => {
                extra_files.push(String::from_utf8_lossy(status.path_bytes()).to_string());
            }
            Status::IGNORED => {}
            _ => {
                eprintln!(
                    "repo contains dirty tracked path: {}",
                    String::from_utf8_lossy(status.path_bytes())
                );
                repo_dirty = true;
            }
        }
    }

    if repo_dirty {
        return Err(anyhow!("repo has uncommited changes; refusing to proceed"));
    }

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
        update_workspace_toml(
            repo_root,
            &workspace_toml,
            &new_workspace_packages,
            "releasebot: pre-release-workspace-normalize",
        )?;
    }

    let problems = new_workspace_packages
        .iter()
        .filter(|p| !RELEASE_ORDER.contains(&p.as_str()) && !IGNORE_PACKAGES.contains(&p.as_str()))
        .collect::<Vec<_>>();

    if !problems.is_empty() {
        for p in problems {
            eprintln!("problem with workspace package: {}", p);
        }
        return Err(anyhow!("workspace packages mismatch with release script"));
    }

    // We construct a list of all potential packages to use for updating
    // references because if we resume a partial release, the Cargo.toml defining
    // workspace members may have already been pruned, leading to these packages
    // not being considered.
    let mut dependency_update_packages = RELEASE_ORDER.clone();
    dependency_update_packages.extend(DISABLE_PACKAGES.iter());
    dependency_update_packages.extend(IGNORE_PACKAGES.iter());
    dependency_update_packages.sort_unstable();

    if do_pre {
        let mut seen_package = pre_start_name.is_none();

        for package in RELEASE_ORDER.iter() {
            if Some(*package) == pre_start_name {
                seen_package = true;
            }

            if seen_package {
                let prefix = format!("{}/", package);

                let mut package_dirty = false;
                for path in &extra_files {
                    if path.starts_with(&prefix) {
                        eprintln!("repo contains untracked path in package: {}", path);
                        package_dirty = true;
                    }
                }

                if package_dirty {
                    return Err(anyhow!("package {} is dirty: refusing to proceed", package));
                }

                release_package(
                    &repo_root,
                    repo,
                    &dependency_update_packages,
                    *package,
                    publish,
                )
                .with_context(|| format!("releasing {}", package))?;
            }
        }
    }

    let mut seen_package = post_start_name.is_none();
    for package in RELEASE_ORDER.iter() {
        if Some(*package) == post_start_name {
            seen_package = true;
        }

        if seen_package {
            update_package_version(
                repo_root,
                &dependency_update_packages,
                *package,
                version_bump,
            )
            .with_context(|| format!("incrementing version for {}", package))?;
        }
    }

    // This is done after all version updates are performed because oxidized-importer
    // referencing pyembed can confuse Cargo due to conflicting requirements for the
    // pythonXY dependency.
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

        update_workspace_toml(
            repo_root,
            &workspace_toml,
            &packages,
            "releasebot: post-release-workspace-normalize",
        )?;
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
            SubCommand::with_name("generate-pyembed-license")
                .about("Emit license information for the pyembed crate"),
        )
        .subcommand(
            SubCommand::with_name("release")
                .about("Perform release actions")
                .arg(
                    Arg::with_name("no_publish")
                        .long("no-publish")
                        .help("Do not publish release"),
                )
                .arg(
                    Arg::with_name("patch")
                        .help("Bump the patch version instead of the minor version"),
                )
                .arg(
                    Arg::with_name("start_at")
                        .long("start-at")
                        .takes_value(true)
                        .help("Where to resume the release process"),
                ),
        )
        .get_matches();

    match matches.subcommand() {
        ("release", Some(args)) => command_release(repo_root, args, &repo),
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
