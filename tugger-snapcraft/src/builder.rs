// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    crate::yaml::Snapcraft,
    anyhow::{anyhow, Context, Result},
    duct::cmd,
    log::warn,
    simple_file_manifest::{FileEntry, FileManifest},
    std::{
        io::{BufRead, BufReader},
        path::Path,
    },
};

/// Represents an invocation of the `snapcraft` command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SnapcraftInvocation {
    /// Arguments to pass to `snapcraft`.
    pub args: Vec<String>,

    /// Whether to purge the build directory before running this command.
    pub purge_build: bool,
}

/// Entity used to build snaps by calling into `snapcraft`.
///
/// This is a rather low-level interface for calling into `snapcraft` in a slightly opinionated
/// manner.
///
/// Instances are bound to a `Snapcraft` instance, which represents a `snapcraft.yaml` file,
/// a virtual file manifest of files to install, and a series of invocations, which are
/// essentially arguments to `snapcraft`.
///
/// When we `build()`, we materialize all the files into a build directory and invoke
/// `snapcraft` repeatedly until we're complete.
#[derive(Clone, Debug, PartialEq)]
pub struct SnapcraftBuilder<'a> {
    pub(crate) snap: Snapcraft<'a>,
    pub(crate) invocations: Vec<SnapcraftInvocation>,
    pub(crate) install_files: FileManifest,
}

impl<'a> SnapcraftBuilder<'a> {
    /// Create a new builder using the specified `snapcraft.yaml` file.
    pub fn new(snap: Snapcraft<'a>) -> Self {
        Self {
            snap,
            invocations: vec![],
            install_files: FileManifest::default(),
        }
    }

    /// Obtain the `Snapcraft` inside this instance.
    pub fn snap(&self) -> &Snapcraft<'a> {
        &self.snap
    }

    /// Obtain the registered snapcraft invocations in this instance.
    pub fn invocations(&self) -> &Vec<SnapcraftInvocation> {
        &self.invocations
    }

    /// Obtain the files to be installed in the build environment.
    pub fn install_files(&self) -> &FileManifest {
        &self.install_files
    }

    /// Register a new `snapcraft` invocation to run during the build.
    #[must_use]
    pub fn add_invocation(mut self, invocation: SnapcraftInvocation) -> Self {
        self.invocations.push(invocation);
        self
    }

    /// Register a new `snapcraft` invocation from just arguments.
    ///
    /// The first registered invocation will purge the build path by
    /// default.
    #[must_use]
    pub fn add_invocation_args(mut self, args: &[impl AsRef<str>]) -> Self {
        self.invocations.push(SnapcraftInvocation {
            args: args
                .iter()
                .map(|x| x.as_ref().to_string())
                .collect::<Vec<_>>(),
            purge_build: self.invocations.is_empty(),
        });
        self
    }

    /// Mark a file as specified by a filesystem path as to be installed in the
    /// build environment.
    pub fn install_file(
        mut self,
        path: impl AsRef<Path>,
        strip_prefix: impl AsRef<Path>,
    ) -> Result<Self> {
        let rel_path = path.as_ref().strip_prefix(strip_prefix.as_ref())?;

        let entry = FileEntry::try_from(path.as_ref())?;

        self.install_files.add_file_entry(rel_path, entry)?;

        Ok(self)
    }

    /// Add files to install from the content of an existing `FileManifest`.
    pub fn install_manifest(mut self, manifest: &FileManifest) -> Result<Self> {
        self.install_files.add_manifest(manifest)?;

        Ok(self)
    }

    /// Invoke `snapcraft` with the given configuration.
    ///
    /// Registered files will be written to `build_path`.
    pub fn build<P: AsRef<Path>>(&self, build_path: P) -> Result<()> {
        for invocation in &self.invocations {
            self.build_invocation(build_path.as_ref(), invocation)?;
        }

        Ok(())
    }

    /// Build a single `SnapcraftInvocation`.
    ///
    /// This will perform the following actions:
    ///
    /// 1. Potentially purge `build_path`.
    /// 2. Materialize registered files into `build_path`.
    /// 3. Materialize `snapcraft.yaml` into `build_path/snap/snapcraft.yaml`.
    /// 4. Invoke `snapcraft` with the specified arguments.
    pub fn build_invocation<P: AsRef<Path>>(
        &self,
        build_path: P,
        invocation: &SnapcraftInvocation,
    ) -> Result<()> {
        let build_path = build_path.as_ref();

        if invocation.purge_build && build_path.exists() {
            warn!("purging {}", build_path.display());
            remove_dir_all::remove_dir_all(build_path)
                .with_context(|| format!("removing {}", build_path.display()))?;
        }

        if !build_path.exists() {
            std::fs::create_dir_all(build_path)
                .with_context(|| format!("creating {}", build_path.display()))?;
        }

        self.install_files
            .materialize_files(build_path)
            .with_context(|| format!("installing files to {}", build_path.display()))?;

        let snap_path = build_path.join("snap");
        if !snap_path.exists() {
            std::fs::create_dir(&snap_path)
                .with_context(|| format!("creating {}", snap_path.display()))?;
        }

        let snapcraft_yaml_path = snap_path.join("snapcraft.yaml");

        {
            let mut fs = std::fs::File::create(&snapcraft_yaml_path).with_context(|| {
                format!("opening {} for writing", snapcraft_yaml_path.display())
            })?;
            serde_yaml::to_writer(&mut fs, &self.snap)
                .context("serializing to snapcraft.yaml file")?;
        }

        warn!("invoking snapcraft with args: {:?}", &invocation.args);
        let command = cmd("snapcraft", &invocation.args)
            .dir(build_path)
            .stderr_to_stdout()
            .reader()?;
        {
            let reader = BufReader::new(&command);
            for line in reader.lines() {
                warn!("{}", line?);
            }
        }

        let output = command
            .try_wait()?
            .ok_or_else(|| anyhow!("unable to wait on command"))?;
        if output.status.success() {
            Ok(())
        } else {
            Err(anyhow!("error running snapcraft"))
        }
    }
}

