// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    crate::file_resource::FileManifest,
    anyhow::{anyhow, Result},
    duct::cmd,
    slog::warn,
    std::{
        ffi::OsStr,
        io::{BufRead, BufReader, Write},
        path::{Path, PathBuf},
    },
    uuid::Uuid,
    xml::{
        common::XmlVersion,
        writer::{EventWriter, XmlEvent},
    },
};

/// Compute the `Id` of a directory.
pub fn directory_to_id(prefix: &str, path: &Path) -> String {
    format!(
        "{}.dir.{}",
        prefix,
        path.to_string_lossy().replace('/', ".").replace('-', "_")
    )
}

const GUID_NAMESPACE: &str = "https://github.com/indygreg/PyOxidizer/tugger/wix";

/// Compute the GUID of a component.
pub fn component_guid(prefix: &str, path: &Path) -> String {
    Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!("{}/{}/component/{}", GUID_NAMESPACE, prefix, path.display()).as_bytes(),
    )
    .to_hyphenated()
    .encode_upper(&mut Uuid::encode_buffer())
    .to_string()
}

pub fn component_id(prefix: &str, path: &Path) -> String {
    let guid = component_guid(prefix, path);

    format!(
        "{}.component.{}",
        prefix,
        guid.to_lowercase().replace('-', "_")
    )
}

pub fn file_guid(prefix: &str, path: &OsStr) -> String {
    Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!(
            "{}/{}/file/{}",
            GUID_NAMESPACE,
            prefix,
            path.to_string_lossy()
        )
        .as_bytes(),
    )
    .to_hyphenated()
    .encode_upper(&mut Uuid::encode_buffer())
    .to_string()
}

pub fn file_id(prefix: &str, path: &OsStr) -> String {
    let guid = file_guid(prefix, path);

    format!("{}.file.{}", prefix, guid.to_lowercase().replace('-', "_"))
}

pub fn component_group_id(prefix: &str, path: &Path) -> String {
    format!(
        "{}.group.{}",
        prefix,
        path.display()
            .to_string()
            .replace('/', ".")
            .replace('-', "_")
    )
}

