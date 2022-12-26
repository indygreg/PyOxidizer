// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    anyhow::{anyhow, Context, Result},
    cargo_toml::Manifest,
    clap::{Arg, ArgAction, ArgMatches, Command},
    duct::cmd,
    git2::{Repository, Status},
    once_cell::sync::Lazy,
    std::{
        ffi::OsString,
        io::{BufRead, BufReader},
        path::Path,
    },
};

pub mod documentation;

const CARGO_LOCKFILE_NAME: &str = "new-project-cargo.lock";

/// Packages in the workspace we should ignore.
static IGNORE_PACKAGES: Lazy<Vec<&'static str>> =
    Lazy::new(|| vec!["pyembed-bench", "pyoxy", "release"]);

/// Order that packages should be released in.
static RELEASE_ORDER: Lazy<Vec<&'static str>> = Lazy::new(|| {
    vec![
        "starlark-dialect-build-targets",
        "tugger-common",
        "tugger-rust-toolchain",
        "tugger-binary-analysis",
        //"tugger-rpm",
        "tugger-snapcraft",
        "tugger-apple",
        "tugger-windows",
        "tugger-windows-codesign",
        "tugger-code-signing",
        "tugger-wix",
        "python-packed-resources",
        "python-packaging",
        "tugger",
        "python-oxidized-importer",
        "pyembed",
        "pyoxidizer",
        // "pyoxy",
    ]
});

fn get_workspace_members(path: &Path) -> Result<Vec<String>> {
    let manifest = Manifest::from_path(path)?;
    Ok(manifest
        .workspace
        .ok_or_else(|| anyhow!("no [workspace] section"))?
        .members)
}

/// Update the `[package]` version key in a Cargo.toml file.
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

/// Obtain the package version string from a Cargo.toml file.
fn cargo_toml_package_version(path: &Path) -> Result<String> {
    let manifest = cargo_toml::Manifest::from_path(path)?;

    Ok(manifest
        .package
        .ok_or_else(|| anyhow!("no [package]"))?
        .version()
        .to_string())
}

enum PackageLocation {
    /// Relative path inside the PyOxidizer repository.
    RepoRelative,
    /// No explicit location, which uses defaults/remote index.
    Remote,
}

