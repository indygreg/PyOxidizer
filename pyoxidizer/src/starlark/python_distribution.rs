// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use itertools::Itertools;
use slog::warn;
use starlark::environment::Environment;
use starlark::values::{
    default_compare, RuntimeError, TypedValue, Value, ValueError, ValueResult,
    INCORRECT_PARAMETER_TYPE_ERROR_CODE,
};
use starlark::{
    any, immutable, not_supported, starlark_fun, starlark_module, starlark_signature,
    starlark_signature_extraction, starlark_signatures,
};
use std::any::Any;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::convert::TryFrom;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use super::env::{
    optional_dict_arg, optional_list_arg, optional_str_arg, required_list_arg, required_str_arg,
};
use super::python_resource::{PythonExtensionModule, PythonResourceData, PythonSourceModule};
use crate::app_packaging::environment::EnvironmentContext;
use crate::py_packaging::distribution::{
    resolve_parsed_distribution, resolve_python_paths, ExtensionModuleFilter,
    ParsedPythonDistribution, PythonDistributionLocation,
};
use crate::py_packaging::distutils::{prepare_hacked_distutils, read_built_extensions};
use crate::py_packaging::fsscan::{find_python_resources, PythonFileResource};
use crate::py_packaging::pip::pip_install as raw_pip_install;
use crate::py_packaging::resource::PythonResource;
use crate::python_distributions::CPYTHON_BY_TRIPLE;

#[derive(Debug, Clone)]
pub struct PythonDistribution {
    pub source: PythonDistributionLocation,

    dest_dir: PathBuf,

    pub distribution: Option<ParsedPythonDistribution>,
}

impl PythonDistribution {
    fn from_location(location: PythonDistributionLocation, dest_dir: &Path) -> PythonDistribution {
        PythonDistribution {
            source: location,
            dest_dir: dest_dir.to_path_buf(),
            distribution: None,
        }
    }

    pub fn ensure_distribution_resolved(&mut self, logger: &slog::Logger) {
        if self.distribution.is_some() {
            return;
        }

        let dist = resolve_parsed_distribution(logger, &self.source, &self.dest_dir).unwrap();
        warn!(logger, "distribution info: {:#?}", dist.as_minimal_info());

        self.distribution = Some(dist);
    }
}

impl TypedValue for PythonDistribution {
    immutable!();
    any!();
    not_supported!(binop);
    not_supported!(container);
    not_supported!(function);
    not_supported!(get_hash);
    not_supported!(to_int);

    fn to_str(&self) -> String {
        format!("PythonDistribution<{:#?}>", self.source)
    }

    fn to_repr(&self) -> String {
        self.to_str()
    }

    fn get_type(&self) -> &'static str {
        "PythonDistribution"
    }

    fn to_bool(&self) -> bool {
        true
    }

    fn compare(&self, other: &dyn TypedValue, _recursion: u32) -> Result<Ordering, ValueError> {
        default_compare(self, other)
    }
}

fn find_resources(path: &Path, state_dir: Option<&Path>) -> Result<Vec<PythonResource>, String> {
    let mut res = Vec::new();

    for r in find_python_resources(&path) {
        match r {
            PythonFileResource::Source { .. } => {
                res.push(PythonResource::try_from(&r).or_else(|e| Err(e.to_string()))?);
            }

            PythonFileResource::Resource(..) => {
                res.push(PythonResource::try_from(&r).or_else(|e| Err(e.to_string()))?);
            }

            _ => {}
        }
    }

    if let Some(p) = state_dir {
        for ext in read_built_extensions(&p).or_else(|e| Err(e.to_string()))? {
            res.push(PythonResource::BuiltExtensionModule(ext));
        }
    }

    Ok(res)
}