/// Convert a `FileManifest` to WiX XML defining those files.
///
/// The generated XML contains `<Fragment>` and `<DirectoryRef>` for every
/// file in the install manifest.
///
/// `install_prefix` is a directory where the files in `manifest` are
/// installed.
///
/// `root_directory_id` defines the `<DirectoryRef Id="..."` value for the
/// root directory. Typically this ID is referenced in an outer wxs file
/// to materialize all files defined by this manifest/wxs file. A common
/// value is `INSTALLDIR` or `APPLICATIONFOLDER`.
///
/// `directory_id_prefix` defines a string prefix for `<DirectoryRef Id="..."`
/// values. The IDs will have the form `<directory_id_prefix>.<relative_directory>`,
/// with some normalization (e.g. `/` is normalized to `.` and `-` to `_`).
///
/// `component_id_prefix` defines a string prefix for `<Component Id="..."`
/// values.
pub fn write_file_manifest_to_wix<W: Write, P: AsRef<Path>>(
    writer: &mut EventWriter<W>,
    manifest: &FileManifest,
    install_prefix: P,
    root_directory_id: &str,
    id_prefix: &str,
) -> Result<()> {
    writer.write(XmlEvent::StartDocument {
        version: XmlVersion::Version10,
        encoding: Some("utf-8"),
        standalone: None,
    })?;

    writer.write(
        XmlEvent::start_element("Wix").default_ns("http://schemas.microsoft.com/wix/2006/wi"),
    )?;

    let directories = manifest.entries_by_directory();

    // Emit a <Fragment> for each directory.
    //
    // Each directory has a <DirectoryRef> pointing to its parent.
    for (directory, files) in &directories {
        let parent_directory_id = match directory {
            Some(path) => directory_to_id(id_prefix, path),
            None => root_directory_id.to_string(),
        };

        writer.write(XmlEvent::start_element("Fragment"))?;
        writer.write(XmlEvent::start_element("DirectoryRef").attr("Id", &parent_directory_id))?;

        // Add <Directory> entries for children directories.
        for (child_id, name) in directories
            .keys()
            // Root directory (None) can never be a child. Filter it.
            .filter_map(|d| if d.is_some() { Some(d.unwrap()) } else { None })
            .filter_map(|d| {
                // If we're in the root directory, children are directories without
                // a parent.
                if directory.is_none()
                    && (d.parent().is_none() || d.parent() == Some(Path::new("")))
                {
                    Some((directory_to_id(id_prefix, d), d.to_string_lossy()))
                } else if directory.is_some()
                    && &Some(d) != directory
                    && d.starts_with(directory.unwrap())
                {
                    if directory.unwrap().components().count() == d.components().count() - 1 {
                        Some((
                            directory_to_id(id_prefix, d),
                            d.components().last().unwrap().as_os_str().to_string_lossy(),
                        ))
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
        {
            writer.write(
                XmlEvent::start_element("Directory")
                    .attr("Id", &child_id)
                    .attr("Name", &*name),
            )?;
            writer.write(XmlEvent::end_element())?;
        }

        // Add `<Component>` for files in this directory.
        for filename in files.keys() {
            let rel_path = match directory {
                Some(d) => d.join(filename),
                None => PathBuf::from(filename),
            };

            let guid = component_guid(id_prefix, &rel_path);
            let id = component_id(id_prefix, &rel_path);

            writer.write(
                XmlEvent::start_element("Component")
                    .attr("Id", &id)
                    .attr("Guid", &guid),
            )?;

            let source = if let Some(directory) = directory {
                install_prefix.as_ref().join(directory).join(filename)
            } else {
                install_prefix.as_ref().join(filename)
            };
            writer.write(
                XmlEvent::start_element("File")
                    .attr("Id", &file_id(id_prefix, filename))
                    .attr("KeyPath", "yes")
                    .attr("Source", &source.display().to_string()),
            )?;

            // </File>
            writer.write(XmlEvent::end_element())?;
            // </Component>
            writer.write(XmlEvent::end_element())?;
        }

        // </DirectoryRef>
        writer.write(XmlEvent::end_element())?;
        // </Fragment>
        writer.write(XmlEvent::end_element())?;

        // Add a <Fragment> to define a component group for this directory tree.
        writer.write(XmlEvent::start_element("Fragment"))?;

        let component_group_id = match directory {
            Some(path) => component_group_id(id_prefix, path),
            None => component_group_id(id_prefix, Path::new("ROOT")),
        };

        writer.write(XmlEvent::start_element("ComponentGroup").attr("Id", &component_group_id))?;

        // Every file in this directory tree is part of this group. We could do
        // this more efficiently by using <ComponentGroupRef>. But since this is
        // an auto-generated file, the redundancy isn't too harmful.
        for p in manifest.entries().filter_map(|(p, _)| {
            if let Some(base) = directory {
                if p.starts_with(base) {
                    Some(p)
                } else {
                    None
                }
            } else {
                Some(p)
            }
        }) {
            let component_id = component_id(id_prefix, &p);

            writer.write(XmlEvent::start_element("ComponentRef").attr("Id", &component_id))?;
            writer.write(XmlEvent::end_element())?;
        }

        // </ComponentGroup>
        writer.write(XmlEvent::end_element())?;
        // </Fragment>
        writer.write(XmlEvent::end_element())?;
    }

    // </Wix>
    writer.write(XmlEvent::end_element())?;

    Ok(())
}

pub fn target_triple_to_wix_arch(triple: &str) -> Option<&'static str> {
    if triple.starts_with("x86_64-pc-windows") {
        Some("x64")
    } else if triple.starts_with("i586-pc-windows") || triple.starts_with("i686-pc-windows") {
        Some("x86")
    } else if triple.starts_with("aarch64-pc-windows") {
        Some("arm64")
    } else {
        None
    }
}

/// Run `candle.exe` against a `.wxs` file to produce a `.wixobj` file.
///
/// `wix_toolset_path` is the directory where `candle.exe` can be found.
///
/// `wxs_path` is the `.wxs` file to compile.
///
/// `arch` is turned into the value for `-arch`.
///
/// `defines` are preprocessor parameters that get passed to `-d<K>=<V>`.
///
/// `output_path` defines an optional output path. If not defined, a
/// `.wixobj` will be generated in the directory of the source file.
pub fn run_candle<P: AsRef<Path>, S: AsRef<str>>(
    logger: &slog::Logger,
    wix_toolset_path: P,
    wxs_path: P,
    arch: &str,
    defines: impl Iterator<Item = (S, S)>,
    output_path: Option<P>,
) -> Result<PathBuf> {
    let wxs_path = wxs_path.as_ref();
    let parent = wxs_path
        .parent()
        .ok_or_else(|| anyhow!("unable to find parent directory of wxs file"))?;

    let mut args = vec![
        "-nologo".to_string(),
        "-ext".to_string(),
        "WixBalExtension".to_string(),
        "-ext".to_string(),
        "WixUtilExtension".to_string(),
        "-arch".to_string(),
        arch.to_string(),
    ];

    for (k, v) in defines {
        args.push(format!("-d{}={}", k.as_ref(), v.as_ref()))
    }

    if let Some(output_path) = &output_path {
        args.push("-out".to_string());
        args.push(format!("{}", output_path.as_ref().display()));
    }

    args.push(
        wxs_path
            .file_name()
            .ok_or_else(|| anyhow!("unable to resolve filename"))?
            .to_string_lossy()
            .to_string(),
    );

    let candle_path = wix_toolset_path.as_ref().join("candle.exe");

    warn!(logger, "running candle for {}", wxs_path.display());

    let command = cmd(candle_path, args)
        .dir(parent)
        .stderr_to_stdout()
        .reader()?;
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
        Ok(if let Some(output_path) = &output_path {
            output_path.as_ref().to_path_buf()
        } else {
            wxs_path.with_extension("wixobj")
        })
    } else {
        Err(anyhow!("error running candle"))
    }
}

/// Run `light.exe` against multiple `.wixobj` files to link them together.
///
/// `wix_toolset_path` is the directory where `light` is located.
///
/// `build_path` is the current working directory of the invoked
/// process.
///
/// `wixobjs` is an iterable of paths defining `.wixobj` files to link together.
///
/// `variables` are extra variables to define via `-d<k>[=<v>]`.
pub fn run_light<
    P1: AsRef<Path>,
    P2: AsRef<Path>,
    P3: AsRef<Path>,
    P4: AsRef<Path>,
    S: AsRef<str>,
>(
    logger: &slog::Logger,
    wix_toolset_path: P1,
    build_path: P2,
    wixobjs: impl Iterator<Item = P3>,
    variables: impl Iterator<Item = (S, Option<S>)>,
    output_path: P4,
) -> Result<()> {
    let light_path = wix_toolset_path.as_ref().join("light.exe");

    let mut args = vec![
        "-nologo".to_string(),
        "-ext".to_string(),
        "WixUIExtension".to_string(),
        "-ext".to_string(),
        "WixBalExtension".to_string(),
        "-ext".to_string(),
        "WixUtilExtension".to_string(),
        "-out".to_string(),
        output_path.as_ref().display().to_string(),
    ];

    for (k, v) in variables {
        if let Some(v) = &v {
            args.push(format!("-d{}={}", k.as_ref(), v.as_ref()));
        } else {
            args.push(format!("-d{}", k.as_ref()));
        }
    }

    for p in wixobjs {
        args.push(format!("{}", p.as_ref().display()));
    }

    warn!(
        logger,
        "running light to produce {}",
        output_path.as_ref().display()
    );

    let command = cmd(light_path, args)
        .dir(build_path.as_ref())
        .stderr_to_stdout()
        .reader()?;
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
        Err(anyhow!("error running light.exe"))
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        file_resource::{FileContent, FileManifest},
        wix::*,
    };

    #[test]
    fn test_file_manifest_to_wix() -> Result<()> {
        let c = FileContent {
            data: vec![42],
            executable: false,
        };

        let mut m = FileManifest::default();
        m.add_file(Path::new("root.txt"), &c)?;
        m.add_file(Path::new("dir0/dir0_file0.txt"), &c)?;
        m.add_file(Path::new("dir0/child0/dir0_child0_file0.txt"), &c)?;
        m.add_file(Path::new("dir0/child0/dir0_child0_file1.txt"), &c)?;
        m.add_file(Path::new("dir0/child1/dir0_child1_file0.txt"), &c)?;
        m.add_file(Path::new("dir1/child0/dir1_child0_file0.txt"), &c)?;

        let buffer = Vec::new();
        let buf_writer = std::io::BufWriter::new(buffer);

        let mut config = EmitterConfig::new();
        config.perform_indent = true;
        let mut emitter = config.create_writer(buf_writer);

        let install_prefix = Path::new("/install-prefix");

        write_file_manifest_to_wix(&mut emitter, &m, &install_prefix, "root", "prefix")?;
        String::from_utf8(emitter.into_inner().into_inner()?)?;

        // TODO validate XML.

        Ok(())
    }
}
