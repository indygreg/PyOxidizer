// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    crate::starlark::{
        code_signing::{handle_signable_event, SigningAction, SigningContext},
        file_content::{FileContentValue, FileContentWrapper},
    },
    anyhow::anyhow,
    log::warn,
    simple_file_manifest::FileManifest,
    starlark::{
        environment::TypeValues,
        eval::call_stack::CallStack,
        values::{
            error::{RuntimeError, ValueError},
            none::NoneType,
            {Mutable, TypedValue, Value, ValueResult},
        },
        {
            starlark_fun, starlark_module, starlark_parse_param_type, starlark_signature,
            starlark_signature_extraction, starlark_signatures,
        },
    },
    starlark_dialect_build_targets::{
        get_context_value, optional_str_arg, EnvironmentContext, ResolvedTarget,
        ResolvedTargetValue, RunMode,
    },
    std::{
        path::{Path, PathBuf},
        sync::{Arc, Mutex, MutexGuard},
    },
    tugger_code_signing::SigningDestination,
};

fn error_context<F, T>(label: &str, f: F) -> Result<T, ValueError>
where
    F: FnOnce() -> anyhow::Result<T>,
{
    f().map_err(|e| {
        ValueError::Runtime(RuntimeError {
            code: "SIMPLE_FILE_MANIFEST",
            message: format!("{:?}", e),
            label: label.to_string(),
        })
    })
}

/// Run signing checks after a FileManifest has been materialized.
fn post_materialize_signing_checks(
    label: &'static str,
    type_values: &TypeValues,
    call_stack: &mut CallStack,
    action: SigningAction,
    installed_paths: &[PathBuf],
) -> Result<(), ValueError> {
    for path in installed_paths {
        let filename = path.file_name().ok_or_else(|| {
            ValueError::Runtime(RuntimeError {
                code: "TUGGER_FILE_RESOURCE",
                message: "unable to resolve filename of path (this should never happen)"
                    .to_string(),
                label: label.to_string(),
            })
        })?;

        let candidate = path.as_path().into();
        let mut context = SigningContext::new(label, action, filename, &candidate);
        context.set_path(path);
        context.set_signing_destination(SigningDestination::File(path.clone()));

        handle_signable_event(type_values, call_stack, context)?;
    }

    Ok(())
}

#[derive(Clone, Debug)]
pub struct FileManifestValue {
    inner: Arc<Mutex<FileManifest>>,
    /// Optional path to be the default run target.
    pub run_path: Option<PathBuf>,
}

impl TypedValue for FileManifestValue {
    type Holder = Mutable<FileManifestValue>;
    const TYPE: &'static str = "FileManifest";

    fn values_for_descendant_check_and_freeze(&self) -> Box<dyn Iterator<Item = Value>> {
        Box::new(std::iter::empty())
    }
}

// Starlark functions.
impl FileManifestValue {
    /// FileManifest()
    pub fn new_from_args() -> ValueResult {
        Self::new_from_manifest(FileManifest::default())
    }

    pub fn new_from_manifest(manifest: FileManifest) -> ValueResult {
        Ok(Value::new(Self {
            inner: Arc::new(Mutex::new(manifest)),
            run_path: None,
        }))
    }

    pub fn inner(&self, label: &str) -> Result<MutexGuard<FileManifest>, ValueError> {
        self.inner.try_lock().map_err(|e| {
            ValueError::Runtime(RuntimeError {
                code: "SIMPLE_FILE_MANIFEST",
                message: format!("error obtaining lock: {}", e),
                label: label.to_string(),
            })
        })
    }

    fn build(
        &self,
        type_values: &TypeValues,
        call_stack: &mut CallStack,
        target: String,
    ) -> ValueResult {
        const LABEL: &str = "FileManifest.build()";

        let context_value = get_context_value(type_values)?;
        let context = context_value
            .downcast_ref::<EnvironmentContext>()
            .ok_or(ValueError::IncorrectParameterType)?;

        let output_path = context.target_build_path(&target);

        let inner = self.inner(LABEL)?;

        let installed_paths = error_context(LABEL, || {
            warn!("installing files to {}", output_path.display());
            inner
                .materialize_files_with_replace(&output_path)
                .map_err(anyhow::Error::new)
        })?;

        post_materialize_signing_checks(
            LABEL,
            type_values,
            call_stack,
            SigningAction::FileManifestInstall,
            &installed_paths,
        )?;

        // Use the stored run target if available, falling back to the single
        // executable file if non-ambiguous.
        // TODO support defining default run target in data structure.
        let run_mode = if let Some(default) = &self.run_path {
            RunMode::Path {
                path: output_path.join(default),
            }
        } else {
            let exes = inner
                .iter_entries()
                .filter(|(_, c)| c.is_executable())
                .collect::<Vec<_>>();

            if exes.len() == 1 {
                RunMode::Path {
                    path: output_path.join(exes[0].0),
                }
            } else {
                RunMode::None
            }
        };

        Ok(Value::new(ResolvedTargetValue {
            inner: ResolvedTarget {
                run_mode,
                output_path,
            },
        }))
    }

