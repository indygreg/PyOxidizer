// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*!
Scanning the filesystem for Python resources.
*/

use {
    crate::{
        module_util::{is_package_from_path, PythonModuleSuffixes},
        package_metadata::PythonPackageMetadata,
        resource::{
            BytecodeOptimizationLevel, PythonEggFile, PythonExtensionModule, PythonModuleBytecode,
            PythonModuleSource, PythonPackageDistributionResource,
            PythonPackageDistributionResourceFlavor, PythonPackageResource, PythonPathExtension,
            PythonResource,
        },
    },
    anyhow::Result,
    std::{
        collections::HashSet,
        ffi::OsStr,
        path::{Path, PathBuf},
    },
    tugger_file_manifest::{File, FileData, FileEntry, FileManifest},
};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

#[cfg(unix)]
fn is_executable(metadata: &std::fs::Metadata) -> bool {
    let permissions = metadata.permissions();
    permissions.mode() & 0o111 != 0
}

#[cfg(windows)]
fn is_executable(_metadata: &std::fs::Metadata) -> bool {
    false
}

pub fn walk_tree_files(path: &Path) -> Box<dyn Iterator<Item = walkdir::DirEntry>> {
    let res = walkdir::WalkDir::new(path).sort_by(|a, b| a.file_name().cmp(b.file_name()));

    let filtered = res.into_iter().filter_map(|entry| {
        let entry = entry.expect("unable to get directory entry");

        let path = entry.path();

        if path.is_dir() {
            None
        } else {
            Some(entry)
        }
    });

    Box::new(filtered)
}

#[derive(Debug, PartialEq)]
struct ResourceFile {
    /// Filesystem path of this resource.
    pub full_path: PathBuf,

    /// Relative path of this resource.
    pub relative_path: PathBuf,
}

#[derive(Debug, PartialEq)]
enum PathItem<'a> {
    PythonResource(PythonResource<'a>),
    ResourceFile(ResourceFile),
}

#[derive(Debug, PartialEq)]
struct PathEntry {
    path: PathBuf,
    /// Whether we emitted a `PythonResource::File` instance.
    file_emitted: bool,
    /// Whether we emitted a non-`PythonResource::File` instance.
    non_file_emitted: bool,
}

/// An iterator of `PythonResource`.
pub struct PythonResourceIterator<'a> {
    root_path: PathBuf,
    cache_tag: String,
    suffixes: PythonModuleSuffixes,
    paths: Vec<PathEntry>,
    /// Content overrides for individual paths.
    ///
    /// This is a hacky way to allow us to abstract I/O.
    path_content_overrides: FileManifest,
    seen_packages: HashSet<String>,
    resources: Vec<ResourceFile>,
    // Whether to emit `PythonResource::File` entries.
    emit_files: bool,
    // Whether to emit non-`PythonResource::File` entries.
    emit_non_files: bool,
    _phantom: std::marker::PhantomData<&'a ()>,
}

impl<'a> PythonResourceIterator<'a> {
    fn new(
        path: &Path,
        cache_tag: &str,
        suffixes: &PythonModuleSuffixes,
        emit_files: bool,
        emit_non_files: bool,
    ) -> PythonResourceIterator<'a> {
        let res = walkdir::WalkDir::new(path).sort_by(|a, b| a.file_name().cmp(b.file_name()));

        let filtered = res
            .into_iter()
            .filter_map(|entry| {
                let entry = entry.expect("unable to get directory entry");

                let path = entry.path();

                if path.is_dir() {
                    None
                } else {
                    Some(PathEntry {
                        path: path.to_path_buf(),
                        file_emitted: false,
                        non_file_emitted: false,
                    })
                }
            })
            .collect::<Vec<_>>();