starlark_module! { python_distribution_module =>
    #[allow(non_snake_case, clippy::ptr_arg)]
    PythonDistribution(env env, sha256, local_path=None, url=None) {
        required_str_arg("sha256", &sha256)?;
        optional_str_arg("local_path", &local_path)?;
        optional_str_arg("url", &url)?;

        if local_path.get_type() != "NoneType" && url.get_type() != "NoneType" {
            return Err(RuntimeError {
                code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
                message: "cannot define both local_path and url".to_string(),
                label: "cannot define both local_path and url".to_string(),
            }.into());
        }

        let distribution = if local_path.get_type() != "NoneType" {
            PythonDistributionLocation::Local {
                local_path: local_path.to_string(),
                sha256: sha256.to_string(),
            }
        } else {
            PythonDistributionLocation::Url {
                url: url.to_string(),
                sha256: sha256.to_string(),
            }
        };

        let context = env.get("CONTEXT").expect("CONTEXT not defined");
        let dest_dir = context.downcast_apply(|x: &EnvironmentContext| x.python_distributions_path.clone());

        Ok(Value::new(PythonDistribution::from_location(distribution, &dest_dir)))
    }

    #[allow(clippy::ptr_arg)]
    PythonDistribution.extension_modules(env env, this, filter="all") {
        let filter = required_str_arg("filter", &filter)?;

        let filter = match filter.as_str() {
            "minimal" => ExtensionModuleFilter::Minimal,
            "all" => ExtensionModuleFilter::All,
            "no-libraries" => ExtensionModuleFilter::NoLibraries,
            "no-gpl" => ExtensionModuleFilter::NoGPL,
            _ => return Err(RuntimeError {
                code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
                message: "policy must be one of {minimal, all, no-libraries, no-gpl}".to_string(),
                label: "invalid policy value".to_string(),
            }.into())
        };

        let context = env.get("CONTEXT").expect("CONTEXT not defined");

        let logger = context.downcast_apply(|x: &EnvironmentContext| x.logger.clone());

        Ok(Value::from(this.downcast_apply_mut(|dist: &mut PythonDistribution| {
            dist.ensure_distribution_resolved(&logger);

            dist.distribution.as_ref().unwrap().filter_extension_modules(&logger, &filter).iter().map(|em| {
                Value::new(PythonExtensionModule { em: em.clone() })
            }).collect_vec()
        })))
    }

    #[allow(clippy::ptr_arg)]
    PythonDistribution.source_modules(env env, this) {
        let context = env.get("CONTEXT").expect("CONTEXT not defined");

        let logger = context.downcast_apply(|x: &EnvironmentContext| x.logger.clone());

        Ok(Value::from(this.downcast_apply_mut(|dist: &mut PythonDistribution| {
            dist.ensure_distribution_resolved(&logger);

            dist.distribution.as_ref().unwrap().source_modules().iter().map(|module| {
                Value::new(PythonSourceModule { module: module.clone() })
            }).collect_vec()
        })))
    }

    #[allow(clippy::ptr_arg)]
    PythonDistribution.resources_data(env env, this) {
        let context = env.get("CONTEXT").expect("CONTEXT not defined");

        let logger = context.downcast_apply(|x: &EnvironmentContext| x.logger.clone());

        Ok(Value::from(this.downcast_apply_mut(|dist: &mut PythonDistribution| {
            dist.ensure_distribution_resolved(&logger);

            dist.distribution.as_ref().unwrap().resources_data().iter().map(|data| {
                Value::new(PythonResourceData { data: data.clone() })
            }).collect_vec()
        })))
    }

    #[allow(clippy::ptr_arg)]
    PythonDistribution.pip_install(env env, this, args, extra_envs=None) {
        required_list_arg("args", "string", &args)?;
        optional_dict_arg("extra_envs", "string", "string", &extra_envs)?;

        let args: Vec<String> = args.into_iter()?.map(|x| x.to_string()).collect();

        let extra_envs = match extra_envs.get_type() {
            "dict" => extra_envs.into_iter()?.map(|key| {
                let k = key.to_string();
                let v = extra_envs.at(key.clone()).unwrap().to_string();
                (k, v)
            }).collect(),
            "NoneType" => HashMap::new(),
            _ => panic!("should have validated type above"),
        };

        let context = env.get("CONTEXT").expect("CONTEXT not defined");
        let logger = context.downcast_apply(|x: &EnvironmentContext| x.logger.clone());

        let resources = this.downcast_apply_mut(|dist: &mut PythonDistribution| {
            dist.ensure_distribution_resolved(&logger);

            let dist = dist.distribution.as_ref().unwrap();
            // TODO get verbose flag from context.
            raw_pip_install(&logger, &dist, false, &args, &extra_envs)
        }).or_else(|e| Err(
            RuntimeError {
                code: "PIP_INSTALL_ERROR",
                message: format!("error running pip install: {}", e),
                label: "pip_install()".to_string(),
            }.into()
        ))?;

        Ok(Value::from(resources.iter().map(Value::from).collect::<Vec<Value>>()))
    }

    #[allow(clippy::ptr_arg)]
    PythonDistribution.read_package_root(
        env env,
        this,
        path,
        packages
    ) {
        let path = required_str_arg("path", &path)?;
        required_list_arg("packages", "string", &packages)?;

        let packages = packages.into_iter()?.map(|x| x.to_string()).collect::<Vec<String>>();

        let context = env.get("CONTEXT").expect("CONTEXT not defined");
        let logger = context.downcast_apply(|x: &EnvironmentContext| x.logger.clone());

        let resources: Vec<PythonResource> = this.downcast_apply_mut(|dist: &mut PythonDistribution| -> Result<Vec<PythonResource>, ValueError> {
            dist.ensure_distribution_resolved(&logger);

            find_resources(&Path::new(&path), None).or_else(|e| Err(
                RuntimeError {
                    code: "PACKAGE_ROOT_ERROR",
                    message: format!("could not find resources: {}", e),
                    label: "read_package_root()".to_string(),
                }.into()
            ))
        })?;

        Ok(
            Value::from(
                resources
                    .iter()
                    .filter(|x| x.is_in_packages(&packages))
                    .map(Value::from)
                    .collect::<Vec<Value>>()))
    }

    #[allow(clippy::ptr_arg)]
    PythonDistribution.read_virtualenv(
        env env,
        this,
        path
    ) {
        let path = required_str_arg("path", &path)?;

        let context = env.get("CONTEXT").expect("CONTEXT not defined");
        let logger = context.downcast_apply(|x: &EnvironmentContext| x.logger.clone());

        let resources = this.downcast_apply_mut(|dist: &mut PythonDistribution| -> Result<Vec<PythonResource>, ValueError> {
            dist.ensure_distribution_resolved(&logger);

            let dist = dist.distribution.as_ref().unwrap();

            let python_paths = resolve_python_paths(&Path::new(&path), &dist.version);

            find_resources(&python_paths.site_packages, None).or_else(|e| Err(
                RuntimeError {
                    code: "VIRTUALENV_ERROR",
                    message: format!("could not find resources: {}", e),
                    label: "read_virtualenv()".to_string(),
                }.into()
            ))
        })?;

        Ok(Value::from(resources.iter().map(Value::from).collect::<Vec<Value>>()))
    }

    #[allow(clippy::ptr_arg)]
    PythonDistribution.setup_py_install(
        env env,
        this,
        package_path,
        extra_envs=None,
        extra_global_arguments=None
    ) {
        let package_path = required_str_arg("package_path", &package_path)?;
        optional_dict_arg("extra_envs", "string", "string", &extra_envs)?;
        optional_list_arg("extra_global_arguments", "string", &extra_global_arguments)?;

        let extra_envs = match extra_envs.get_type() {
            "dict" => extra_envs.into_iter()?.map(|key| {
                let k = key.to_string();
                let v = extra_envs.at(key.clone()).unwrap().to_string();
                (k, v)
            }).collect(),
            "NoneType" => HashMap::new(),
            _ => panic!("should have validated type above"),
        };
        let extra_global_arguments = match extra_global_arguments.get_type() {
            "list" => extra_global_arguments.into_iter()?.map(|x| x.to_string()).collect(),
            "NoneType" => Vec::new(),
            _ => panic!("should have validated type above"),
        };

        let package_path = PathBuf::from(package_path);

        let context = env.get("CONTEXT").expect("CONTEXT not defined");
        let cwd = env.get("CWD").expect("CWD not defined").to_string();
        let logger = context.downcast_apply(|x: &EnvironmentContext| x.logger.clone());

        // TODO most of the logic in this function should ideally be contained within
        // a library function, outside the context of Starlark.

        let exec_cwd = if package_path.is_absolute() {
            package_path
        } else {
            PathBuf::from(cwd).join(package_path)
        };

        let temp_dir = tempdir::TempDir::new("pyoxidizer-setup-py-install").or_else(|e| Err(
            RuntimeError {
                code: "SETUP_PY_ERROR",
                message: format!("error creating temporary directory: {}", e),
                label: "setup_py_install()".to_string(),
            }.into()
        ))?;

        let target_dir_path = temp_dir.path().join("install");
        let target_dir_s = target_dir_path.display().to_string();

        let resources = this.downcast_apply_mut(|dist: &mut PythonDistribution| -> Result<Vec<PythonResource>, ValueError> {
            dist.ensure_distribution_resolved(&logger);

            let dist = dist.distribution.as_ref().unwrap();

            let python_paths = resolve_python_paths(&target_dir_path, &dist.version);

            std::fs::create_dir_all(&python_paths.site_packages).or_else(|e| Err(
                RuntimeError {
                    code: "SETUP_PY_ERROR",
                    message: format!("error creating directory: {}", e),
                    label: "setup_py_install()".to_string(),
                }.into()
            ))?;

            let mut envs = prepare_hacked_distutils(
                &logger,
                dist,
                temp_dir.path(),
                &[&python_paths.site_packages, &python_paths.stdlib],
            )
            .expect("unable to hack distutils");

            for (key, value) in &extra_envs {
                envs.insert(key.clone(), value.clone());
            }

            warn!(logger, "python setup.py installing to {}", target_dir_s);

            let mut args = vec!["setup.py"];

            for arg in &extra_global_arguments {
                args.push(arg);
            }

            // TODO add --verbose based on context flag

            args.extend(&["install", "--prefix", &target_dir_s, "--no-compile"]);

            // TODO send stderr to stdout.
            let mut cmd = std::process::Command::new(&dist.python_exe)
                .current_dir(&exec_cwd)
                .args(&args)
                .envs(&envs)
                .stdout(std::process::Stdio::piped())
                .spawn()
                .expect("error running setup.py");
            {
                let stdout = cmd.stdout.as_mut().unwrap();
                let reader = BufReader::new(stdout);

                for line in reader.lines() {
                    warn!(logger, "{}", line.unwrap());
                }
            }

            let status = cmd.wait().unwrap();
            if !status.success() {
                panic!("error running setup.py");
            }

            let state_dir = PathBuf::from(envs.get("PYOXIDIZER_DISTUTILS_STATE_DIR").unwrap());
            find_resources(&python_paths.site_packages, Some(&state_dir)).or_else(|e| Err(
                RuntimeError {
                    code: "SETUP_PY_ERROR",
                    message: format!("could not find resources: {}", e),
                    label: "setup_py_install()".to_string(),
                }.into()
            ))
        })?;

        Ok(Value::from(resources.iter().map(Value::from).collect::<Vec<Value>>()))
    }

    #[allow(clippy::ptr_arg)]
    default_python_distribution(env env, build_target=None) {
        let build_target = match build_target.get_type() {
            "NoneType" => env.get("BUILD_TARGET").unwrap().to_string(),
            "string" => build_target.to_string(),
            t => {
                return Err(ValueError::TypeNotX {
                    object_type: t.to_string(),
                    op: "str".to_string(),
                })
            }
        };

        match CPYTHON_BY_TRIPLE.get(&build_target) {
            Some(dist) => {
                let distribution = PythonDistributionLocation::Url {
                    url: dist.url.clone(),
                    sha256: dist.sha256.clone(),
                };

                let context = env.get("CONTEXT").expect("CONTEXT not defined");
                let dest_dir = context.downcast_apply(|x: &EnvironmentContext| x.python_distributions_path.clone());

                Ok(Value::new(PythonDistribution::from_location(distribution, &dest_dir)))
            }
            None => Err(ValueError::Runtime(RuntimeError {
                code: "no_default_distribution",
                message: format!("could not find default Python distribution for {}", build_target),
                label: "build_target".to_string(),
            }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::testutil::*;
    use super::*;

    #[test]
    fn test_default_python_distribution() {
        let dist = starlark_ok("default_python_distribution()");
        assert_eq!(dist.get_type(), "PythonDistribution");

        let host_distribution = CPYTHON_BY_TRIPLE
            .get(crate::app_packaging::repackage::HOST)
            .unwrap();

        let wanted = PythonDistributionLocation::Url {
            url: host_distribution.url.clone(),
            sha256: host_distribution.sha256.clone(),
        };

        dist.downcast_apply(|x: &PythonDistribution| assert_eq!(x.source, wanted));
    }

    #[test]
    fn test_default_python_distribution_bad_arg() {
        let err = starlark_nok("default_python_distribution(False)");
        assert_eq!(err.message, "The type 'bool' is not str");
    }

    #[test]
    fn test_python_distribution_no_args() {
        let err = starlark_nok("PythonDistribution()");
        assert!(err.message.starts_with("Missing parameter sha256"));
    }

    #[test]
    fn test_python_distribution_multiple_args() {
        let err = starlark_nok(
            "PythonDistribution('sha256', url='url_value', local_path='local_path_value')",
        );
        assert_eq!(err.message, "cannot define both local_path and url");
    }

    #[test]
    fn test_python_distribution_url() {
        let dist = starlark_ok("PythonDistribution('sha256', url='some_url')");
        let wanted = PythonDistributionLocation::Url {
            url: "some_url".to_string(),
            sha256: "sha256".to_string(),
        };

        dist.downcast_apply(|x: &PythonDistribution| assert_eq!(x.source, wanted));
    }

    #[test]
    fn test_python_distribution_local_path() {
        let dist = starlark_ok("PythonDistribution('sha256', local_path='some_path')");
        let wanted = PythonDistributionLocation::Local {
            local_path: "some_path".to_string(),
            sha256: "sha256".to_string(),
        };

        dist.downcast_apply(|x: &PythonDistribution| assert_eq!(x.source, wanted));
    }

    #[test]
    fn test_source_modules() {
        let mods = starlark_ok("default_python_distribution().source_modules()");
        assert_eq!(mods.get_type(), "list");
    }

    #[test]
    fn test_pip_install_simple() {
        let resources =
            starlark_ok("default_python_distribution().pip_install(['pyflakes==2.1.1'])");
        assert_eq!(resources.get_type(), "list");

        let mut it = resources.into_iter().unwrap();

        let v = it.next().unwrap();
        assert_eq!(v.get_type(), "PythonSourceModule");
        v.downcast_apply(|x: &PythonSourceModule| {
            assert_eq!(x.module.name, "pyflakes");
            assert!(x.module.is_package);
        });
    }
}
