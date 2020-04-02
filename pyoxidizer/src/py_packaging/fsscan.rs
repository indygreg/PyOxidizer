// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*!
Scanning the filesystem for Python resources.
*/

use {
    super::distribution::PythonModuleSuffixes,
    super::resource::{BytecodeModule, BytecodeOptimizationLevel, DataLocation, SourceModule},
    anyhow::Result,
    itertools::Itertools,
    std::collections::{BTreeMap, HashSet},
    std::ffi::OsStr,
    std::path::{Path, PathBuf},
};

pub fn is_package_from_path(path: &Path) -> bool {
    let file_name = path.file_name().unwrap().to_str().unwrap();
    file_name.starts_with("__init__.")
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
pub struct FileBasedResource {
    pub package: String,
    pub stem: String,
    pub full_name: String,
    pub path: PathBuf,
}

/// Represents a Python resource backed by the filesystem.
#[derive(Debug, PartialEq)]
pub enum PythonFileResource {
    /// Python module source code.
    ///
    /// i.e. a .py file.
    Source(SourceModule),

    /// A Python module bytecode file.
    ///
    /// i.e. a .pyc file.
    Bytecode(BytecodeModule),

    /// A Python module bytecode file, compiled at optimization level 1.
    ///
    /// i.e. a .opt-1.pyc file.
    BytecodeOpt1(BytecodeModule),

    /// A Python module bytecode file, compiled at optimization level 2.
    ///
    /// i.e. a .opt-2.pyc file.
    BytecodeOpt2 {
        package: String,
        stem: String,
        full_name: String,
        path: PathBuf,
    },

    /// A compiled extension module.
    ///
    /// i.e. a .so or .pyd file.
    ExtensionModule {
        package: String,
        stem: String,
        full_name: String,
        path: PathBuf,
        extension_file_suffix: String,
    },

    /// A non-module Python resource.
    Resource(FileBasedResource),

    /// A Python egg.
    ///
    /// i.e. a .egg file.
    EggFile { path: PathBuf },

    /// A Python path extension file.
    ///
    /// i.e. a .pth file.
    PthFile { path: PathBuf },

    /// Any other file.
    Other {
        package: String,
        stem: String,
        full_name: String,
        path: PathBuf,
    },
}

pub struct PythonResourceIterator {
    root_path: PathBuf,
    suffixes: PythonModuleSuffixes,
    walkdir_result: Box<dyn Iterator<Item = walkdir::DirEntry>>,
    seen_packages: HashSet<String>,
    resources: Vec<FileBasedResource>,
}

impl PythonResourceIterator {
    fn new(path: &Path, suffixes: &PythonModuleSuffixes) -> PythonResourceIterator {
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

        PythonResourceIterator {
            root_path: path.to_path_buf(),
            suffixes: suffixes.clone(),
            walkdir_result: Box::new(filtered),
            seen_packages: HashSet::new(),
            resources: Vec::new(),
        }
    }

    fn resolve_dir_entry(&mut self, entry: walkdir::DirEntry) -> Option<PythonFileResource> {
        let path = entry.path();

        let mut rel_path = path
            .strip_prefix(&self.root_path)
            .expect("unable to strip path prefix");
        let mut rel_str = rel_path.to_str().expect("could not convert path to str");
        let mut components = rel_path
            .iter()
            .map(|p| p.to_str().expect("unable to get path as str"))
            .collect::<Vec<_>>();

        // .dist-info directories contain packaging metadata. They aren't interesting to us.
        // We /could/ emit these files if we wanted to. But until there is a need, exclude them.
        if components[0].ends_with(".dist-info") {
            return None;
        }

        // Ditto for .egg-info directories.
        if components[0].ends_with(".egg-info") {
            return None;
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

                let stem = if module_name == "__init__" {
                    "".to_string()
                } else {
                    full_module_name.push(module_name);
                    module_name.to_string()
                };

                let full_module_name = itertools::join(full_module_name, ".");

                if package.is_empty() {
                    package = full_module_name.clone();
                }

                self.seen_packages.insert(package.clone());

                return Some(PythonFileResource::ExtensionModule {
                    package,
                    stem,
                    full_name: full_module_name,
                    path: path.to_path_buf(),
                    extension_file_suffix: ext_suffix.clone(),
                });
            }
        }

        // TODO use registered suffixes for source and bytecode detection.
        let resource = match rel_path.extension().and_then(OsStr::to_str) {
            Some("py") => {
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

                self.seen_packages.insert(package.clone());

                PythonFileResource::Source(SourceModule {
                    name: full_module_name,
                    source: DataLocation::Path(path.to_path_buf()),
                    is_package: is_package_from_path(&path),
                })
            }
            Some("pyc") => {
                // .pyc files should be in a __pycache__ directory.
                if components.len() < 2 {
                    panic!("encountered .pyc file with invalid path: {}", rel_str);
                }

                // Possibly from Python 2?
                if components[components.len() - 2] != "__pycache__" {
                    let package_parts = &components[0..components.len() - 1];
                    let package = itertools::join(package_parts, ".");
                    let full_name = itertools::join(&components, ".");
                    let stem = components[components.len() - 1].to_string();

                    return Some(PythonFileResource::Other {
                        package,
                        stem,
                        full_name,
                        path: path.to_path_buf(),
                    });
                }

                let package_parts = &components[0..components.len() - 2];
                let mut package = itertools::join(package_parts, ".");

                // Files have format <package>/__pycache__/<module>.cpython-37.opt-1.pyc
                let module_name = rel_path
                    .file_stem()
                    .expect("unable to get file stem")
                    .to_str()
                    .expect("unable to convert file stem to str");
                let module_name_parts = module_name.split('.').collect_vec();
                let module_name =
                    itertools::join(&module_name_parts[0..module_name_parts.len() - 1], ".");

                let mut full_module_name: Vec<&str> = package_parts.to_vec();

                let stem = if module_name == "__init__" {
                    ""
                } else {
                    full_module_name.push(&module_name);
                    &module_name
                };

                let full_module_name = itertools::join(full_module_name, ".");

                if package.is_empty() {
                    package = full_module_name.clone();
                }

                self.seen_packages.insert(package.clone());

                if rel_str.ends_with(".opt-1.pyc") {
                    PythonFileResource::BytecodeOpt1(BytecodeModule::from_path(
                        &full_module_name,
                        BytecodeOptimizationLevel::One,
                        path,
                    ))
                } else if rel_str.ends_with(".opt-2.pyc") {
                    PythonFileResource::BytecodeOpt2 {
                        package,
                        stem: stem.to_string(),
                        full_name: full_module_name,
                        path: path.to_path_buf(),
                    }
                } else {
                    PythonFileResource::Bytecode(BytecodeModule::from_path(
                        &full_module_name,
                        BytecodeOptimizationLevel::Zero,
                        path,
                    ))
                }
            }
            Some("egg") => PythonFileResource::EggFile {
                path: path.to_path_buf(),
            },
            Some("pth") => PythonFileResource::PthFile {
                path: path.to_path_buf(),
            },
            _ => {
                // If it isn't a .py or a .pyc file, it is a resource file.
                let package_parts = &components[0..components.len() - 1];
                let mut package = itertools::join(package_parts, ".");

                let name = itertools::join(&components, ".");
                let stem = components[components.len() - 1].to_string();

                if package.is_empty() {
                    package = name.clone();
                }

                PythonFileResource::Resource(FileBasedResource {
                    package,
                    stem,
                    full_name: name,
                    path: path.to_path_buf(),
                })
            }
        };

        Some(resource)
    }
}