fn update_cargo_toml_dependency_package_location(
    path: &Path,
    package: &str,
    location: PackageLocation,
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
                    PackageLocation::RepoRelative => local_path.clone(),
                    PackageLocation::Remote => format!("# {}", local_path),
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

/// Update the pyembed crate version in environment.rs.
fn update_environment_rs_pyembed_version(root: &Path, version: &semver::Version) -> Result<()> {
    let path = root.join("pyoxidizer").join("src").join("environment.rs");

    let mut lines = Vec::new();

    let fh = std::fs::File::open(&path).with_context(|| format!("opening {}", path.display()))?;
    let reader = BufReader::new(fh);

    let mut seen_version = false;
    for line in reader.lines() {
        let line = line?;

        lines.push(if line.starts_with("const PYEMBED_CRATE_VERSION: ") {
            seen_version = true;

            format!("const PYEMBED_CRATE_VERSION: &str = \"{}\";", version)
        } else {
            line
        });
    }
    lines.push("".to_string());

    if !seen_version {
        return Err(anyhow!(
            "PYEMBED_CRATE_VERSION line not found in {}",
            path.display()
        ));
    }

    std::fs::write(&path, lines.join("\n"))?;

    Ok(())
}

/// Update version string in pyoxidizer.bzl file.
fn update_pyoxidizer_bzl_version(root: &Path, version: &semver::Version) -> Result<()> {
    // Version string in file does not have pre-release component.
    let version = semver::Version::new(version.major, version.minor, version.patch);

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
    pyembed_force_path: bool,
) -> Result<()> {
    // For all version changes, ensure the new project Cargo.lock content stays up
    // to date.
    let cargo_lock_path = root
        .join("pyoxidizer")
        .join("src")
        .join(CARGO_LOCKFILE_NAME);

    let lock_current = std::fs::read_to_string(&cargo_lock_path)?;
    let lock_wanted = generate_new_project_cargo_lock(root, pyembed_force_path)?;

    if lock_current != lock_wanted {
        println!("updating {} to reflect changes", cargo_lock_path.display());
        std::fs::write(&cargo_lock_path, &lock_wanted)?;
    }

    match package {
        "pyembed" => {
            update_environment_rs_pyembed_version(root, version)?;
        }
        "pyoxidizer" => {
            update_pyoxidizer_bzl_version(root, version)?;
        }
        _ => {}
    }

    Ok(())
}

pub fn run_cmd<S>(
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
    run_cmd(package, root, "cargo", vec!["update"], vec![]).context("running cargo update")
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

    // This shouldn't be needed. But it serves as an extra guard to prevent
    // things from getting out of sync.
    ensure_new_project_cargo_lock_current(root)
        .context("validating new project Cargo.lock is current")?;

    let manifest_path = root.join(package).join("Cargo.toml");
    let manifest = Manifest::from_path(&manifest_path)
        .with_context(|| format!("reading {}", manifest_path.display()))?;

    let version = manifest
        .package
        .as_ref()
        .ok_or_else(|| anyhow!("no [package]"))?
        .version();

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
            let commit_message = String::from_utf8_lossy(commit.message_bytes());

            if let Some(message) = commit_message.strip_prefix("releasebot: ") {
                // Ignore commits that should have no bearing on this package.
                if message.starts_with("pre-release-workspace-normalize")
                    || message.starts_with("post-release-workspace-normalize")
                    || message.starts_with("post-release-version-change ")
                {
                    println!(
                        "{}: ignoring releasebot commit: {} ({})",
                        package,
                        oid,
                        message.strip_suffix('\n').unwrap_or(message),
                    );
                    continue;
                } else if let Some(s) = message.strip_prefix("release-version-change ") {
                    // This commit updated the version of a package. We need to look at the package
                    // and version change to see if it impacts us.

                    let parts = s
                        .strip_suffix('\n')
                        .unwrap_or(message)
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
        semver::Version::new(
            current_version.major,
            current_version.minor,
            current_version.patch,
        )
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
                        PackageLocation::Remote
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

        // Force pyembed to use a path = reference in the Cargo.lock at this point because
        // the new version may not exist on the registry yet. We'll amend this down below,
        // after publishing.
        reflect_package_version_change(root, package, &release_version, package == "pyembed")?;

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

        if package == "pyembed" {
            println!("pyembed: updating pyembed publish in new project Cargo.lock");
            reflect_package_version_change(root, package, &release_version, false)?;
        }

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
                    PackageLocation::Remote
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
            root,
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

    let version = manifest
        .package
        .as_ref()
        .ok_or_else(|| anyhow!("no [package]"))?
        .version();

    println!("{}: existing Cargo.toml version: {}", package, version);
    let mut next_version = semver::Version::parse(version).context("parsing package version")?;

    match version_bump {
        VersionBump::Minor => {
            next_version.minor += 1;
        }
        VersionBump::Patch => {
            next_version.patch += 1;
        }
    }

    next_version.pre = semver::Prerelease::new("pre")?;

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
                PackageLocation::RepoRelative
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
    run_cmd(package, root, "cargo", vec!["update"], vec![]).context("running cargo update")?;

    reflect_package_version_change(root, package, &next_version, false)?;

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

fn command_release(repo_root: &Path, args: &ArgMatches, repo: &Repository) -> Result<()> {
    let publish = !args.get_flag("no_publish");

    let version_bump = if args.get_flag("patch") {
        VersionBump::Patch
    } else {
        VersionBump::Minor
    };

    let (do_pre, pre_start_name, post_start_name) =
        if let Some(start_at) = args.get_one::<String>("start_at") {
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

    let up_to = args.get_one::<String>("up_to");

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

    // The cargo lock content will change as part of the release as dependencies
    // are updated. We verify it multiple times during the release. But we want to
    // start in a consistent state, so we check it up front as well.
    ensure_new_project_cargo_lock_current(repo_root)?;

    let workspace_toml = repo_root.join("Cargo.toml");
    let workspace_packages =
        get_workspace_members(&workspace_toml).context("parsing workspace Cargo.toml")?;

    let problems = workspace_packages
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
                    repo_root,
                    repo,
                    &dependency_update_packages,
                    package,
                    publish,
                )
                .with_context(|| format!("releasing {}", package))?;
            }

            // This is the final package we're releasing. Stop here.
            if Some(*package) == up_to.map(|x| &**x) {
                eprintln!("stopping release process at {}", package);
                break;
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
                package,
                version_bump,
            )
            .with_context(|| format!("incrementing version for {}", package))?;
        }

        if Some(*package) == up_to.map(|x| &**x) {
            break;
        }
    }

    Ok(())
}

fn generate_new_project_cargo_lock(repo_root: &Path, pyembed_force_path: bool) -> Result<String> {
    // The lock file is derived from a new Rust project, similarly to the one that
    // `pyoxidizer init-rust-project` generates. Ideally we'd actually call that command.
    // However, there's a bit of a chicken and egg problem, especially as we call this
    // function as part of the release. So/ we emulate what the autogenerated Cargo.toml
    // would resemble. We don't need it to match exactly: we just need to ensure the
    // dependency set is complete.

    const PACKAGE_NAME: &str = "placeholder_project";

    let temp_dir = tempfile::TempDir::new()?;
    let project_path = temp_dir.path().join(PACKAGE_NAME);
    let cargo_toml_path = project_path.join("Cargo.toml");

    let pyembed_version =
        cargo_toml_package_version(&repo_root.join("pyembed").join("Cargo.toml"))?;

    let pyembed_entry = format!(
        "[dependencies.pyembed]\nversion = \"{}\"\ndefault-features = false\n",
        pyembed_version
    );

    // For pre-releases, refer to pyembed by its repo path, as pre-releases aren't
    // published. Otherwise, leave as-is: Cargo.lock should pick up the version published
    // on the registry and embed that metadata.
    let pyembed_entry = if pyembed_version.ends_with("-pre") || pyembed_force_path {
        format!(
            "{}path = \"{}\"\n",
            pyembed_entry,
            repo_root.join("pyembed").display()
        )
    } else {
        pyembed_entry
    };

    cmd(
        "cargo",
        vec![
            "init".to_string(),
            "--bin".to_string(),
            format!("{}", project_path.display()),
        ],
    )
    .stdout_to_stderr()
    .run()?;

    let extra_toml_path = repo_root
        .join("pyoxidizer")
        .join("src")
        .join("templates")
        .join("cargo-extra.toml.hbs");

    let mut manifest_data = std::fs::read_to_string(&cargo_toml_path)?;
    manifest_data.push_str(&pyembed_entry);

    // This is a handlebars template but it has nothing special. So just read as
    // a regualar file.
    manifest_data.push_str(&std::fs::read_to_string(&extra_toml_path)?);

    std::fs::write(&cargo_toml_path, manifest_data.as_bytes())?;

    cmd("cargo", vec!["generate-lockfile", "--offline"])
        .dir(&project_path)
        .stdout_to_stderr()
        .run()?;

    let cargo_lock_path = project_path.join("Cargo.lock");

    // Filter out our placeholder package because the value will be different for
    // generated projects.
    let mut lock_file = cargo_lock::Lockfile::load(&cargo_lock_path)?;

    lock_file.packages = lock_file
        .packages
        .drain(..)
        .filter(|package| package.name.as_str() != PACKAGE_NAME)
        .collect::<Vec<_>>();

    Ok(lock_file.to_string())
}

/// Ensures the new project Cargo lock file in source control is up to date with reality.
fn ensure_new_project_cargo_lock_current(repo_root: &Path) -> Result<()> {
    let path = repo_root
        .join("pyoxidizer")
        .join("src")
        .join(CARGO_LOCKFILE_NAME);

    let file_text = std::fs::read_to_string(&path)?;
    let wanted_text = generate_new_project_cargo_lock(repo_root, false)?;

    if file_text == wanted_text {
        Ok(())
    } else {
        Err(anyhow!("{} is not up to date", path.display()))
    }
}

fn command_generate_new_project_cargo_lock(repo_root: &Path, _args: &ArgMatches) -> Result<()> {
    print!("{}", generate_new_project_cargo_lock(repo_root, false)?);

    Ok(())
}

fn command_synchronize_generated_files(repo_root: &Path) -> Result<()> {
    let cargo_lock = generate_new_project_cargo_lock(repo_root, false)?;
    crate::documentation::generate_sphinx_files(repo_root)?;

    let pyoxidizer_src_path = repo_root.join("pyoxidizer").join("src");
    let lock_path = pyoxidizer_src_path.join("new-project-cargo.lock");

    println!("writing {}", lock_path.display());
    std::fs::write(&lock_path, cargo_lock.as_bytes())?;

    Ok(())
}

fn main_impl() -> Result<()> {
    let cwd = std::env::current_dir()?;

    let repo = Repository::discover(&cwd).context("finding Git repository")?;
    let repo_root = repo
        .workdir()
        .ok_or_else(|| anyhow!("unable to resolve working directory"))?;

    let matches = Command::new("PyOxidizer Releaser")
        .version("0.1")
        .author("Gregory Szorc <gregory.szorc@gmail.com>")
        .about("Perform releases from the PyOxidizer repository")
        .arg_required_else_help(true)
        .subcommand(
            Command::new("generate-new-project-cargo-lock")
                .about("Emit a Cargo.lock file for the pyembed crate"),
        )
        .subcommand(
            Command::new("release")
                .about("Perform release actions")
                .arg(
                    Arg::new("no_publish")
                        .long("no-publish")
                        .action(ArgAction::SetTrue)
                        .help("Do not publish release"),
                )
                .arg(
                    Arg::new("patch")
                        .long("patch")
                        .action(ArgAction::SetTrue)
                        .help("Bump the patch version instead of the minor version"),
                )
                .arg(
                    Arg::new("start_at")
                        .long("start-at")
                        .action(ArgAction::Set)
                        .help("Where to resume the release process"),
                )
                .arg(
                    Arg::new("up_to")
                        .long("up-to")
                        .action(ArgAction::Set)
                        .help("Name of final package to release"),
                ),
        )
        .subcommand(Command::new("synchronize-generated-files").about("Write out generated files"))
        .get_matches();

    match matches.subcommand() {
        Some(("release", args)) => command_release(repo_root, args, &repo),
        Some(("generate-new-project-cargo-lock", args)) => {
            command_generate_new_project_cargo_lock(repo_root, args)
        }
        Some(("synchronize-generated-files", _)) => command_synchronize_generated_files(repo_root),
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
