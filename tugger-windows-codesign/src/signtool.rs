// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Interface to `signtool.exe`. */

use {
    crate::X509SigningCertificate,
    anyhow::{anyhow, Context, Result},
    slog::warn,
    std::{
        io::{BufRead, BufReader},
        path::{Path, PathBuf},
    },
};

#[cfg(target_family = "windows")]
use crate::find_windows_sdk_current_arch_bin_path;

/// Describes a timestamp server to use during signing.
#[derive(Clone, Debug)]
pub enum TimestampServer {
    /// Simple timestamp server.
    ///
    /// Corresponds to `/t` flag to signtool.
    Simple(String),

    /// RFC 3161 timestamp server.
    ///
    /// First item is the URL, second is signing algorithm. Corresponds to signtool
    /// flags `/tr` and `/td`.
    Rfc3161(String, String),
}

#[cfg(target_family = "windows")]
pub fn find_signtool() -> Result<PathBuf> {
    let bin_path = find_windows_sdk_current_arch_bin_path(None).context("finding Windows SDK")?;

    let p = bin_path.join("signtool.exe");

    if p.exists() {
        Ok(p)
    } else {
        Err(anyhow!(
            "unable to locate signtool.exe in Windows SDK at {}",
            bin_path.display()
        ))
    }
}

#[cfg(target_family = "unix")]
pub fn find_signtool() -> Result<PathBuf> {
    Err(anyhow!("finding signtool.exe only supported on Windows"))
}

/// Represents an invocation of `signtool.exe sign` to sign some files.
#[derive(Clone, Debug)]
pub struct SigntoolSign {
    certificate: X509SigningCertificate,
    verbose: bool,
    debug: bool,
    description: Option<String>,
    file_digest_algorithm: Option<String>,
    timestamp_server: Option<TimestampServer>,
    extra_args: Vec<String>,
    sign_files: Vec<PathBuf>,
}

impl SigntoolSign {
    /// Construct a new instance using a specified signing certificate.
    pub fn new(certificate: X509SigningCertificate) -> Self {
        Self {
            certificate,
            verbose: false,
            debug: false,
            description: None,
            file_digest_algorithm: None,
            timestamp_server: None,
            extra_args: vec![],
            sign_files: vec![],
        }
    }

    /// Clone this instance, but not the list of files to sign.
    pub fn clone_settings(&self) -> Self {
        Self {
            certificate: self.certificate.clone(),
            verbose: self.verbose,
            debug: self.debug,
            description: self.description.clone(),
            file_digest_algorithm: self.file_digest_algorithm.clone(),
            timestamp_server: self.timestamp_server.clone(),
            extra_args: self.extra_args.clone(),
            sign_files: vec![],
        }
    }

    /// Run signtool in verbose mode.
    ///
    /// Activates the `/v` flag.
    pub fn verbose(&mut self) -> &mut Self {
        self.verbose = true;
        self
    }

    /// Run signtool in debug mode.
    ///
    /// Activates the `/debug` flag.
    pub fn debug(&mut self) -> &mut Self {
        self.debug = true;
        self
    }

    /// Set the description of the content to be signed.
    ///
    /// This is passed into the `/d` argument.
    pub fn description(&mut self, description: impl ToString) -> &mut Self {
        self.description = Some(description.to_string());
        self
    }

    /// Set the file digest algorithm to use.
    ///
    /// This is passed into the `/fd` argument.
    pub fn file_digest_algorithm(&mut self, algorithm: impl ToString) -> &mut Self {
        self.file_digest_algorithm = Some(algorithm.to_string());
        self
    }

    /// Set the timestamp server to use when signing.
    pub fn timestamp_server(&mut self, server: TimestampServer) -> &mut Self {
        self.timestamp_server = Some(server);
        self
    }

    /// Set extra arguments to pass to signtool.
    ///
    /// Ideally this would not be used. Consider adding a separate API for use cases
    /// that require this.
    pub fn extra_args(&mut self, extra_args: impl Iterator<Item = impl ToString>) -> &mut Self {
        self.extra_args = extra_args.map(|x| x.to_string()).collect::<_>();
        self
    }