impl Iterator for PythonResourceIterator {
    type Item = PythonFileResource;

    fn next(&mut self) -> Option<PythonFileResource> {
        // Our strategy is to walk directory entries and buffer resource files locally.
        // We then emit those at the end, perhaps doing some post-processing along the
        // way.
        loop {
            let res = self.walkdir_result.next();

            // We're out of directory entries;
            if res.is_none() {
                break;
            }

            let entry = res.unwrap();
            let python_resource = self.resolve_dir_entry(entry);

            // Try the next directory entry.
            if python_resource.is_none() {
                continue;
            }

            let python_resource = python_resource.unwrap();

            // Buffer Resource entries until later.
            if let PythonFileResource::Resource(resource) = python_resource {
                self.resources.push(resource);
                continue;
            }

            return Some(python_resource);
        }

        loop {
            if self.resources.is_empty() {
                return None;
            }

            // This isn't efficient. But we shouldn't care.
            let resource = self.resources.remove(0);

            // We initially resolve the package name from the relative filesystem path.
            // But not all directories are Python packages! When we encountered Python
            // modules during traversal, we added their package name to a set. Here, we
            // ensure the resource's package is an actual package, munging values if needed.
            if self.seen_packages.contains(&resource.package) {
                return Some(PythonFileResource::Resource(resource));
            } else {
                // We need to shift components from the reported package name into the
                // stem until we arrive at a known package.

                // Special case where there is no root package.
                if resource.package.is_empty() {
                    continue;
                }

                let mut components = resource.package.split('.').collect_vec();
                let mut shift_parts = Vec::new();

                while !components.is_empty() {
                    let part = components.pop().unwrap();
                    shift_parts.push(part);
                    let new_package = itertools::join(&components, ".");

                    if !self.seen_packages.contains(&new_package) {
                        continue;
                    }

                    // We have arrived at a known package. Shift collected parts in names
                    // accordingly.
                    shift_parts.reverse();
                    let prepend = itertools::join(shift_parts, ".");

                    // Use / instead of . because this emulates filesystem behavior.
                    let stem = prepend + "/" + &resource.stem;

                    return Some(PythonFileResource::Resource(FileBasedResource {
                        package: new_package,
                        stem,
                        full_name: resource.full_name,
                        path: resource.path,
                    }));
                }

                // Hmmm. We couldn't resolve this resource to a known Python package. Let's
                // just emit it and let downstream deal with it.
                return Some(PythonFileResource::Resource(resource));
            }
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
pub fn find_python_resources(
    root_path: &Path,
    suffixes: &PythonModuleSuffixes,
) -> PythonResourceIterator {
    PythonResourceIterator::new(root_path, suffixes)
}

pub fn find_python_modules(
    root_path: &Path,
    suffixes: &PythonModuleSuffixes,
) -> Result<BTreeMap<String, Vec<u8>>> {
    let mut mods = BTreeMap::new();

    for resource in find_python_resources(root_path, suffixes) {
        if let PythonFileResource::Source(module) = resource {
            let data = module.source.resolve()?;
            mods.insert(module.name, data);
        }
    }

    Ok(mods)
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        std::fs::{create_dir_all, write},
    };

    #[test]
    fn test_source_resolution() {
        let td = tempdir::TempDir::new("pyoxidizer-test").unwrap();
        let tp = td.path();

        let acme_path = tp.join("acme");
        let acme_a_path = acme_path.join("a");
        let acme_bar_path = acme_path.join("bar");

        create_dir_all(&acme_a_path).unwrap();
        create_dir_all(&acme_bar_path).unwrap();

        write(acme_path.join("__init__.py"), "").unwrap();
        write(acme_a_path.join("__init__.py"), "").unwrap();
        write(acme_bar_path.join("__init__.py"), "").unwrap();

        write(acme_a_path.join("foo.py"), "# acme.foo").unwrap();

        let suffixes = PythonModuleSuffixes {
            source: vec![],
            bytecode: vec![],
            debug_bytecode: vec![],
            optimized_bytecode: vec![],
            extension: vec![],
        };

        let resources = PythonResourceIterator::new(tp, &suffixes).collect_vec();
        assert_eq!(resources.len(), 4);

        assert_eq!(
            resources[0],
            PythonFileResource::Source(SourceModule {
                name: "acme".to_string(),
                source: DataLocation::Path(acme_path.join("__init__.py")),
                is_package: true,
            })
        );
        assert_eq!(
            resources[1],
            PythonFileResource::Source(SourceModule {
                name: "acme.a".to_string(),
                source: DataLocation::Path(acme_a_path.join("__init__.py")),
                is_package: true,
            })
        );
        assert_eq!(
            resources[2],
            PythonFileResource::Source(SourceModule {
                name: "acme.a.foo".to_string(),
                source: DataLocation::Path(acme_a_path.join("foo.py")),
                is_package: false,
            })
        );
        assert_eq!(
            resources[3],
            PythonFileResource::Source(SourceModule {
                name: "acme.bar".to_string(),
                source: DataLocation::Path(acme_bar_path.join("__init__.py")),
                is_package: true,
            })
        );
    }

    #[test]
    fn test_site_packages() {
        let td = tempdir::TempDir::new("pyoxidizer-test").unwrap();
        let tp = td.path();

        let sp_path = tp.join("site-packages");
        let acme_path = sp_path.join("acme");

        create_dir_all(&acme_path).unwrap();

        write(acme_path.join("__init__.py"), "").unwrap();
        write(acme_path.join("bar.py"), "").unwrap();

        let suffixes = PythonModuleSuffixes {
            source: vec![],
            bytecode: vec![],
            debug_bytecode: vec![],
            optimized_bytecode: vec![],
            extension: vec![],
        };

        let resources = PythonResourceIterator::new(tp, &suffixes).collect_vec();
        assert_eq!(resources.len(), 2);

        assert_eq!(
            resources[0],
            PythonFileResource::Source(SourceModule {
                name: "acme".to_string(),
                source: DataLocation::Path(acme_path.join("__init__.py")),
                is_package: true,
            })
        );
        assert_eq!(
            resources[1],
            PythonFileResource::Source(SourceModule {
                name: "acme.bar".to_string(),
                source: DataLocation::Path(acme_path.join("bar.py")),
                is_package: false,
            })
        );
    }

    #[test]
    fn test_extension_module() -> Result<()> {
        let td = tempdir::TempDir::new("pyoxidizer-test")?;
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

        let resources = PythonResourceIterator::new(tp, &suffixes).collect_vec();

        assert_eq!(resources.len(), 5);

        assert_eq!(
            resources[0],
            PythonFileResource::ExtensionModule {
                package: "_cffi_backend".to_string(),
                stem: "_cffi_backend".to_string(),
                full_name: "_cffi_backend".to_string(),
                path: cffi_path,
                extension_file_suffix: ".cp37-win_amd64.pyd".to_string(),
            }
        );
        assert_eq!(
            resources[1],
            PythonFileResource::ExtensionModule {
                package: "bar".to_string(),
                stem: "bar".to_string(),
                full_name: "bar".to_string(),
                path: so_path,
                extension_file_suffix: ".so".to_string(),
            }
        );
        assert_eq!(
            resources[2],
            PythonFileResource::ExtensionModule {
                package: "foo".to_string(),
                stem: "foo".to_string(),
                full_name: "foo".to_string(),
                path: pyd_path,
                extension_file_suffix: ".pyd".to_string(),
            }
        );
        assert_eq!(
            resources[3],
            PythonFileResource::ExtensionModule {
                package: "markupsafe".to_string(),
                stem: "_speedups".to_string(),
                full_name: "markupsafe._speedups".to_string(),
                path: markupsafe_speedups_path,
                extension_file_suffix: ".cpython-37m-x86_64-linux-gnu.so".to_string(),
            }
        );
        assert_eq!(
            resources[4],
            PythonFileResource::ExtensionModule {
                package: "zstd".to_string(),
                stem: "zstd".to_string(),
                full_name: "zstd".to_string(),
                path: zstd_path,
                extension_file_suffix: ".cpython-37m-x86_64-linux-gnu.so".to_string(),
            }
        );

        Ok(())
    }

    #[test]
    fn test_egg_file() {
        let td = tempdir::TempDir::new("pyoxidizer-test").unwrap();
        let tp = td.path();

        create_dir_all(&tp).unwrap();

        let egg_path = tp.join("foo-1.0-py3.7.egg");
        write(&egg_path, "").unwrap();

        let suffixes = PythonModuleSuffixes {
            source: vec![],
            bytecode: vec![],
            debug_bytecode: vec![],
            optimized_bytecode: vec![],
            extension: vec![],
        };

        let resources = PythonResourceIterator::new(tp, &suffixes).collect_vec();
        assert_eq!(resources.len(), 1);

        assert_eq!(resources[0], PythonFileResource::EggFile { path: egg_path });
    }

    #[test]
    fn test_egg_dir() {
        let td = tempdir::TempDir::new("pyoxidizer-test").unwrap();
        let tp = td.path();

        create_dir_all(&tp).unwrap();

        let egg_path = tp.join("site-packages").join("foo-1.0-py3.7.egg");
        let egg_info_path = egg_path.join("EGG-INFO");
        let package_path = egg_path.join("foo");

        create_dir_all(&egg_info_path).unwrap();
        create_dir_all(&package_path).unwrap();

        write(egg_info_path.join("PKG-INFO"), "").unwrap();
        write(package_path.join("__init__.py"), "").unwrap();
        write(package_path.join("bar.py"), "").unwrap();

        let suffixes = PythonModuleSuffixes {
            source: vec![],
            bytecode: vec![],
            debug_bytecode: vec![],
            optimized_bytecode: vec![],
            extension: vec![],
        };

        let resources = PythonResourceIterator::new(tp, &suffixes).collect_vec();
        assert_eq!(resources.len(), 2);

        assert_eq!(
            resources[0],
            PythonFileResource::Source(SourceModule {
                name: "foo".to_string(),
                source: DataLocation::Path(package_path.join("__init__.py")),
                is_package: true,
            })
        );
        assert_eq!(
            resources[1],
            PythonFileResource::Source(SourceModule {
                name: "foo.bar".to_string(),
                source: DataLocation::Path(package_path.join("bar.py")),
                is_package: false,
            })
        );
    }

    #[test]
    fn test_pth_file() {
        let td = tempdir::TempDir::new("pyoxidizer-test").unwrap();
        let tp = td.path();

        create_dir_all(&tp).unwrap();

        let pth_path = tp.join("foo.pth");
        write(&pth_path, "").unwrap();

        let suffixes = PythonModuleSuffixes {
            source: vec![],
            bytecode: vec![],
            debug_bytecode: vec![],
            optimized_bytecode: vec![],
            extension: vec![],
        };

        let resources = PythonResourceIterator::new(tp, &suffixes).collect_vec();
        assert_eq!(resources.len(), 1);

        assert_eq!(resources[0], PythonFileResource::PthFile { path: pth_path });
    }
}
