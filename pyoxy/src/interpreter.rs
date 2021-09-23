// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    anyhow::{Context, Result},
    pyembed::{MainPythonInterpreter, OxidizedPythonInterpreterConfig},
    serde::{Deserialize, Serialize},
    std::{
        ffi::{OsStr, OsString},
        ops::{Deref, DerefMut},
        path::Path,
    },
};

/// An embedded Python interpreter configuration.
#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(default, transparent)]
pub struct Config<'a> {
    inner: OxidizedPythonInterpreterConfig<'a>,
}

impl<'a> Deref for Config<'a> {
    type Target = OxidizedPythonInterpreterConfig<'a>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<'a> DerefMut for Config<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl<'a> From<Config<'a>> for OxidizedPythonInterpreterConfig<'a> {
    fn from(c: Config<'a>) -> Self {
        c.inner
    }
}

/// Runs an embedded Python interpreter in `python` mode.
pub fn run_python<T>(exe: &Path, args: &[T]) -> Result<i32>
where
    T: Into<OsString> + AsRef<OsStr>,
{
    let mut config = Config::default();
    config.set_missing_path_configuration = false;
    config.exe = Some(exe.to_path_buf());
    config.argv = Some(
        vec![exe.as_os_str().to_os_string()]
            .into_iter()
            .chain(args.iter().map(|x| x.into()))
            .collect::<Vec<_>>(),
    );

    let interp =
        MainPythonInterpreter::new(config.into()).context("initializing Python interpreter")?;
    Ok(interp.run())
}