        PythonResourceIterator {
            root_path: path.to_path_buf(),
            cache_tag: cache_tag.to_string(),
            suffixes: suffixes.clone(),
            paths: filtered,
            path_content_overrides: FileManifest::default(),
            seen_packages: HashSet::new(),
            resources: Vec::new(),
            emit_files,
            emit_non_files,
            _phantom: std::marker::PhantomData,
        }
    }

    /// Construct an instance from an iterable of `(File)`.
    pub fn from_data_locations(
        resources: &[File],
        cache_tag: &str,
        suffixes: &PythonModuleSuffixes,
        emit_files: bool,
        emit_non_files: bool,
    ) -> Result<PythonResourceIterator<'a>> {
        let mut paths = resources
            .iter()
            .map(|file| PathEntry {
                path: file.path.clone(),
                file_emitted: false,
                non_file_emitted: false,
            })
            .collect::<Vec<_>>();
        paths.sort_by(|a, b| a.path.cmp(&b.path));

        let mut path_content_overrides = FileManifest::default();
        for resource in resources {
            path_content_overrides.add_file_entry(&resource.path, resource.entry.clone())?;
        }

        Ok(PythonResourceIterator {
            root_path: PathBuf::new(),
            cache_tag: cache_tag.to_string(),
            suffixes: suffixes.clone(),
            paths,
            path_content_overrides,
            seen_packages: HashSet::new(),
            resources: Vec::new(),
            emit_files,
            emit_non_files,
            _phantom: std::marker::PhantomData,
        })
    }

    fn resolve_is_executable(&self, path: &Path) -> bool {
        match self.path_content_overrides.get(path) {
            Some(file) => file.executable,
            None => {
                if let Ok(metadata) = path.metadata() {
                    is_executable(&metadata)
                } else {
                    false
                }
            }
        }
    }

    fn resolve_file_data(&self, path: &Path) -> FileData {
        match self.path_content_overrides.get(path) {
            Some(file) => file.data.clone(),
            None => FileData::Path(path.to_path_buf()),
        }
    }

    fn resolve_path(&mut self, path: &Path) -> Option<PathItem<'a>> {
        let mut rel_path = path
            .strip_prefix(&self.root_path)
            .expect("unable to strip path prefix");
        let mut rel_str = rel_path.to_str().expect("could not convert path to str");
        let mut components = rel_path
            .iter()
            .map(|p| p.to_str().expect("unable to get path as str"))
            .collect::<Vec<_>>();

        // Files in .dist-info and .egg-info directories are distribution metadata files.
        // Parsing the package name out of the directory name can be a bit wonky, as
        // case sensitivity and other normalization can come into play. So our strategy
        // is to parse the well-known metadata record inside the directory to extract
        // the package info. If the file doesn't exist or can't be parsed, we ignore this
        // distribution entirely.

        let distribution_info = if components[0].ends_with(".dist-info") {
            Some((
                self.root_path.join(components[0]).join("METADATA"),
                PythonPackageDistributionResourceFlavor::DistInfo,
            ))
        } else if components[0].ends_with(".egg-info") {
            Some((
                self.root_path.join(components[0]).join("PKG-INFO"),
                PythonPackageDistributionResourceFlavor::EggInfo,
            ))
        } else {
            None
        };

        if let Some((metadata_path, location)) = distribution_info {
            let data = if let Some(file) = self.path_content_overrides.get(&metadata_path) {
                if let Ok(data) = file.data.resolve() {
                    data
                } else {
                    return None;
                }
            } else if let Ok(data) = std::fs::read(&metadata_path) {
                data
            } else {
                return None;
            };

            let metadata = if let Ok(metadata) = PythonPackageMetadata::from_metadata(&data) {
                metadata
            } else {
                return None;
            };

            let package = metadata.name()?;
            let version = metadata.version()?;

            // Name of resource is file path after the initial directory.
            let name = components[1..components.len()].join("/");

            return Some(PathItem::PythonResource(
                PythonPackageDistributionResource {
                    location,
                    package: package.to_string(),
                    version: version.to_string(),
                    name,
                    data: self.resolve_file_data(path),
                }
                .into(),
            ));
        }

        // site-packages directories are package roots within package roots. Treat them as
        // such.
        let in_site_packages = if components[0] == "site-packages" {
            let sp_path = self.root_path.join("site-packages");
            rel_path = path
                .strip_prefix(sp_path)
                .expect("unable to strip site-packages prefix");

            rel_str = rel_path.to_str().expect("could not convert path to str");
            components = rel_path
                .iter()
                .map(|p| p.to_str().expect("unable to get path as str"))
                .collect::<Vec<_>>();

            true
        } else {
            false
        };

        // It looks like we're in an unpacked egg. This is similar to the site-packages
        // scenario: we essentially have a new package root that corresponds to the
        // egg's extraction directory.
        if (&components[0..components.len() - 1])
            .iter()
            .any(|p| p.ends_with(".egg"))
        {
            let mut egg_root_path = self.root_path.clone();

            if in_site_packages {
                egg_root_path = egg_root_path.join("site-packages");
            }

            for p in &components[0..components.len() - 1] {
                egg_root_path = egg_root_path.join(p);

                if p.ends_with(".egg") {
                    break;
                }
            }

            rel_path = path
                .strip_prefix(egg_root_path)
                .expect("unable to strip egg prefix");
            components = rel_path
                .iter()
                .map(|p| p.to_str().expect("unable to get path as str"))
                .collect::<Vec<_>>();

            // Ignore EGG-INFO directory, as it is just packaging metadata.
            if components[0] == "EGG-INFO" {
                return None;
            }
        }

        let file_name = rel_path.file_name().unwrap().to_string_lossy();

        for ext_suffix in &self.suffixes.extension {
            if file_name.ends_with(ext_suffix) {
                let package_parts = &components[0..components.len() - 1];
                let mut package = itertools::join(package_parts, ".");

                let module_name = &file_name[0..file_name.len() - ext_suffix.len()];

                let mut full_module_name: Vec<&str> = package_parts.to_vec();

                if module_name != "__init__" {
                    full_module_name.push(module_name);
                }

                let full_module_name = itertools::join(full_module_name, ".");

                if package.is_empty() {
                    package = full_module_name.clone();
                }

                self.seen_packages.insert(package);

                let module_components = full_module_name.split('.').collect::<Vec<_>>();
                let final_name = module_components[module_components.len() - 1];
                let init_fn = Some(format!("PyInit_{}", final_name));

                return Some(PathItem::PythonResource(
                    PythonExtensionModule {
                        name: full_module_name,
                        init_fn,
                        extension_file_suffix: ext_suffix.clone(),
                        shared_library: Some(self.resolve_file_data(path)),
                        object_file_data: vec![],
                        is_package: is_package_from_path(path),
                        link_libraries: vec![],
                        is_stdlib: false,
                        builtin_default: false,
                        required: false,
                        variant: None,
                        license: None,
                    }
                    .into(),
                ));
            }
        }

        // File extension matches a registered source suffix.
        if self
            .suffixes
            .source
            .iter()
            .any(|ext| rel_str.ends_with(ext))
        {
            let package_parts = &components[0..components.len() - 1];
            let mut package = itertools::join(package_parts, ".");

            let module_name = rel_path
                .file_stem()
                .expect("unable to get file stem")
                .to_str()
                .expect("unable to convert path to str");

            let mut full_module_name: Vec<&str> = package_parts.to_vec();

            if module_name != "__init__" {
                full_module_name.push(module_name);
            }

            let full_module_name = itertools::join(full_module_name, ".");

            if package.is_empty() {
                package = full_module_name.clone();
            }

            self.seen_packages.insert(package);

            return Some(PathItem::PythonResource(
                PythonModuleSource {
                    name: full_module_name,
                    source: self.resolve_file_data(path),
                    is_package: is_package_from_path(&path),
                    cache_tag: self.cache_tag.clone(),
                    is_stdlib: false,
                    is_test: false,
                }
                .into(),
            ));
        }

        if self
            .suffixes
            .bytecode
            .iter()
            .any(|ext| rel_str.ends_with(ext))
        {
            // .pyc files should be in a __pycache__ directory.
            if components.len() < 2 {
                return None;
            }

            // Possibly from Python 2?
            if components[components.len() - 2] != "__pycache__" {
                return None;
            }

            let package_parts = &components[0..components.len() - 2];
            let mut package = itertools::join(package_parts, ".");

            // Files have format <package>/__pycache__/<module>.<cache_tag>.<extra tag><suffix>>
            let filename = rel_path
                .file_name()
                .expect("unable to get file name")
                .to_string_lossy()
                .to_string();

            let filename_parts = filename.split('.').collect::<Vec<&str>>();

            if filename_parts.len() < 3 {
                return None;
            }

            let mut remaining_filename = filename.clone();

            // The first part is always the module name.
            let module_name = filename_parts[0];
            remaining_filename = remaining_filename[module_name.len() + 1..].to_string();

            // The second part is the cache tag. It should match ours.
            if filename_parts[1] != self.cache_tag {
                return None;
            }

            // Keep the leading dot in case there is no cache tag: in this case the
            // suffix has the leading dot and we'll need to match against that.
            remaining_filename = remaining_filename[self.cache_tag.len()..].to_string();

            // Look for optional tag, of which we only recognize opt-1, opt-2, and None.
            let optimization_level = if filename_parts[2] == "opt-1" {
                remaining_filename = remaining_filename[6..].to_string();
                BytecodeOptimizationLevel::One
            } else if filename_parts[2] == "opt-2" {
                remaining_filename = remaining_filename[6..].to_string();
                BytecodeOptimizationLevel::Two
            } else {
                BytecodeOptimizationLevel::Zero
            };

            // Only the bytecode suffix should remain.
            if !self.suffixes.bytecode.contains(&remaining_filename) {
                return None;
            }

            let mut full_module_name: Vec<&str> = package_parts.to_vec();

            if module_name != "__init__" {
                full_module_name.push(&module_name);
            }

            let full_module_name = itertools::join(full_module_name, ".");

            if package.is_empty() {
                package = full_module_name.clone();
            }

            self.seen_packages.insert(package);

            return Some(PathItem::PythonResource(
                PythonModuleBytecode::from_path(
                    &full_module_name,
                    optimization_level,
                    &self.cache_tag,
                    path,
                )
                .into(),
            ));
        }

        let resource = match rel_path.extension().and_then(OsStr::to_str) {
            Some("egg") => PathItem::PythonResource(
                PythonEggFile {
                    data: self.resolve_file_data(path),
                }
                .into(),
            ),
            Some("pth") => PathItem::PythonResource(
                PythonPathExtension {
                    data: self.resolve_file_data(path),
                }
                .into(),
            ),
            _ => {
                // If it is some other file type, we categorize it as a resource
                // file. The package name and resource name are resolved later,
                // by the iterator.
                PathItem::ResourceFile(ResourceFile {
                    full_path: path.to_path_buf(),
                    relative_path: rel_path.to_path_buf(),
                })
            }
        };

        Some(resource)
    }
}