    /// FileManifest.add_manifest(other)
    pub fn add_manifest(&mut self, other: FileManifestValue) -> ValueResult {
        const LABEL: &str = "FileManifest.add_manifest()";

        let mut inner = self.inner(LABEL)?;
        let other_inner = other.inner(LABEL)?;

        error_context(LABEL, || {
            inner.add_manifest(&other_inner).map_err(anyhow::Error::new)
        })?;

        Ok(Value::new(NoneType::None))
    }

    /// FileManifest.add_file(content, path = None, directory = None)
    pub fn add_file(
        &mut self,
        content: FileContentValue,
        path: Value,
        directory: Value,
    ) -> ValueResult {
        const LABEL: &str = "FileManifest.add_file()";

        let path = optional_str_arg("path", &path)?;
        let directory = optional_str_arg("directory", &directory)?;

        let mut inner = self.inner(LABEL)?;
        let content_inner = content.inner(LABEL)?;

        error_context(LABEL, || {
            if path.is_some() && directory.is_some() {
                return Err(anyhow!(
                    "at most 1 of `path` and `directory` must be specified"
                ));
            }

            let path = if let Some(path) = path {
                PathBuf::from(path)
            } else if let Some(directory) = directory {
                PathBuf::from(directory).join(&content_inner.filename)
            } else {
                PathBuf::from(&content_inner.filename)
            };

            inner.add_file_entry(path, content_inner.content.clone())?;

            Ok(())
        })?;

        Ok(Value::new(NoneType::None))
    }

    /// FileManifest.add_path(path, strip_prefix, force_read=False)
    pub fn add_path(
        &mut self,
        path: String,
        strip_prefix: String,
        force_read: bool,
    ) -> ValueResult {
        const LABEL: &str = "FileManifest.add_path()";

        let mut inner = self.inner(LABEL)?;

        error_context(LABEL, || {
            let path = Path::new(&path);
            let strip_prefix = Path::new(&strip_prefix);

            if force_read {
                inner.add_path_memory(path, strip_prefix)
            } else {
                inner.add_path(path, strip_prefix)
            }
            .map_err(anyhow::Error::new)
        })?;

        Ok(Value::new(NoneType::None))
    }

    /// FileManifest.get_file(path) -> FileContent
    pub fn get_file(&self, path: String) -> ValueResult {
        const LABEL: &str = "FileManifest.get_file()";

        let (path, filename) = error_context(LABEL, || {
            let path = PathBuf::from(path);
            let filename = path
                .file_name()
                .ok_or_else(|| {
                    anyhow!("unable to resolve file name from path: {}", path.display())
                })?
                .to_string_lossy()
                .to_string();

            Ok((path, filename))
        })?;

        let inner = self.inner(LABEL)?;

        if let Some(entry) = inner.get(path) {
            Ok(FileContentWrapper {
                content: entry.clone(),
                filename,
            }
            .into())
        } else {
            Ok(Value::new(NoneType::None))
        }
    }

    /// FileManifest.install(path, replace=true)
    pub fn install(
        &self,
        type_values: &TypeValues,
        call_stack: &mut CallStack,
        path: String,
        replace: bool,
    ) -> ValueResult {
        const LABEL: &str = "FileManifest.install()";

        let raw_context = get_context_value(type_values)?;
        let context = raw_context
            .downcast_ref::<EnvironmentContext>()
            .ok_or(ValueError::IncorrectParameterType)?;

        let inner = self.inner(LABEL)?;

        let installed_paths = error_context(LABEL, || {
            let dest_path = context.build_path().join(path);

            if replace {
                inner.materialize_files_with_replace(&dest_path)
            } else {
                inner.materialize_files(&dest_path)
            }
            .map_err(anyhow::Error::new)
        })?;

        post_materialize_signing_checks(
            LABEL,
            type_values,
            call_stack,
            SigningAction::FileManifestInstall,
            &installed_paths,
        )?;

        Ok(Value::new(NoneType::None))
    }