#[cfg(test)]
mod tests {
    #[cfg(target_os = "linux")]
    use {
        super::*,
        crate::{SnapApp, SnapPart},
        tugger_common::{glob::evaluate_glob, testutil::*},
    };

    #[cfg(target_os = "linux")]
    #[test]
    fn test_build_rust_project() -> Result<()> {
        // This times out in GitHub Actions for some reason. Probably has to do with
        // nested virtualization.
        if std::env::var("GITHUB_ACTIONS").is_ok() {
            return Ok(());
        }

        if let Ok(output) = cmd("snapcraft", vec!["build", "--help"])
            .stderr_to_stdout()
            .stdout_capture()
            .run()
        {
            if !String::from_utf8_lossy(output.stdout.as_ref()).contains("--destructive-mode") {
                eprintln!("snapcraft doesn't support --destructive-mode; skipping test");
                return Ok(());
            }
        } else {
            eprintln!("error running snapcraft; skipping test");
            return Ok(());
        }

        let test_dir = DEFAULT_TEMP_DIR.path().join("test-build-rust-project");
        let project_path = test_dir.join("testapp");

        let output = cmd(
            "cargo",
            vec![
                "init".to_string(),
                "--bin".to_string(),
                project_path.display().to_string(),
            ],
        )
        .stderr_to_stdout()
        .run()?;
        if !output.status.success() {
            panic!(
                "failed to invoke `cargo init`: {}",
                String::from_utf8_lossy(&output.stdout)
            );
        }

        let name = "testapp";
        let mut snap = Snapcraft::new(
            name.into(),
            "0.1".into(),
            "summary".into(),
            "description".into(),
        );
        snap.base = Some("core18".into());

        // We can't use the rust plugin with --destructive-mode because it will pick
        // up the Rust from the host environment, which will link against a libc
        // possibly not suited for the base image in use. So we reinvent the wheel
        // of building Rust projects and tell it to target musl libc, which will be
        // statically linked.
        snap.add_part(
            name.into(),
            SnapPart {
                plugin: Some("nil".into()),
                source: Some(".".into()),
                build_environment: vec![[("PATH".into(), "${HOME}/.cargo/bin:${PATH}".into())].iter().cloned().collect()],
                override_build: Some(
                    "RUSTC_WRAPPER= cargo install --target x86_64-unknown-linux-musl --path . --root ${SNAPCRAFT_PART_INSTALL} --force".into(),
                ),
                ..SnapPart::default()
            },
        );
        snap.add_app(
            name.into(),
            SnapApp {
                command: Some("bin/testapp".into()),
                ..SnapApp::default()
            },
        );

        let snap_filename = "testapp_0.1.amd64.snap";

        let mut builder = SnapcraftBuilder::new(snap).add_invocation_args(&[
            "snap",
            "--destructive-mode",
            "--debug",
            "-o",
            snap_filename,
        ]);
        for path in evaluate_glob(&project_path, "**/*")? {
            builder = builder.install_file(path, &project_path)?;
        }

        builder.build(test_dir.join("build"))?;

        let dest_path = test_dir.join("build").join(snap_filename);

        assert!(dest_path.exists());

        Ok(())
    }
}