    /// Mark a file path as to be signed.
    pub fn sign_file(&mut self, path: impl AsRef<Path>) -> &mut Self {
        self.sign_files.push(path.as_ref().to_path_buf());
        self
    }

    /// Run `signtool sign` with requested options.
    pub fn run(&self, logger: &slog::Logger) -> Result<()> {
        let signtool = find_signtool().context("locating signtool.exe")?;

        let mut args = vec!["sign".to_string()];

        if self.verbose {
            args.push("/v".to_string());
        }

        if self.debug {
            args.push("/debug".to_string());
        }

        match &self.certificate {
            X509SigningCertificate::Auto => {
                args.push("/a".to_string());
            }
            X509SigningCertificate::File(file) => {
                args.push("/f".to_string());
                args.push(file.path().display().to_string());
                if let Some(password) = file.password() {
                    args.push("/p".to_string());
                    args.push(password.to_string());
                }
            }
            X509SigningCertificate::SubjectName(sn) => {
                args.push("/n".to_string());
                args.push(sn.to_string());
            }
        }

        if let Some(description) = &self.description {
            args.push("/d".to_string());
            args.push(description.to_string());
        }

        if let Some(algorithm) = &self.file_digest_algorithm {
            args.push("/fd".to_string());
            args.push(algorithm.to_string());
        }

        if let Some(server) = &self.timestamp_server {
            match server {
                TimestampServer::Simple(url) => {
                    args.push("/t".to_string());
                    args.push(url.to_string());
                }
                TimestampServer::Rfc3161(url, algorithm) => {
                    args.push("/tr".to_string());
                    args.push(url.to_string());
                    args.push("/td".to_string());
                    args.push(algorithm.to_string());
                }
            }
        }

        args.extend(self.extra_args.iter().cloned());

        args.extend(self.sign_files.iter().map(|p| p.display().to_string()));

        let command = duct::cmd(signtool, args)
            .stderr_to_stdout()
            .reader()
            .context("running signtool")?;
        {
            let reader = BufReader::new(&command);
            for line in reader.lines() {
                warn!(logger, "{}", line?);
            }
        }

        let output = command
            .try_wait()?
            .ok_or_else(|| anyhow!("unable to wait on command"))?;
        if output.status.success() {
            Ok(())
        } else {
            Err(anyhow!("error running signtool"))
        }
    }
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        crate::{
            certificate_to_pfx, create_self_signed_code_signing_certificate,
            FileBasedX509SigningCertificate,
        },
        tugger_common::testutil::*,
    };

    #[test]
    fn test_find_signtool() -> Result<()> {
        let res = find_signtool();

        // Rust development environments on Windows should have the Windows SDK available.
        if cfg!(target_family = "windows") {
            res?;
        } else {
            assert!(res.is_err());
        }

        Ok(())
    }

    #[test]
    fn test_sign_executable() -> Result<()> {
        if cfg!(target_family = "unix") {
            eprintln!("skipping test because only works on Windows");
            return Ok(());
        }

        let logger = get_logger()?;
        let temp_path = DEFAULT_TEMP_DIR.path().join("test_sign_executable");
        std::fs::create_dir(&temp_path)?;

        let cert = create_self_signed_code_signing_certificate("tugger@example.com")?;
        let pfx_data = certificate_to_pfx(&cert, "some_password", "cert_name")?;

        let key_path = temp_path.join("signing.pfx");
        std::fs::write(&key_path, &pfx_data)?;

        // We sign the current test executable because why not.
        let sign_path = temp_path.join("test.exe");
        let current_exe = std::env::current_exe()?;
        std::fs::copy(&current_exe, &sign_path)?;

        let mut c = FileBasedX509SigningCertificate::new(&key_path);
        c.set_password("some_password");

        SigntoolSign::new(c.into())
            .verbose()
            .debug()
            .description("tugger test executable")
            .file_digest_algorithm("sha256")
            .sign_file(&sign_path)
            .run(&logger)?;

        Ok(())
    }
}