    pub fn paths(&self) -> ValueResult {
        const LABEL: &str = "FileManifest.paths()";

        let inner = self.inner(LABEL)?;

        let paths = inner
            .iter_entries()
            .map(|(path, _)| Value::from(format!("{}", path.display())))
            .collect::<Vec<_>>();

        Ok(Value::from(paths))
    }

    /// FileManifest.remove(path) -> FileContent
    pub fn remove(&mut self, path: String) -> ValueResult {
        const LABEL: &str = "FileManifest.remove()";

        let (path, filename) = error_context(LABEL, || {
            let path = PathBuf::from(path);
            let filename = path
                .file_name()
                .ok_or_else(|| {
                    anyhow!("unable to resolve file name from path: {}", path.display())
                })?
                .to_string_lossy()
                .to_string();

            Ok((path, filename))
        })?;

        let mut inner = self.inner(LABEL)?;

        if let Some(entry) = inner.remove(path) {
            Ok(FileContentWrapper {
                content: entry,
                filename,
            }
            .into())
        } else {
            Ok(Value::new(NoneType::None))
        }
    }
}

starlark_module! { file_manifest_module =>
    #[allow(non_snake_case)]
    FileManifest(env _env) {
        FileManifestValue::new_from_args()
    }

    FileManifest.add_manifest(this, other: FileManifestValue) {
        let mut this = this.downcast_mut::<FileManifestValue>().unwrap().unwrap();
        this.add_manifest(other)
    }

    FileManifest.add_file(
        this,
        content: FileContentValue,
        path = NoneType::None,
        directory = NoneType::None
    ) {
        let mut this = this.downcast_mut::<FileManifestValue>().unwrap().unwrap();
        this.add_file(content, path, directory)
    }

    FileManifest.add_path(this, path: String, strip_prefix: String, force_read: bool = false) {
        let mut this = this.downcast_mut::<FileManifestValue>().unwrap().unwrap();
        this.add_path(path, strip_prefix, force_read)
    }

    FileManifest.build(env env, call_stack cs, this, target: String) {
        let this = this.downcast_ref::<FileManifestValue>().unwrap();
        this.build(env, cs, target)
    }

    FileManifest.get_file(this, path: String) {
        let this = this.downcast_ref::<FileManifestValue>().unwrap();
        this.get_file(path)
    }

    FileManifest.install(env env, call_stack cs, this, path: String, replace: bool = true) {
        let this = this.downcast_ref::<FileManifestValue>().unwrap();
        this.install(env, cs, path, replace)
    }

    FileManifest.paths(this) {
        let this = this.downcast_ref::<FileManifestValue>().unwrap();
        this.paths()
    }

    FileManifest.remove(this, path: String) {
        let mut this = this.downcast_mut::<FileManifestValue>().unwrap().unwrap();
        this.remove(path)
    }
}

#[cfg(test)]
mod tests {
    use {
        super::*, crate::starlark::testutil::*, anyhow::Result, simple_file_manifest::FileEntry,
        tugger_common::testutil::*,
    };

    #[test]
    fn test_new_file_manifest() {
        let m = starlark_ok("FileManifest()");
        assert_eq!(m.get_type(), "FileManifest");

        let m = m.downcast_ref::<FileManifestValue>().unwrap();
        assert_eq!(m.inner("ignored").unwrap().clone(), FileManifest::default());
    }

    #[test]
    fn test_add_file_manifest() -> Result<()> {
        let mut env = StarlarkEnvironment::new()?;
        env.eval("m1 = FileManifest()")?;
        env.eval("m2 = FileManifest()")?;

        env.eval("m1.add_manifest(m2)")?;

        Ok(())
    }

