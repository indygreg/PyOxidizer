// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    clap::{Arg, ArgMatches, Command},
    debian_packaging::{
        error::DebianError,
        repository::{
            copier::{RepositoryCopier, RepositoryCopierConfig},
            PublishEvent,
        },
    },
    std::sync::{Arc, Mutex},
    thiserror::Error,
};

const URLS_ABOUT: &str = "\
Repository URLs

Various commands accept URLs describing the location of a repository. Here is
how they work.

If a value contains `://`, it will be parsed as a URL. Otherwise it will be
interpreted as a local filesystem path.

The following URL schemes (the part before the `://` in a URL) are recognized:

file://
   A local filesystem path. The path component of the URL is interpreted as
   a filesystem path.

http:// and https://
   A HTTP-based repository.

   Read-only (writes not supported)

null://
   A repository that points to nothing.

   This repository sends all writes to a null / nothing / a black hole.

   By default, `null://` will assume a file does not exist in the destination.
   It is possible to override this behavior by specifying one of the following
   values:

   null://exists-no-integrity-check
      Assumes a file exists without indicating that an integrity check was
      performed.
   null://exists-integrity-verified
      Assumes a file exists and indicates an integrity check was performed.
   null://exists-integrity-mismatch
      Assumes a file exists and its content does not match what the copier
      desires it to be.

   Write-only (reads not supported)

s3://
   An S3 bucket.

   URLs of the form `s3://bucket` anchor the repository at the root of the S3
   bucket.

   URLs of the form `s3://bucket/path` anchor the repository under a key prefix
   in the bucket.

   The AWS client will be resolved using configuration files and environment
   variables as is typical for AWS clients. For example, it looks in
   `~/.aws/config` and in `AWS_*` environment variables.

   Write-only (reads not supported)

In all cases, the URL should point to the base of the Debian repository. This
is typically a directory containing `dists` and `pool` sub-directories.
";

const COPY_REPOSITORY_ABOUT: &str = "\
Copy a Debian repository.

Given a source and destination repository and parameters to control what to
copy, this command will ensure the destination repository has a complete
copy of the content in the source repository.

Repository copying works by reading the `[In]Release` file for a given
distribution, fetching additional indices files (such as `Packages` and
`Sources` files) to find additional content, and bulk copying all found
files to the destination.

Copying is generally smart about avoiding I/O if possible. For example,
a file in the destination will not be written to if it already exists and
has the appropriate content.

# YAML Configuration

A YAML file can be used to specify the configuration of the copy operation(s)
to perform.

The YAML file consists of 1 or more documents. Each document can have the
following keys:

source_url (required) (string)
   The URL or path of the repository to copy from.

destination_url (required) (string)
   The URL or path of the repository to copy to.

distributions (optional) (list[string])
   Names of distributions to copy. Distributions must be located in paths
   like `dists/<value>`.

distribution_paths (optional) (list[string])
   Paths of distributions to copy.

   Use this if a distribution isn't in a directory named `dists/<value>`.

only_components (optional) (list[string])
   Names of components to copy. Common component names include `contrib` and
   `main`.

   If not specified, all advertised components are copied.

binary_packages_copy (optional) (bool)
   Whether to copy binary packages.

binary_packages_only_architectures (optional) (list[string])
   Filter of architectures of binary packages to copy.

installer_binary_packages_copy (optional) (bool)
   Whether to copy installer binary packages (udebs).

installer_binary_packages_only_architectures (optional) (list[string])
   Filter of architectures of installer binary packages to copy.

sources_copy (optional) (bool)
   Whether to copy source packages.

# Partial Copying

By default, a copy operation will copy all content in the specified
distributions. However, it is possible to limit the content that is
copied.

Our definition of _copy_ preserves the repository indices (the
cryptographically signed documents advertising the repository content).
When performing a partial _copy_, rewriting the indices to advertise a
different set of content would invalidate the existing cryptographic
signature, which is not something we want to allow in _copy_ mode.

If partial copying is being performed, all indices files are preserved
without modification, therefore preserving their cryptographic signature.
However, these indices may refer to content that doesn't exist in the
destination. This can result in packaging clients encountering missing
files.
";

#[derive(Debug, Error)]
pub enum DrtError {
    #[error("argument parsing error: {0:?}")]
    Clap(#[from] clap::Error),

    #[error("{0:?}")]
    Debian(#[from] DebianError),

    #[error("I/O error: {0:?}")]
    Io(#[from] std::io::Error),

    #[error("YAML error: {0:?}")]
    SerdeYaml(#[from] serde_yaml::Error),

    #[error("invalid sub-command: {0}")]
    InvalidSubCommand(String),
}

pub type Result<T> = std::result::Result<T, DrtError>;

pub async fn run_cli() -> Result<()> {
    let default_threads = format!("{}", num_cpus::get());

    let app = Command::new("Debian Repository Tool")
        .version("0.1")
        .author("Gregory Szorc <gregory.szorc@gmail.com>")
        .about("Interface with Debian Repositories")
        .arg_required_else_help(true);

    let app = app.arg(
        Arg::new("max-parallel-io")
            .long("--max-parallel-io")
            .takes_value(true)
            .default_value(&default_threads)
            .global(true)
            .help("Maximum number of parallel I/O operations to perform"),
    );

    let app = app.subcommand(
        Command::new("copy-repository")
            .about("Copy a Debian repository between locations")
            .long_about(COPY_REPOSITORY_ABOUT)
            .arg(
                Arg::new("yaml-config")
                    .long("--yaml-config")
                    .takes_value(true)
                    .required(true)
                    .allow_invalid_utf8(true)
                    .help("Path to a YAML file defining the copy configuration"),
            ),
    );

    let mut app =
        app.subcommand(Command::new("urls").about("Print documentation about repository URLs"));

    let matches = app.clone().get_matches();

    match matches.subcommand() {
        Some(("copy-repository", args)) => command_copy_repository(args).await,
        Some(("urls", _)) => {
            println!("{}", URLS_ABOUT);
            Ok(())
        }
        Some((command, _)) => Err(DrtError::InvalidSubCommand(command.to_string())),
        None => {
            app.print_help()?;
            Ok(())
        }
    }
}

async fn command_copy_repository(args: &ArgMatches) -> Result<()> {
    let max_parallel_io = args.value_of_t::<usize>("max-parallel-io")?;

    let yaml_path = args
        .value_of_os("yaml-config")
        .expect("yaml-config argument is required");

    let f = std::fs::File::open(yaml_path)?;
    let config: RepositoryCopierConfig = serde_yaml::from_reader(f)?;

    let pb = Arc::new(Mutex::new(None));

    let cb = Box::new(move |event: PublishEvent| match event {
        PublishEvent::WriteSequenceBeginWithTotalBytes(total) => {
            let mut bar = pbr::ProgressBar::new(total);
            bar.set_units(pbr::Units::Bytes);

            pb.lock().unwrap().replace(bar);
        }
        PublishEvent::WriteSequenceProgressBytes(count) => {
            pb.lock()
                .unwrap()
                .as_mut()
                .expect("progress bar should be defined")
                .add(count);
        }
        PublishEvent::WriteSequenceFinished => {
            let mut guard = pb.lock().unwrap();
            guard
                .as_mut()
                .expect("progress bar should be defined")
                .finish();
            guard.take();
        }
        PublishEvent::CopyPhaseBegin(_) | PublishEvent::CopyPhaseEnd(_) => {
            println!("{}", event);
        }
        _ => {}
    });

    RepositoryCopier::copy_from_config(config, max_parallel_io, &Some(cb)).await?;

    Ok(())
}
