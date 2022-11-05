// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Functionality for running from a YAML file. */

use {
    crate::interpreter::Config,
    anyhow::{anyhow, Context, Result},
    pyembed::MainPythonInterpreter,
    std::{
        ffi::{OsStr, OsString},
        fs::File,
        io::{BufRead, BufReader, Read},
        path::Path,
    },
};

/// Run with YAML content provided by a string.
///
/// The YAML will be parsed to an [OxidizedPythonInterpreterConfig]. Unless
/// the fields `exe` or `argv` are set, the provided values will be used.
///
/// A [MainPythonInterpreter] will be spawned from the [OxidizedPythonInterpreterConfig].
/// It will then run whatever it is configured to run and finalize. The function
/// returns an exit code.
///
/// If the interpreter raises a Python exception, this will be handled by
/// Python and it will not materialize as an `Err`.
pub fn run_yaml_str<T>(yaml: &str, exe: &Path, args: &[T]) -> Result<i32>
where
    T: Into<OsString> + AsRef<OsStr>,
{
    let mut config: Config =
        serde_yaml::from_str(yaml).context("parsing YAML to data structure")?;

    config.apply_environment();

    if config.exe.is_none() {
        config.exe = Some(exe.to_path_buf());
    }

    if config.argv.is_none() {
        // argv[0] is the program name.
        config.argv = Some(
            std::iter::once(exe.as_os_str().to_os_string())
                .chain(args.iter().map(|x| x.into()))
                .collect::<Vec<OsString>>(),
        );
    }

    let interp =
        MainPythonInterpreter::new(config.into()).context("initializing Python interpreter")?;
    Ok(interp.run())
}

enum ParserState {
    NoDocumentStart,
    InDocument,
    Finished,
}

/// Run from a reader that contains YAML.
///
/// The reader will ignore all content up to a line beginning with `---`.
///
/// The path of the executable and additional arguments to it are also passed.
/// These will be defined as `sys.argv` unless the read config overwrites
/// the parameters.
pub fn run_yaml_reader<T>(reader: impl Read, exe_path: &Path, args: &[T]) -> Result<i32>
where
    T: Into<OsString> + AsRef<OsStr>,
{
    let reader = BufReader::new(reader);

    let mut yaml_lines = vec![];
    let mut state = ParserState::NoDocumentStart;

    for line in reader.lines() {
        let line = line?;

        match state {
            ParserState::NoDocumentStart => {
                if line.starts_with("---") {
                    state = ParserState::InDocument;
                    yaml_lines.push(line);
                }
            }
            ParserState::InDocument => {
                if line.starts_with("...") {
                    state = ParserState::Finished;
                }

                yaml_lines.push(line);

                if matches!(state, ParserState::Finished) {
                    break;
                }
            }
            ParserState::Finished => {
                panic!("should not get here");
            }
        }
    }

    match state {
        ParserState::NoDocumentStart => {
            return Err(anyhow!(
                "failed to locate YAML document; does it have a line beginning with '---'?"
            ));
        }
        ParserState::InDocument | ParserState::Finished => {}
    }

    run_yaml_str(&yaml_lines.join("\n"), exe_path, args)
}

/// Run a YAML file.
pub fn run_yaml_path<T>(yaml_path: &Path, args: &[T]) -> Result<i32>
where
    T: Into<OsString> + AsRef<OsStr>,
{
    run_yaml_reader(File::open(yaml_path)?, yaml_path, args)
}