    #[test]
    fn test_add_path() -> Result<()> {
        let mut env = StarlarkEnvironment::new()?;
        let manifest_value = env.eval("m = FileManifest(); m")?;

        let res = env.eval("m.add_path('/does/not/exist', '/does/not')");
        assert!(res.is_err());

        let temp_file0 = DEFAULT_TEMP_DIR.path().join("test_add_path_0");
        let temp_file1 = DEFAULT_TEMP_DIR.path().join("test_add_path_1");
        std::fs::write(&temp_file0, vec![42])?;
        std::fs::write(&temp_file1, vec![42, 42])?;
        let parent = temp_file0.parent().unwrap();

        env.eval(&format!(
            "m.add_path('{}', '{}')",
            temp_file0.display().to_string().escape_default(),
            parent.display().to_string().escape_default()
        ))?;
        env.eval(&format!(
            "m.add_path('{}', '{}', force_read = True)",
            temp_file1.display().to_string().escape_default(),
            parent.display().to_string().escape_default()
        ))?;

        let manifest = manifest_value.downcast_ref::<FileManifestValue>().unwrap();
        {
            let inner = manifest.inner("ignored").unwrap();
            assert_eq!(inner.iter_files().count(), 2);
            assert_eq!(
                inner.get("test_add_path_0"),
                Some(&FileEntry::new_from_path(temp_file0, false))
            );
            assert_eq!(inner.get("test_add_path_1"), Some(&vec![42, 42].into()),);
        }

        Ok(())
    }

    #[test]
    fn add_file() -> Result<()> {
        let mut env = StarlarkEnvironment::new()?;

        env.eval("c = FileContent(filename = 'file', content = 'foo')")?;
        env.eval("m = FileManifest()")?;
        env.eval("m.add_file(c)")?;

        let raw = env.eval("m")?;
        let manifest = raw.downcast_ref::<FileManifestValue>().unwrap();
        let inner = manifest.inner("ignored").unwrap();

        let entries = inner.iter_entries().collect::<Vec<_>>();
        assert_eq!(
            entries,
            vec![(&PathBuf::from("file"), &b"foo".as_ref().into())]
        );

        Ok(())
    }

    #[test]
    fn add_file_path() -> Result<()> {
        let mut env = StarlarkEnvironment::new()?;

        env.eval("c = FileContent(filename = 'file', content = 'foo')")?;
        env.eval("m = FileManifest()")?;
        env.eval("m.add_file(c, path = 'foo/bar')")?;

        let raw = env.eval("m")?;
        let manifest = raw.downcast_ref::<FileManifestValue>().unwrap();
        let inner = manifest.inner("ignored").unwrap();

        let entries = inner.iter_entries().collect::<Vec<_>>();
        assert_eq!(
            entries,
            vec![(&PathBuf::from("foo/bar"), &b"foo".as_ref().into())]
        );

        Ok(())
    }

    #[test]
    fn add_file_directory() -> Result<()> {
        let mut env = StarlarkEnvironment::new()?;

        env.eval("c = FileContent(filename = 'file', content = 'foo')")?;
        env.eval("m = FileManifest()")?;
        env.eval("m.add_file(c, directory = 'dir')")?;

        let raw = env.eval("m")?;
        let manifest = raw.downcast_ref::<FileManifestValue>().unwrap();
        let inner = manifest.inner("ignored").unwrap();

        let entries = inner.iter_entries().collect::<Vec<_>>();
        assert_eq!(
            entries,
            vec![(&PathBuf::from("dir/file"), &b"foo".as_ref().into())]
        );

        Ok(())
    }

    #[test]
    fn get_file() -> Result<()> {
        let mut env = StarlarkEnvironment::new()?;

        env.eval("m = FileManifest()")?;
        assert_eq!(env.eval("m.get_file('missing')")?.get_type(), "NoneType");

        env.eval("m.add_file(FileContent(filename = 'file', content = 'foo'))")?;
        assert_eq!(
            env.eval("m.get_file('file')")?.get_type(),
            FileContentValue::TYPE
        );

        Ok(())
    }

    #[test]
    fn paths() -> Result<()> {
        let mut env = StarlarkEnvironment::new()?;

        env.eval("m = FileManifest()")?;
        let v = env.eval("m.paths()")?;
        assert_eq!(v.get_type(), "list");
        assert_eq!(v.iter().unwrap().iter().count(), 0);

        env.eval("m.add_file(FileContent(filename = 'file', content = 'foo'))")?;
        let v = env.eval("m.paths()")?;

        let values = v.iter().unwrap().to_vec();
        assert_eq!(values.len(), 1);
        assert_eq!(values[0].to_string(), "file");

        Ok(())
    }

    #[test]
    fn remove() -> Result<()> {
        let mut env = StarlarkEnvironment::new()?;

        env.eval("m = FileManifest()")?;
        env.eval("m.add_file(FileContent(filename = 'file', content = 'foo'))")?;
        assert_eq!(
            env.eval("m.remove('file')")?.get_type(),
            FileContentValue::TYPE
        );
        assert_eq!(env.eval("m.remove('file')")?.get_type(), "NoneType");

        Ok(())
    }
}
