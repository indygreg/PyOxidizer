// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*!
Defines primitives representing Python resources.
*/

use {
    anyhow::Result,
    python_packaging::{
        module_util::{packages_from_module_name, resolve_path_for_module},
        resource::{
            PythonExtensionModule, PythonModuleSource, PythonPackageDistributionResource,
            PythonPackageResource,
        },
    },
    tugger_file_manifest::{FileEntry, FileManifest},
};

pub trait AddToFileManifest {
    /// Add the object to a FileManifest instance.
    fn add_to_file_manifest(&self, manifest: &mut FileManifest, prefix: &str) -> Result<()>;
}

impl AddToFileManifest for PythonModuleSource {
    fn add_to_file_manifest(&self, manifest: &mut FileManifest, prefix: &str) -> Result<()> {
        let content = FileEntry {
            data: self.source.resolve()?.into(),
            executable: false,
        };

        manifest.add_file_entry(&self.resolve_path(prefix), content)?;

        for package in packages_from_module_name(&self.name) {
            let package_path = resolve_path_for_module(prefix, &package, true, None);

            if !manifest.has_path(&package_path) {
                manifest.add_file_entry(
                    &package_path,
                    FileEntry {
                        data: vec![].into(),
                        executable: false,
                    },
                )?;
            }
        }

        Ok(())
    }
}

impl AddToFileManifest for PythonPackageResource {
    fn add_to_file_manifest(&self, manifest: &mut FileManifest, prefix: &str) -> Result<()> {
        let dest_path = self.resolve_path(prefix);

        manifest.add_file_entry(
            &dest_path,
            FileEntry {
                data: self.data.resolve()?.into(),
                executable: false,
            },
        )?;

        Ok(())
    }
}

impl AddToFileManifest for PythonPackageDistributionResource {
    fn add_to_file_manifest(&self, manifest: &mut FileManifest, prefix: &str) -> Result<()> {
        let dest_path = self.resolve_path(prefix);

        manifest.add_file_entry(
            &dest_path,
            FileEntry {
                data: self.data.resolve()?.into(),
                executable: false,
            },
        )?;

        Ok(())
    }
}

impl AddToFileManifest for PythonExtensionModule {
    fn add_to_file_manifest(&self, manifest: &mut FileManifest, prefix: &str) -> Result<()> {
        if let Some(data) = &self.shared_library {
            manifest.add_file_entry(
                &self.resolve_path(prefix),
                FileEntry {
                    data: data.resolve()?.into(),
                    executable: true,
                },
            )?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use {super::*, itertools::Itertools, std::path::PathBuf, tugger_file_manifest::FileData};

    const DEFAULT_CACHE_TAG: &str = "cpython-39";

    #[test]
    fn test_source_module_add_to_manifest_top_level() -> Result<()> {
        let mut m = FileManifest::default();

        PythonModuleSource {
            name: "foo".to_string(),
            source: FileData::Memory(vec![]),
            is_package: false,
            cache_tag: DEFAULT_CACHE_TAG.to_string(),
            is_stdlib: false,
            is_test: false,
        }
        .add_to_file_manifest(&mut m, ".")?;

        PythonModuleSource {
            name: "bar".to_string(),
            source: FileData::Memory(vec![]),
            is_package: false,
            cache_tag: DEFAULT_CACHE_TAG.to_string(),
            is_stdlib: false,
            is_test: false,
        }
        .add_to_file_manifest(&mut m, ".")?;

        let entries = m.iter_entries().collect_vec();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].0, &PathBuf::from("./bar.py"));
        assert_eq!(entries[1].0, &PathBuf::from("./foo.py"));

        Ok(())
    }

    #[test]
    fn test_source_module_add_to_manifest_top_level_package() -> Result<()> {
        let mut m = FileManifest::default();

        PythonModuleSource {
            name: "foo".to_string(),
            source: FileData::Memory(vec![]),
            is_package: true,
            cache_tag: DEFAULT_CACHE_TAG.to_string(),
            is_stdlib: false,
            is_test: false,
        }
        .add_to_file_manifest(&mut m, ".")?;

        let entries = m.iter_entries().collect_vec();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].0, &PathBuf::from("./foo/__init__.py"));

        Ok(())
    }

    #[test]
    fn test_source_module_add_to_manifest_missing_parent() -> Result<()> {
        let mut m = FileManifest::default();

        PythonModuleSource {
            name: "root.parent.child".to_string(),
            source: FileData::Memory(vec![]),
            is_package: false,
            cache_tag: DEFAULT_CACHE_TAG.to_string(),
            is_stdlib: false,
            is_test: false,
        }
        .add_to_file_manifest(&mut m, ".")?;

        let entries = m.iter_entries().collect_vec();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].0, &PathBuf::from("./root/__init__.py"));
        assert_eq!(entries[1].0, &PathBuf::from("./root/parent/__init__.py"));
        assert_eq!(entries[2].0, &PathBuf::from("./root/parent/child.py"));

        Ok(())
    }

    #[test]
    fn test_source_module_add_to_manifest_missing_parent_package() -> Result<()> {
        let mut m = FileManifest::default();

        PythonModuleSource {
            name: "root.parent.child".to_string(),
            source: FileData::Memory(vec![]),
            is_package: true,
            cache_tag: DEFAULT_CACHE_TAG.to_string(),
            is_stdlib: false,
            is_test: false,
        }
        .add_to_file_manifest(&mut m, ".")?;

        let entries = m.iter_entries().collect_vec();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].0, &PathBuf::from("./root/__init__.py"));
        assert_eq!(entries[1].0, &PathBuf::from("./root/parent/__init__.py"));
        assert_eq!(
            entries[2].0,
            &PathBuf::from("./root/parent/child/__init__.py")
        );

        Ok(())
    }
}