impl<'a> Iterator for PythonResourceIterator<'a> {
    type Item = Result<PythonResource<'a>>;

    fn next(&mut self) -> Option<Result<PythonResource<'a>>> {
        // Our strategy is to walk directory entries and buffer resource files locally.
        // We then emit those at the end, perhaps doing some post-processing along the
        // way.
        loop {
            if self.paths.is_empty() {
                break;
            }

            // If we're emitting PythonResource::File entries and we haven't
            // done so for this path, do so now.
            if self.emit_files && !self.paths[0].file_emitted {
                self.paths[0].file_emitted = true;

                let rel_path = self.paths[0]
                    .path
                    .strip_prefix(&self.root_path)
                    .expect("unable to strip path prefix")
                    .to_path_buf();

                let f = File {
                    path: rel_path,
                    entry: FileEntry {
                        executable: self.resolve_is_executable(&self.paths[0].path),
                        data: self.resolve_file_data(&self.paths[0].path),
                    },
                };

                return Some(Ok(f.into()));
            }

            if self.emit_non_files && !self.paths[0].non_file_emitted {
                self.paths[0].non_file_emitted = true;

                // Because resolve_path is a mutable borrow.
                let path_temp = self.paths[0].path.clone();

                if let Some(entry) = self.resolve_path(&path_temp) {
                    // Buffer Resource entries until later.
                    match entry {
                        PathItem::ResourceFile(resource) => {
                            self.resources.push(resource);
                        }
                        PathItem::PythonResource(resource) => {
                            return Some(Ok(resource));
                        }
                    }
                }
            }

            // We're done emitting variants for this path. Discard it and move
            // on to next record.
            //
            // Removing the first element is a bit inefficient. Should we
            // reverse storage / iteration order instead?
            self.paths.remove(0);
            continue;
        }

        loop {
            if self.resources.is_empty() {
                return None;
            }

            // This isn't efficient. But we shouldn't care.
            let resource = self.resources.remove(0);

            // Resource addressing in Python is a bit wonky. This is because the resource
            // reading APIs allow loading resources across package and directory boundaries.
            // For example, let's say we have a resource defined at the relative path
            // `foo/bar/resource.txt`. This resource could be accessed via the following
            // mechanisms:
            //
            // * Via the `resource.txt` resource on package `bar`'s resource reader.
            // * Via the `bar/resource.txt` resource on package `foo`'s resource reader.
            // * Via the `foo/bar/resource.txt` resource on the root resource reader.
            //
            // Furthermore, there could be resources in subdirectories that don't have
            // Python packages, forcing directory separators in resource names. e.g.
            // `foo/bar/resources/baz.txt`, where there isn't a `foo.bar.resources` Python
            // package.
            //
            // Our strategy for handling this is to initially resolve the relative path to
            // the resource. Then when we get to this code, we have awareness of all Python
            // packages and can supplement the relative path (which is the one true resource
            // identifier) with annotations, such as the leaf-most Python package.

            // Resources should always have a filename component. Otherwise how did we get here?
            let basename = resource
                .relative_path
                .file_name()
                .unwrap()
                .to_string_lossy();

            // We also resolve the leaf-most Python package that this resource is within and
            // the relative path within that package.
            let (leaf_package, relative_name) =
                if let Some(relative_directory) = resource.relative_path.parent() {
                    // We walk relative directory components until we find a Python package.
                    let mut components = relative_directory
                        .iter()
                        .map(|p| p.to_string_lossy())
                        .collect::<Vec<_>>();

                    let mut relative_components = vec![basename];
                    let mut package = None;
                    let mut relative_name = None;

                    while !components.is_empty() {
                        let candidate_package = itertools::join(&components, ".");

                        if self.seen_packages.contains(&candidate_package) {
                            package = Some(candidate_package);
                            relative_components.reverse();
                            relative_name = Some(itertools::join(&relative_components, "/"));
                            break;
                        }

                        let popped = components.pop().unwrap();
                        relative_components.push(popped);
                    }

                    (package, relative_name)
                } else {
                    (None, None)
                };

            // Resources without a resolved package are not legal.
            if leaf_package.is_none() {
                continue;
            }

            let leaf_package = leaf_package.unwrap();
            let relative_name = relative_name.unwrap();

            return Some(Ok(PythonPackageResource {
                leaf_package,
                relative_name,
                data: self.resolve_file_data(&resource.full_path),
                is_stdlib: false,
                is_test: false,
            }
            .into()));
        }
    }
}

