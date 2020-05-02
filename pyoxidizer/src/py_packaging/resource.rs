// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*!
Defines primitives representing Python resources.
*/

use {
    crate::app_packaging::resource::{FileContent, FileManifest},
    anyhow::Result,
    python_packaging::module_util::{packages_from_module_name, resolve_path_for_module},
    python_packaging::resource::{
        PythonExtensionModule, PythonModuleSource, PythonPackageDistributionResource,
        PythonPackageResource,
    },
};

pub trait AddToFileManifest {
    /// Add the object to a FileManifest instance.
    fn add_to_file_manifest(&self, manifest: &mut FileManifest, prefix: &str) -> Result<()>;
}

impl AddToFileManifest for PythonModuleSource {
    fn add_to_file_manifest(&self, manifest: &mut FileManifest, prefix: &str) -> Result<()> {
        let content = FileContent {
            data: self.source.resolve()?,
            executable: false,
        };

        manifest.add_file(&self.resolve_path(prefix), &content)?;

        for package in packages_from_module_name(&self.name) {
            let package_path = resolve_path_for_module(prefix, &package, true, None);

            if !manifest.has_path(&package_path) {
                manifest.add_file(
                    &package_path,
                    &FileContent {
                        data: vec![],
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

        manifest.add_file(
            &dest_path,
            &FileContent {
                data: self.data.resolve()?,
                executable: false,
            },
        )
    }
}

impl AddToFileManifest for PythonPackageDistributionResource {
    fn add_to_file_manifest(&self, manifest: &mut FileManifest, prefix: &str) -> Result<()> {
        let dest_path = self.resolve_path(prefix);

        manifest.add_file(
            &dest_path,
            &FileContent {
                data: self.data.resolve()?,
                executable: false,
            },
        )
    }
}

impl AddToFileManifest for PythonExtensionModule {
    fn add_to_file_manifest(&self, manifest: &mut FileManifest, prefix: &str) -> Result<()> {
        if let Some(data) = &self.extension_data {
            manifest.add_file(
                &self.resolve_path(prefix),
                &FileContent {
                    data: data.resolve()?,
                    executable: true,
                },
            )
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use {
        super::*, itertools::Itertools, python_packaging::resource::DataLocation,
        std::path::PathBuf,
    };

    const DEFAULT_CACHE_TAG: &str = "cpython-37";

    #[test]
    fn test_source_module_add_to_manifest_top_level() -> Result<()> {
        let mut m = FileManifest::default();

        PythonModuleSource {
            name: "foo".to_string(),
            source: DataLocation::Memory(vec![]),
            is_package: false,
            cache_tag: DEFAULT_CACHE_TAG.to_string(),
        }
        .add_to_file_manifest(&mut m, ".")?;

        PythonModuleSource {
            name: "bar".to_string(),
            source: DataLocation::Memory(vec![]),
            is_package: false,
            cache_tag: DEFAULT_CACHE_TAG.to_string(),
        }
        .add_to_file_manifest(&mut m, ".")?;

        let entries = m.entries().collect_vec();
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
            source: DataLocation::Memory(vec![]),
            is_package: true,
            cache_tag: DEFAULT_CACHE_TAG.to_string(),
        }
        .add_to_file_manifest(&mut m, ".")?;

        let entries = m.entries().collect_vec();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].0, &PathBuf::from("./foo/__init__.py"));

        Ok(())
    }

    #[test]
    fn test_source_module_add_to_manifest_missing_parent() -> Result<()> {
        let mut m = FileManifest::default();

        PythonModuleSource {
            name: "root.parent.child".to_string(),
            source: DataLocation::Memory(vec![]),
            is_package: false,
            cache_tag: DEFAULT_CACHE_TAG.to_string(),
        }
        .add_to_file_manifest(&mut m, ".")?;

        let entries = m.entries().collect_vec();
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
            source: DataLocation::Memory(vec![]),
            is_package: true,
            cache_tag: DEFAULT_CACHE_TAG.to_string(),
        }
        .add_to_file_manifest(&mut m, ".")?;

        let entries = m.entries().collect_vec();
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
