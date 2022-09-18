// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    crate::starlark::file_manifest::FileManifestValue,
    simple_file_manifest::{FileEntry, FileManifest},
    starlark::{
        environment::TypeValues,
        values::{
            error::{RuntimeError, ValueError},
            none::NoneType,
            {Value, ValueResult},
        },
        {
            starlark_fun, starlark_module, starlark_parse_param_type, starlark_signature,
            starlark_signature_extraction, starlark_signatures,
        },
    },
    starlark_dialect_build_targets::{
        get_context_value, optional_list_arg, optional_str_arg, required_list_arg,
        EnvironmentContext,
    },
    std::collections::HashSet,
    tugger_common::glob::evaluate_glob,
};

fn error_context<F, T>(label: &str, f: F) -> Result<T, ValueError>
where
    F: FnOnce() -> anyhow::Result<T>,
{
    f().map_err(|e| {
        ValueError::Runtime(RuntimeError {
            code: "TUGGER_FILE_RESOURCE",
            message: format!("{:?}", e),
            label: label.to_string(),
        })
    })
}

/// glob(include, exclude=None, relative_to=None)
fn starlark_glob(
    type_values: &TypeValues,
    include: &Value,
    exclude: &Value,
    strip_prefix: &Value,
) -> ValueResult {
    required_list_arg("include", "string", include)?;
    optional_list_arg("exclude", "string", exclude)?;
    let strip_prefix = optional_str_arg("strip_prefix", strip_prefix)?;

    let include = include
        .iter()?
        .iter()
        .map(|x| x.to_string())
        .collect::<Vec<String>>();

    let exclude = match exclude.get_type() {
        "list" => exclude.iter()?.iter().map(|x| x.to_string()).collect(),
        _ => Vec::new(),
    };

    let raw_context = get_context_value(type_values)?;
    let context = raw_context
        .downcast_ref::<EnvironmentContext>()
        .ok_or(ValueError::IncorrectParameterType)?;

    let manifest = error_context("glob()", || {
        let mut result = HashSet::new();

        // Evaluate all the includes first.
        for v in include {
            for p in evaluate_glob(context.cwd(), &v)? {
                result.insert(p);
            }
        }

        // Then apply excludes.
        for v in exclude {
            for p in evaluate_glob(context.cwd(), &v)? {
                result.remove(&p);
            }
        }

        let mut manifest = FileManifest::default();

        for path in result {
            let content = FileEntry::try_from(path.as_path())?;

            let path = if let Some(prefix) = &strip_prefix {
                path.strip_prefix(prefix)?.to_path_buf()
            } else {
                path.to_path_buf()
            };

            manifest.add_file_entry(&path, content)?;
        }

        Ok(manifest)
    })?;

    FileManifestValue::new_from_manifest(manifest)
}

starlark_module! { file_resource_module =>
    glob(env env, include, exclude=NoneType::None, strip_prefix=NoneType::None) {
        starlark_glob(env, &include, &exclude, &strip_prefix)
    }

}