/// Find Python resources in a directory.
///
/// Given a root directory path, walk the directory and find all Python
/// resources in it.
///
/// A resource is a Python source file, bytecode file, or resource file which
/// can be addressed via the ``A.B.C`` naming convention.
///
/// Returns an iterator of ``PythonResource`` instances.
pub fn find_python_resources<'a>(
    root_path: &Path,
    cache_tag: &str,
    suffixes: &PythonModuleSuffixes,
    emit_files: bool,
    emit_non_files: bool,
) -> PythonResourceIterator<'a> {
    PythonResourceIterator::new(root_path, cache_tag, suffixes, emit_files, emit_non_files)
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        once_cell::sync::Lazy,
        std::fs::{create_dir_all, write},
    };

    const DEFAULT_CACHE_TAG: &str = "cpython-37";

    static DEFAULT_SUFFIXES: Lazy<PythonModuleSuffixes> = Lazy::new(|| PythonModuleSuffixes {
        source: vec![".py".to_string()],
        bytecode: vec![".pyc".to_string()],
        debug_bytecode: vec![],
        optimized_bytecode: vec![],
        extension: vec![],
    });

    #[test]
    fn test_source_resolution() -> Result<()> {
        let td = tempfile::Builder::new()
            .prefix("python-packaging-test")
            .tempdir()?;
        let tp = td.path();

        let acme_path = tp.join("acme");
        let acme_a_path = acme_path.join("a");
        let acme_bar_path = acme_path.join("bar");

        create_dir_all(&acme_a_path).unwrap();
        create_dir_all(&acme_bar_path).unwrap();

        write(acme_path.join("__init__.py"), "")?;
        write(acme_a_path.join("__init__.py"), "")?;
        write(acme_bar_path.join("__init__.py"), "")?;

        write(acme_a_path.join("foo.py"), "# acme.foo")?;

        let resources =
            PythonResourceIterator::new(tp, DEFAULT_CACHE_TAG, &DEFAULT_SUFFIXES, true, true)
                .collect::<Result<Vec<_>>>()?;
        assert_eq!(resources.len(), 8);

        assert_eq!(
            resources[0],
            File {
                path: PathBuf::from("acme/__init__.py"),
                entry: FileEntry {
                    executable: false,
                    data: acme_path.join("__init__.py").into(),
                }
            }
            .into()
        );
        assert_eq!(
            resources[1],
            PythonModuleSource {
                name: "acme".to_string(),
                source: FileData::Path(acme_path.join("__init__.py")),
                is_package: true,
                cache_tag: DEFAULT_CACHE_TAG.to_string(),
                is_stdlib: false,
                is_test: false,
            }
            .into()
        );
        assert_eq!(
            resources[2],
            File {
                path: PathBuf::from("acme/a/__init__.py"),
                entry: FileEntry {
                    executable: false,
                    data: acme_a_path.join("__init__.py").into(),
                }
            }
            .into()
        );
        assert_eq!(
            resources[3],
            PythonModuleSource {
                name: "acme.a".to_string(),
                source: FileData::Path(acme_a_path.join("__init__.py")),
                is_package: true,
                cache_tag: DEFAULT_CACHE_TAG.to_string(),
                is_stdlib: false,
                is_test: false,
            }
            .into()
        );
        assert_eq!(
            resources[4],
            File {
                path: PathBuf::from("acme/a/foo.py"),
                entry: FileEntry {
                    executable: false,
                    data: acme_a_path.join("foo.py").into(),
                }
            }
            .into()
        );
        assert_eq!(
            resources[5],
            PythonModuleSource {
                name: "acme.a.foo".to_string(),
                source: FileData::Path(acme_a_path.join("foo.py")),
                is_package: false,
                cache_tag: DEFAULT_CACHE_TAG.to_string(),
                is_stdlib: false,
                is_test: false,
            }
            .into()
        );
        assert_eq!(
            resources[6],
            File {
                path: PathBuf::from("acme/bar/__init__.py"),
                entry: FileEntry {
                    executable: false,
                    data: acme_bar_path.join("__init__.py").into(),
                }
            }
            .into()
        );
        assert_eq!(
            resources[7],
            PythonModuleSource {
                name: "acme.bar".to_string(),
                source: FileData::Path(acme_bar_path.join("__init__.py")),
                is_package: true,
                cache_tag: DEFAULT_CACHE_TAG.to_string(),
                is_stdlib: false,
                is_test: false,
            }
            .into()
        );

        Ok(())
    }

    #[test]
    fn test_bytecode_resolution() -> Result<()> {
        let td = tempfile::Builder::new()
            .prefix("python-packaging-test")
            .tempdir()?;
        let tp = td.path();

        let acme_path = tp.join("acme");
        let acme_a_path = acme_path.join("a");
        let acme_bar_path = acme_path.join("bar");

        create_dir_all(&acme_a_path)?;
        create_dir_all(&acme_bar_path)?;

        let acme_pycache_path = acme_path.join("__pycache__");
        let acme_a_pycache_path = acme_a_path.join("__pycache__");
        let acme_bar_pycache_path = acme_bar_path.join("__pycache__");

        create_dir_all(&acme_pycache_path)?;
        create_dir_all(&acme_a_pycache_path)?;
        create_dir_all(&acme_bar_pycache_path)?;

        // Dummy paths that should be ignored.
        write(acme_pycache_path.join("__init__.pyc"), "")?;
        write(acme_pycache_path.join("__init__.cpython-37.foo.pyc"), "")?;

        write(acme_pycache_path.join("__init__.cpython-37.pyc"), "")?;
        write(acme_pycache_path.join("__init__.cpython-37.opt-1.pyc"), "")?;
        write(acme_pycache_path.join("__init__.cpython-37.opt-2.pyc"), "")?;
        write(acme_pycache_path.join("__init__.cpython-38.pyc"), "")?;
        write(acme_pycache_path.join("__init__.cpython-38.opt-1.pyc"), "")?;
        write(acme_pycache_path.join("__init__.cpython-38.opt-2.pyc"), "")?;
        write(acme_pycache_path.join("foo.cpython-37.pyc"), "")?;
        write(acme_pycache_path.join("foo.cpython-37.opt-1.pyc"), "")?;
        write(acme_pycache_path.join("foo.cpython-37.opt-2.pyc"), "")?;
        write(acme_pycache_path.join("foo.cpython-38.pyc"), "")?;
        write(acme_pycache_path.join("foo.cpython-38.opt-1.pyc"), "")?;
        write(acme_pycache_path.join("foo.cpython-38.opt-2.pyc"), "")?;

        write(acme_a_pycache_path.join("__init__.cpython-37.pyc"), "")?;
        write(
            acme_a_pycache_path.join("__init__.cpython-37.opt-1.pyc"),
            "",
        )?;
        write(
            acme_a_pycache_path.join("__init__.cpython-37.opt-2.pyc"),
            "",
        )?;
        write(acme_a_pycache_path.join("__init__.cpython-38.pyc"), "")?;
        write(
            acme_a_pycache_path.join("__init__.cpython-38.opt-1.pyc"),
            "",
        )?;
        write(
            acme_a_pycache_path.join("__init__.cpython-38.opt-2.pyc"),
            "",
        )?;
        write(acme_a_pycache_path.join("foo.cpython-37.pyc"), "")?;
        write(acme_a_pycache_path.join("foo.cpython-37.opt-1.pyc"), "")?;
        write(acme_a_pycache_path.join("foo.cpython-37.opt-2.pyc"), "")?;
        write(acme_a_pycache_path.join("foo.cpython-38.pyc"), "")?;
        write(acme_a_pycache_path.join("foo.cpython-38.opt-1.pyc"), "")?;
        write(acme_a_pycache_path.join("foo.cpython-38.opt-2.pyc"), "")?;

        write(acme_bar_pycache_path.join("__init__.cpython-37.pyc"), "")?;
        write(
            acme_bar_pycache_path.join("__init__.cpython-37.opt-1.pyc"),
            "",
        )?;
        write(
            acme_bar_pycache_path.join("__init__.cpython-37.opt-2.pyc"),
            "",
        )?;
        write(acme_bar_pycache_path.join("__init__.cpython-38.pyc"), "")?;
        write(
            acme_bar_pycache_path.join("__init__.cpython-38.opt-1.pyc"),
            "",
        )?;
        write(
            acme_bar_pycache_path.join("__init__.cpython-38.opt-2.pyc"),
            "",
        )?;
        write(acme_bar_pycache_path.join("foo.cpython-37.pyc"), "")?;
        write(acme_bar_pycache_path.join("foo.cpython-37.opt-1.pyc"), "")?;
        write(acme_bar_pycache_path.join("foo.cpython-37.opt-2.pyc"), "")?;
        write(acme_bar_pycache_path.join("foo.cpython-38.pyc"), "")?;
        write(acme_bar_pycache_path.join("foo.cpython-38.opt-1.pyc"), "")?;
        write(acme_bar_pycache_path.join("foo.cpython-38.opt-2.pyc"), "")?;

        let resources =
            PythonResourceIterator::new(tp, "cpython-38", &DEFAULT_SUFFIXES, false, true)
                .collect::<Result<Vec<_>>>()?;
        assert_eq!(resources.len(), 18);

        assert_eq!(
            resources[0],
            PythonModuleBytecode::from_path(
                "acme",
                BytecodeOptimizationLevel::One,
                "cpython-38",
                &acme_pycache_path.join("__init__.cpython-38.opt-1.pyc")
            )
            .into()
        );
        assert_eq!(
            resources[1],
            PythonModuleBytecode::from_path(
                "acme",
                BytecodeOptimizationLevel::Two,
                "cpython-38",
                &acme_pycache_path.join("__init__.cpython-38.opt-2.pyc")
            )
            .into()
        );
        assert_eq!(
            resources[2],
            PythonModuleBytecode::from_path(
                "acme",
                BytecodeOptimizationLevel::Zero,
                "cpython-38",
                &acme_pycache_path.join("__init__.cpython-38.pyc")
            )
            .into()
        );
        assert_eq!(
            resources[3],
            PythonModuleBytecode::from_path(
                "acme.foo",
                BytecodeOptimizationLevel::One,
                "cpython-38",
                &acme_pycache_path.join("foo.cpython-38.opt-1.pyc")
            )
            .into()
        );
        assert_eq!(
            resources[4],
            PythonModuleBytecode::from_path(
                "acme.foo",
                BytecodeOptimizationLevel::Two,
                "cpython-38",
                &acme_pycache_path.join("foo.cpython-38.opt-2.pyc")
            )
            .into()
        );
        assert_eq!(
            resources[5],
            PythonModuleBytecode::from_path(
                "acme.foo",
                BytecodeOptimizationLevel::Zero,
                "cpython-38",
                &acme_pycache_path.join("foo.cpython-38.pyc")
            )
            .into()
        );
        assert_eq!(
            resources[6],
            PythonModuleBytecode::from_path(
                "acme.a",
                BytecodeOptimizationLevel::One,
                "cpython-38",
                &acme_a_pycache_path.join("__init__.cpython-38.opt-1.pyc")
            )
            .into()
        );
        assert_eq!(
            resources[7],
            PythonModuleBytecode::from_path(
                "acme.a",
                BytecodeOptimizationLevel::Two,
                "cpython-38",
                &acme_a_pycache_path.join("__init__.cpython-38.opt-2.pyc")
            )
            .into()
        );
        assert_eq!(
            resources[8],
            PythonModuleBytecode::from_path(
                "acme.a",
                BytecodeOptimizationLevel::Zero,
                "cpython-38",
                &acme_a_pycache_path.join("__init__.cpython-38.pyc")
            )
            .into()
        );
        assert_eq!(
            resources[9],
            PythonModuleBytecode::from_path(
                "acme.a.foo",
                BytecodeOptimizationLevel::One,
                "cpython-38",
                &acme_a_pycache_path.join("foo.cpython-38.opt-1.pyc")
            )
            .into()
        );
        assert_eq!(
            resources[10],
            PythonModuleBytecode::from_path(
                "acme.a.foo",
                BytecodeOptimizationLevel::Two,
                "cpython-38",
                &acme_a_pycache_path.join("foo.cpython-38.opt-2.pyc")
            )
            .into()
        );
        assert_eq!(
            resources[11],
            PythonModuleBytecode::from_path(
                "acme.a.foo",
                BytecodeOptimizationLevel::Zero,
                "cpython-38",
                &acme_a_pycache_path.join("foo.cpython-38.pyc")
            )
            .into()
        );
        assert_eq!(
            resources[12],
            PythonModuleBytecode::from_path(
                "acme.bar",
                BytecodeOptimizationLevel::One,
                "cpython-38",
                &acme_bar_pycache_path.join("__init__.cpython-38.opt-1.pyc")
            )
            .into()
        );
        assert_eq!(
            resources[13],
            PythonModuleBytecode::from_path(
                "acme.bar",
                BytecodeOptimizationLevel::Two,
                "cpython-38",
                &acme_bar_pycache_path.join("__init__.cpython-38.opt-2.pyc")
            )
            .into()
        );
        assert_eq!(
            resources[14],
            PythonModuleBytecode::from_path(
                "acme.bar",
                BytecodeOptimizationLevel::Zero,
                "cpython-38",
                &acme_bar_pycache_path.join("__init__.cpython-38.pyc")
            )
            .into()
        );
        assert_eq!(
            resources[15],
            PythonModuleBytecode::from_path(
                "acme.bar.foo",
                BytecodeOptimizationLevel::One,
                "cpython-38",
                &acme_bar_pycache_path.join("foo.cpython-38.opt-1.pyc")
            )
            .into()
        );
        assert_eq!(
            resources[16],
            PythonModuleBytecode::from_path(
                "acme.bar.foo",
                BytecodeOptimizationLevel::Two,
                "cpython-38",
                &acme_bar_pycache_path.join("foo.cpython-38.opt-2.pyc")
            )
            .into()
        );
        assert_eq!(
            resources[17],
            PythonModuleBytecode::from_path(
                "acme.bar.foo",
                BytecodeOptimizationLevel::Zero,
                "cpython-38",
                &acme_bar_pycache_path.join("foo.cpython-38.pyc")
            )
            .into()
        );

        Ok(())
    }

    #[test]
    fn test_site_packages() -> Result<()> {
        let td = tempfile::Builder::new()
            .prefix("python-packaging-test")
            .tempdir()?;
        let tp = td.path();

        let sp_path = tp.join("site-packages");
        let acme_path = sp_path.join("acme");

        create_dir_all(&acme_path).unwrap();

        write(acme_path.join("__init__.py"), "")?;
        write(acme_path.join("bar.py"), "")?;

        let resources =
            PythonResourceIterator::new(tp, DEFAULT_CACHE_TAG, &DEFAULT_SUFFIXES, false, true)
                .collect::<Result<Vec<_>>>()?;
        assert_eq!(resources.len(), 2);

        assert_eq!(
            resources[0],
            PythonModuleSource {
                name: "acme".to_string(),
                source: FileData::Path(acme_path.join("__init__.py")),
                is_package: true,
                cache_tag: DEFAULT_CACHE_TAG.to_string(),
                is_stdlib: false,
                is_test: false,
            }
            .into()
        );
        assert_eq!(
            resources[1],
            PythonModuleSource {
                name: "acme.bar".to_string(),
                source: FileData::Path(acme_path.join("bar.py")),
                is_package: false,
                cache_tag: DEFAULT_CACHE_TAG.to_string(),
                is_stdlib: false,
                is_test: false,
            }
            .into()
        );

        Ok(())
    }

    #[test]
    fn test_extension_module() -> Result<()> {
        let td = tempfile::Builder::new()
            .prefix("python-packaging-test")
            .tempdir()?;
        let tp = td.path();

        create_dir_all(&tp.join("markupsafe"))?;

        let pyd_path = tp.join("foo.pyd");
        let so_path = tp.join("bar.so");
        let cffi_path = tp.join("_cffi_backend.cp37-win_amd64.pyd");
        let markupsafe_speedups_path = tp
            .join("markupsafe")
            .join("_speedups.cpython-37m-x86_64-linux-gnu.so");
        let zstd_path = tp.join("zstd.cpython-37m-x86_64-linux-gnu.so");

        write(&pyd_path, "")?;
        write(&so_path, "")?;
        write(&cffi_path, "")?;
        write(&markupsafe_speedups_path, "")?;
        write(&zstd_path, "")?;

        let suffixes = PythonModuleSuffixes {
            source: vec![],
            bytecode: vec![],
            debug_bytecode: vec![],
            optimized_bytecode: vec![],
            extension: vec![
                ".cp37-win_amd64.pyd".to_string(),
                ".cp37-win32.pyd".to_string(),
                ".cpython-37m-x86_64-linux-gnu.so".to_string(),
                ".pyd".to_string(),
                ".so".to_string(),
            ],
        };

        let resources = PythonResourceIterator::new(tp, "cpython-37", &suffixes, false, true)
            .collect::<Result<Vec<_>>>()?;

        assert_eq!(resources.len(), 5);

        assert_eq!(
            resources[0],
            PythonExtensionModule {
                name: "_cffi_backend".to_string(),
                init_fn: Some("PyInit__cffi_backend".to_string()),
                extension_file_suffix: ".cp37-win_amd64.pyd".to_string(),
                shared_library: Some(FileData::Path(cffi_path)),
                object_file_data: vec![],
                is_package: false,
                link_libraries: vec![],
                is_stdlib: false,
                builtin_default: false,
                required: false,
                variant: None,
                license: None,
            }
            .into()
        );
        assert_eq!(
            resources[1],
            PythonExtensionModule {
                name: "bar".to_string(),
                init_fn: Some("PyInit_bar".to_string()),
                extension_file_suffix: ".so".to_string(),
                shared_library: Some(FileData::Path(so_path)),
                object_file_data: vec![],
                is_package: false,
                link_libraries: vec![],
                is_stdlib: false,
                builtin_default: false,
                required: false,
                variant: None,
                license: None,
            }
            .into(),
        );
        assert_eq!(
            resources[2],
            PythonExtensionModule {
                name: "foo".to_string(),
                init_fn: Some("PyInit_foo".to_string()),
                extension_file_suffix: ".pyd".to_string(),
                shared_library: Some(FileData::Path(pyd_path)),
                object_file_data: vec![],
                is_package: false,
                link_libraries: vec![],
                is_stdlib: false,
                builtin_default: false,
                required: false,
                variant: None,
                license: None,
            }
            .into(),
        );
        assert_eq!(
            resources[3],
            PythonExtensionModule {
                name: "markupsafe._speedups".to_string(),
                init_fn: Some("PyInit__speedups".to_string()),
                extension_file_suffix: ".cpython-37m-x86_64-linux-gnu.so".to_string(),
                shared_library: Some(FileData::Path(markupsafe_speedups_path)),
                object_file_data: vec![],
                is_package: false,
                link_libraries: vec![],
                is_stdlib: false,
                builtin_default: false,
                required: false,
                variant: None,
                license: None,
            }
            .into(),
        );
        assert_eq!(
            resources[4],
            PythonExtensionModule {
                name: "zstd".to_string(),
                init_fn: Some("PyInit_zstd".to_string()),
                extension_file_suffix: ".cpython-37m-x86_64-linux-gnu.so".to_string(),
                shared_library: Some(FileData::Path(zstd_path)),
                object_file_data: vec![],
                is_package: false,
                link_libraries: vec![],
                is_stdlib: false,
                builtin_default: false,
                required: false,
                variant: None,
                license: None,
            }
            .into(),
        );

        Ok(())
    }

    #[test]
    fn test_egg_file() -> Result<()> {
        let td = tempfile::Builder::new()
            .prefix("python-packaging-test")
            .tempdir()?;
        let tp = td.path();

        create_dir_all(&tp)?;

        let egg_path = tp.join("foo-1.0-py3.7.egg");
        write(&egg_path, "")?;

        let resources =
            PythonResourceIterator::new(tp, DEFAULT_CACHE_TAG, &DEFAULT_SUFFIXES, false, true)
                .collect::<Result<Vec<_>>>()?;
        assert_eq!(resources.len(), 1);

        assert_eq!(
            resources[0],
            PythonEggFile {
                data: FileData::Path(egg_path)
            }
            .into()
        );

        Ok(())
    }

    #[test]
    fn test_egg_dir() -> Result<()> {
        let td = tempfile::Builder::new()
            .prefix("python-packaging-test")
            .tempdir()?;
        let tp = td.path();

        create_dir_all(&tp)?;

        let egg_path = tp.join("site-packages").join("foo-1.0-py3.7.egg");
        let egg_info_path = egg_path.join("EGG-INFO");
        let package_path = egg_path.join("foo");

        create_dir_all(&egg_info_path)?;
        create_dir_all(&package_path)?;

        write(egg_info_path.join("PKG-INFO"), "")?;
        write(package_path.join("__init__.py"), "")?;
        write(package_path.join("bar.py"), "")?;

        let resources =
            PythonResourceIterator::new(tp, DEFAULT_CACHE_TAG, &DEFAULT_SUFFIXES, false, true)
                .collect::<Result<Vec<_>>>()?;
        assert_eq!(resources.len(), 2);

        assert_eq!(
            resources[0],
            PythonModuleSource {
                name: "foo".to_string(),
                source: FileData::Path(package_path.join("__init__.py")),
                is_package: true,
                cache_tag: DEFAULT_CACHE_TAG.to_string(),
                is_stdlib: false,
                is_test: false,
            }
            .into()
        );
        assert_eq!(
            resources[1],
            PythonModuleSource {
                name: "foo.bar".to_string(),
                source: FileData::Path(package_path.join("bar.py")),
                is_package: false,
                cache_tag: DEFAULT_CACHE_TAG.to_string(),
                is_stdlib: false,
                is_test: false,
            }
            .into()
        );

        Ok(())
    }

    #[test]
    fn test_pth_file() -> Result<()> {
        let td = tempfile::Builder::new()
            .prefix("python-packaging-test")
            .tempdir()?;
        let tp = td.path();

        create_dir_all(&tp)?;

        let pth_path = tp.join("foo.pth");
        write(&pth_path, "")?;

        let resources =
            PythonResourceIterator::new(tp, DEFAULT_CACHE_TAG, &DEFAULT_SUFFIXES, false, true)
                .collect::<Result<Vec<_>>>()?;
        assert_eq!(resources.len(), 1);

        assert_eq!(
            resources[0],
            PythonPathExtension {
                data: FileData::Path(pth_path)
            }
            .into()
        );

        Ok(())
    }

    /// Resource files without a package are not valid.
    #[test]
    fn test_root_resource_file() -> Result<()> {
        let td = tempfile::Builder::new()
            .prefix("python-packaging-test")
            .tempdir()?;
        let tp = td.path();

        let resource_path = tp.join("resource.txt");
        write(&resource_path, "content")?;

        assert!(
            PythonResourceIterator::new(tp, DEFAULT_CACHE_TAG, &DEFAULT_SUFFIXES, false, true)
                .next()
                .is_none()
        );

        Ok(())
    }

    /// Resource files in a relative directory without a package are not valid.
    #[test]
    fn test_relative_resource_no_package() -> Result<()> {
        let td = tempfile::Builder::new()
            .prefix("python-packaging-test")
            .tempdir()?;
        let tp = td.path();

        write(&tp.join("foo.py"), "")?;
        let resource_dir = tp.join("resources");
        create_dir_all(&resource_dir)?;

        let resource_path = resource_dir.join("resource.txt");
        write(&resource_path, "content")?;

        let resources =
            PythonResourceIterator::new(tp, DEFAULT_CACHE_TAG, &DEFAULT_SUFFIXES, false, true)
                .collect::<Result<Vec<_>>>()?;
        assert_eq!(resources.len(), 1);

        assert_eq!(
            resources[0],
            PythonModuleSource {
                name: "foo".to_string(),
                source: FileData::Path(tp.join("foo.py")),
                is_package: false,
                cache_tag: DEFAULT_CACHE_TAG.to_string(),
                is_stdlib: false,
                is_test: false,
            }
            .into()
        );

        Ok(())
    }

    /// Resource files next to a package are detected.
    #[test]
    fn test_relative_package_resource() -> Result<()> {
        let td = tempfile::Builder::new()
            .prefix("python-packaging-test")
            .tempdir()?;
        let tp = td.path();

        let package_dir = tp.join("foo");
        create_dir_all(&package_dir)?;

        let module_path = package_dir.join("__init__.py");
        write(&module_path, "")?;
        let resource_path = package_dir.join("resource.txt");
        write(&resource_path, "content")?;

        let resources =
            PythonResourceIterator::new(tp, DEFAULT_CACHE_TAG, &DEFAULT_SUFFIXES, false, true)
                .collect::<Result<Vec<_>>>()?;

        assert_eq!(resources.len(), 2);
        assert_eq!(
            resources[0],
            PythonModuleSource {
                name: "foo".to_string(),
                source: FileData::Path(module_path),
                is_package: true,
                cache_tag: DEFAULT_CACHE_TAG.to_string(),
                is_stdlib: false,
                is_test: false,
            }
            .into()
        );
        assert_eq!(
            resources[1],
            PythonPackageResource {
                leaf_package: "foo".to_string(),
                relative_name: "resource.txt".to_string(),
                data: FileData::Path(resource_path),
                is_stdlib: false,
                is_test: false,
            }
            .into()
        );

        Ok(())
    }

    /// Resource files in sub-directory are detected.
    #[test]
    fn test_subdirectory_resource() -> Result<()> {
        let td = tempfile::Builder::new()
            .prefix("python-packaging-test")
            .tempdir()?;
        let tp = td.path();

        let package_dir = tp.join("foo");
        let subdir = package_dir.join("resources");
        create_dir_all(&subdir)?;

        let module_path = package_dir.join("__init__.py");
        write(&module_path, "")?;
        let resource_path = subdir.join("resource.txt");
        write(&resource_path, "content")?;

        let resources =
            PythonResourceIterator::new(tp, DEFAULT_CACHE_TAG, &DEFAULT_SUFFIXES, false, true)
                .collect::<Result<Vec<_>>>()?;

        assert_eq!(resources.len(), 2);
        assert_eq!(
            resources[0],
            PythonModuleSource {
                name: "foo".to_string(),
                source: FileData::Path(module_path),
                is_package: true,
                cache_tag: DEFAULT_CACHE_TAG.to_string(),
                is_stdlib: false,
                is_test: false,
            }
            .into(),
        );
        assert_eq!(
            resources[1],
            PythonPackageResource {
                leaf_package: "foo".to_string(),
                relative_name: "resources/resource.txt".to_string(),
                data: FileData::Path(resource_path),
                is_stdlib: false,
                is_test: false,
            }
            .into()
        );

        Ok(())
    }

    /// .dist-info directory ignored if METADATA file not present.
    #[test]
    fn test_distinfo_missing_metadata() -> Result<()> {
        let td = tempfile::Builder::new()
            .prefix("python-packaging-test")
            .tempdir()?;
        let tp = td.path();

        let dist_path = tp.join("foo-1.2.dist-info");
        create_dir_all(&dist_path)?;
        let resource = dist_path.join("file.txt");
        write(&resource, "content")?;

        let resources =
            PythonResourceIterator::new(tp, DEFAULT_CACHE_TAG, &DEFAULT_SUFFIXES, false, true)
                .collect::<Result<Vec<_>>>()?;
        assert!(resources.is_empty());

        Ok(())
    }

    /// .dist-info with invalid METADATA file has no content emitted.
    #[test]
    fn test_distinfo_bad_metadata() -> Result<()> {
        let td = tempfile::Builder::new()
            .prefix("python-packaging-test")
            .tempdir()?;
        let tp = td.path();

        let dist_path = tp.join("foo-1.2.dist-info");
        create_dir_all(&dist_path)?;
        let metadata = dist_path.join("METADATA");
        write(&metadata, "bad content")?;
        let resource = dist_path.join("file.txt");
        write(&resource, "content")?;

        let resources =
            PythonResourceIterator::new(tp, DEFAULT_CACHE_TAG, &DEFAULT_SUFFIXES, false, true)
                .collect::<Result<Vec<_>>>()?;
        assert!(resources.is_empty());

        Ok(())
    }

    /// .dist-info with partial METADATA content has no content emitted.
    #[test]
    fn test_distinfo_partial_metadata() -> Result<()> {
        let td = tempfile::Builder::new()
            .prefix("python-packaging-test")
            .tempdir()?;
        let tp = td.path();

        let dist_path = tp.join("black-1.2.3.dist-info");
        create_dir_all(&dist_path)?;
        let metadata = dist_path.join("METADATA");
        write(&metadata, "Name: black\n")?;
        let resource = dist_path.join("file.txt");
        write(&resource, "content")?;

        let resources =
            PythonResourceIterator::new(tp, DEFAULT_CACHE_TAG, &DEFAULT_SUFFIXES, false, true)
                .collect::<Result<Vec<_>>>()?;
        assert!(resources.is_empty());

        Ok(())
    }

    /// .dist-info with partial METADATA content has no content emitted.
    #[test]
    fn test_distinfo_valid_metadata() -> Result<()> {
        let td = tempfile::Builder::new()
            .prefix("python-packaging-test")
            .tempdir()?;
        let tp = td.path();

        let dist_path = tp.join("black-1.2.3.dist-info");
        create_dir_all(&dist_path)?;
        let metadata_path = dist_path.join("METADATA");
        write(&metadata_path, "Name: black\nVersion: 1.2.3\n")?;
        let resource_path = dist_path.join("file.txt");
        write(&resource_path, "content")?;

        let subdir = dist_path.join("subdir");
        create_dir_all(&subdir)?;
        let subdir_resource_path = subdir.join("sub.txt");
        write(&subdir_resource_path, "content")?;

        let resources =
            PythonResourceIterator::new(tp, DEFAULT_CACHE_TAG, &DEFAULT_SUFFIXES, false, true)
                .collect::<Result<Vec<_>>>()?;
        assert_eq!(resources.len(), 3);

        assert_eq!(
            resources[0],
            PythonPackageDistributionResource {
                location: PythonPackageDistributionResourceFlavor::DistInfo,
                package: "black".to_string(),
                version: "1.2.3".to_string(),
                name: "METADATA".to_string(),
                data: FileData::Path(metadata_path),
            }
            .into()
        );
        assert_eq!(
            resources[1],
            PythonPackageDistributionResource {
                location: PythonPackageDistributionResourceFlavor::DistInfo,
                package: "black".to_string(),
                version: "1.2.3".to_string(),
                name: "file.txt".to_string(),
                data: FileData::Path(resource_path),
            }
            .into()
        );
        assert_eq!(
            resources[2],
            PythonPackageDistributionResource {
                location: PythonPackageDistributionResourceFlavor::DistInfo,
                package: "black".to_string(),
                version: "1.2.3".to_string(),
                name: "subdir/sub.txt".to_string(),
                data: FileData::Path(subdir_resource_path),
            }
            .into()
        );

        Ok(())
    }

    /// .dist-info with partial METADATA content has no content emitted.
    #[test]
    fn test_egginfo_valid_metadata() -> Result<()> {
        let td = tempfile::Builder::new()
            .prefix("python-packaging-test")
            .tempdir()?;
        let tp = td.path();

        let egg_path = tp.join("black-1.2.3.egg-info");
        create_dir_all(&egg_path)?;
        let metadata_path = egg_path.join("PKG-INFO");
        write(&metadata_path, "Name: black\nVersion: 1.2.3\n")?;
        let resource_path = egg_path.join("file.txt");
        write(&resource_path, "content")?;

        let subdir = egg_path.join("subdir");
        create_dir_all(&subdir)?;
        let subdir_resource_path = subdir.join("sub.txt");
        write(&subdir_resource_path, "content")?;

        let resources =
            PythonResourceIterator::new(tp, DEFAULT_CACHE_TAG, &DEFAULT_SUFFIXES, false, true)
                .collect::<Result<Vec<_>>>()?;
        assert_eq!(resources.len(), 3);

        assert_eq!(
            resources[0],
            PythonPackageDistributionResource {
                location: PythonPackageDistributionResourceFlavor::EggInfo,
                package: "black".to_string(),
                version: "1.2.3".to_string(),
                name: "PKG-INFO".to_string(),
                data: FileData::Path(metadata_path),
            }
            .into()
        );
        assert_eq!(
            resources[1],
            PythonPackageDistributionResource {
                location: PythonPackageDistributionResourceFlavor::EggInfo,
                package: "black".to_string(),
                version: "1.2.3".to_string(),
                name: "file.txt".to_string(),
                data: FileData::Path(resource_path),
            }
            .into()
        );
        assert_eq!(
            resources[2],
            PythonPackageDistributionResource {
                location: PythonPackageDistributionResourceFlavor::EggInfo,
                package: "black".to_string(),
                version: "1.2.3".to_string(),
                name: "subdir/sub.txt".to_string(),
                data: FileData::Path(subdir_resource_path),
            }
            .into()
        );

        Ok(())
    }

    #[test]
    fn test_memory_resources() -> Result<()> {
        let inputs = vec![
            File {
                path: PathBuf::from("foo/__init__.py"),
                entry: FileEntry {
                    executable: false,
                    data: vec![0].into(),
                },
            },
            File {
                path: PathBuf::from("foo/bar.py"),
                entry: FileEntry {
                    executable: true,
                    data: vec![1].into(),
                },
            },
        ];

        let resources = PythonResourceIterator::from_data_locations(
            &inputs,
            DEFAULT_CACHE_TAG,
            &DEFAULT_SUFFIXES,
            true,
            true,
        )?
        .collect::<Result<Vec<_>>>()?;

        assert_eq!(resources.len(), 4);
        assert_eq!(
            resources[0],
            File {
                path: PathBuf::from("foo/__init__.py"),
                entry: FileEntry {
                    executable: false,
                    data: vec![0].into(),
                }
            }
            .into()
        );
        assert_eq!(
            resources[1],
            PythonModuleSource {
                name: "foo".to_string(),
                source: FileData::Memory(vec![0]),
                is_package: true,
                cache_tag: DEFAULT_CACHE_TAG.to_string(),
                is_stdlib: false,
                is_test: false,
            }
            .into()
        );
        assert_eq!(
            resources[2],
            File {
                path: PathBuf::from("foo/bar.py"),
                entry: FileEntry {
                    executable: true,
                    data: vec![1].into(),
                }
            }
            .into()
        );
        assert_eq!(
            resources[3],
            PythonModuleSource {
                name: "foo.bar".to_string(),
                source: FileData::Memory(vec![1]),
                is_package: false,
                cache_tag: DEFAULT_CACHE_TAG.to_string(),
                is_stdlib: false,
                is_test: false,
            }
            .into()
        );

        Ok(())
    }
}
