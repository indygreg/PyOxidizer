// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    crate::{file_resource::FileManifest, http::download_and_verify, zipfile::extract_zip},
    anyhow::{anyhow, Result},
    duct::cmd,
    handlebars::Handlebars,
    lazy_static::lazy_static,
    slog::warn,
    std::{
        borrow::Cow,
        collections::BTreeMap,
        ffi::OsStr,
        io::{BufRead, BufReader, Write},
        path::{Path, PathBuf},
    },
    uuid::Uuid,
    xml::{
        common::XmlVersion,
        writer::{EmitterConfig, EventWriter, XmlEvent},
    },
};

const TOOLSET_URL: &str =
    "https://github.com/wixtoolset/wix3/releases/download/wix3112rtm/wix311-binaries.zip";
const TOOLSET_SHA256: &str = "2c1888d5d1dba377fc7fa14444cf556963747ff9a0a289a3599cf09da03b9e2e";

const VC_REDIST_X86_URL: &str =
    "https://download.visualstudio.microsoft.com/download/pr/c8edbb87-c7ec-4500-a461-71e8912d25e9/99ba493d660597490cbb8b3211d2cae4/vc_redist.x86.exe";

const VC_REDIST_X86_SHA256: &str =
    "3a43e8a55a3f3e4b73d01872c16d47a19dd825756784f4580187309e7d1fcb74";

const VC_REDIST_X64_URL: &str =
    "https://download.visualstudio.microsoft.com/download/pr/9e04d214-5a9d-4515-9960-3d71398d98c3/1e1e62ab57bbb4bf5199e8ce88f040be/vc_redist.x64.exe";

const VC_REDIST_X64_SHA256: &str =
    "d6cd2445f68815fe02489fafe0127819e44851e26dfbe702612bc0d223cbbc2b";

lazy_static! {
    static ref HANDLEBARS: Handlebars<'static> = {
        let mut handlebars = Handlebars::new();

        handlebars
            .register_template_string("main.wxs", include_str!("templates/wix/main.wxs"))
            .unwrap();

        handlebars
            .register_template_string("bundle.wxs", include_str!("templates/wix/bundle.wxs"))
            .unwrap();

        handlebars
    };
}

/// Compute the `Id` of a directory.
fn directory_to_id(prefix: &str, path: &Path) -> String {
    format!(
        "{}.dir.{}",
        prefix,
        path.to_string_lossy().replace('/', ".").replace('-', "_")
    )
}

const GUID_NAMESPACE: &str = "https://github.com/indygreg/PyOxidizer/tugger/wix";

/// Compute the GUID of a component.
fn component_guid(prefix: &str, path: &OsStr) -> String {
    Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!(
            "{}/{}/component/{}",
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

fn component_id(prefix: &str, path: &OsStr) -> String {
    let guid = component_guid(prefix, path);

    format!("{}.component.{}", prefix, guid.to_lowercase())
}

fn file_guid(prefix: &str, path: &OsStr) -> String {
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

fn file_id(prefix: &str, path: &OsStr) -> String {
    let guid = file_guid(prefix, path);

    format!("{}.file.{}", prefix, guid.to_lowercase().replace('-', "_"))
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
/// to materialize all files defined by this manifest/wxs file.
///
/// `directory_id_prefix` defines a string prefix for `<DirectoryRef Id="..."`
/// values. The IDs will have the form `<directory_id_prefix>.<relative_directory>`,
/// with some normalization (e.g. `/` is normalized to `.` and `-` to `_`).
///
/// `component_id_prefix` defines a string prefix for `<Component Id="..."`
/// values.
fn write_file_manifest_to_wix<W: Write, P: AsRef<Path>>(
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
        writer.write(XmlEvent::start_element("DirectoryRef").attr("id", &parent_directory_id))?;

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
            let guid = component_guid(id_prefix, filename);
            let id = component_id(id_prefix, filename);

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
    }

    // </Wix>
    writer.write(XmlEvent::end_element())?;

    Ok(())
}

fn target_triple_to_wix_arch(triple: &str) -> &'static str {
    if triple.contains("x86_64") {
        "x64"
    } else {
        "x86"
    }
}

/// Entity used to build .msi installers using WiX.
pub struct WiXInstallerBuilder {
    /// Rust target triple we are building for.
    target_triple: String,

    /// Files to install in primary install location.
    install_files: FileManifest,

    /// Keys to define in the preprocessor when running candle.
    preprocess_parameters: BTreeMap<String, String>,

    /// Variables to define when running light.
    variables: BTreeMap<String, Option<String>>,
}

impl WiXInstallerBuilder {
    /// Create a new instance.
    pub fn new(target_triple: String) -> Self {
        Self {
            target_triple,
            install_files: FileManifest::default(),
            preprocess_parameters: BTreeMap::new(),
            variables: BTreeMap::new(),
        }
    }

    /// Set a preprocessor parameter value.
    ///
    /// These are passed to `candle.exe`.
    pub fn set_preprocessor_parameter<S: ToString>(&mut self, key: S, value: S) {
        self.preprocess_parameters
            .insert(key.to_string(), value.to_string());
    }

    /// Set a WiX variable with an optional value.
    ///
    /// These are passed to `light.exe`.
    pub fn set_variable<S: ToString>(&mut self, key: S, value: Option<S>) {
        self.variables
            .insert(key.to_string(), value.map(|x| x.to_string()));
    }

    /// Produce an MSI installer using the configuration in this builder.
    pub fn build_msi<P: AsRef<Path>>(&self, logger: &slog::Logger, build_path: P) -> Result<()> {
        let build_path = build_path.as_ref();

        let wix_toolset_path = build_path.join("wix-toolset");
        extract_wix(logger, &wix_toolset_path)?;

        // Materialize FileManifest so we can reference files from WiX.
        let stage_path = build_path.join("staged_files");
        self.install_files.write_to_path(&stage_path)?;

        let wxs_path = build_path.join("wxs");
        let mut emitter_config = EmitterConfig::new();
        emitter_config.perform_indent = true;

        let files_wxs_path = wxs_path.join("install_files.wxs");
        {
            let fh = std::fs::File::create(&files_wxs_path)?;
            let mut emitter = emitter_config.create_writer(fh);
            write_file_manifest_to_wix(
                &mut emitter,
                &self.install_files,
                &stage_path,
                "foo",
                "bar",
            )?;
        }

        let mut wixobj_paths = vec![];

        wixobj_paths.push(run_candle(
            logger,
            &wix_toolset_path,
            &files_wxs_path,
            target_triple_to_wix_arch(&self.target_triple),
            self.preprocess_parameters.iter(),
            None,
        )?);

        run_light(
            logger,
            &wix_toolset_path,
            build_path,
            wixobj_paths.iter(),
            self.variables.iter().map(|(k, v)| (k.clone(), v.clone())),
        )?;

        Ok(())
    }
}

/// Entity used to build a WiX bundle installer.
///
/// Bundle installers have multiple components in them.
#[derive(Default)]
pub struct WiXBundleInstallerBuilder {
    /// Name of the bundle.
    name: String,

    /// Version of the application.
    version: String,

    /// Manufacturer string.
    manufacturer: String,

    /// UUID upgrade code.
    upgrade_code: Option<String>,

    /// Whether to include an x86 Visual C++ Redistributable.
    include_vc_redist_x86: bool,

    /// Whether to include an amd64 Visual C++ Redistributable.
    include_vc_redist_x64: bool,

    /// Keys to define in the preprocessor when running candle.
    preprocess_parameters: BTreeMap<String, String>,

    /// Variables to define when running light.
    variables: BTreeMap<String, Option<String>>,
}

impl WiXBundleInstallerBuilder {
    pub fn new(name: String, version: String, manufacturer: String) -> Self {
        Self {
            name,
            version,
            manufacturer,
            ..Self::default()
        }
    }

    fn upgrade_code(&self) -> Cow<'_, str> {
        if let Some(code) = &self.upgrade_code {
            Cow::Borrowed(code)
        } else {
            Cow::Owned(
                Uuid::new_v5(
                    &Uuid::NAMESPACE_DNS,
                    format!("tugger.bundle.{}", &self.name).as_bytes(),
                )
                .to_string(),
            )
        }
    }

    /// Produce an executable containing defined components.
    pub fn build_exe<P: AsRef<Path>>(&self, logger: &slog::Logger, build_path: P) -> Result<()> {
        let build_path = build_path.as_ref();

        let wix_toolset_path = build_path.join("wix-toolset");
        extract_wix(logger, &wix_toolset_path)?;

        let redist_x86_path = build_path.join("vc_redist.x86.exe");
        let redist_x64_path = build_path.join("vc_redist.x64.exe");

        if self.include_vc_redist_x86 && !redist_x86_path.exists() {
            warn!(logger, "fetching Visual C++ Redistribution (x86)");
            let data = download_and_verify(logger, VC_REDIST_X86_URL, VC_REDIST_X86_SHA256)?;
            std::fs::write(&redist_x86_path, &data)?;
        }

        if self.include_vc_redist_x64 && !redist_x64_path.exists() {
            warn!(logger, "fetching Visual C++ Redistributable (x64)");
            let data = download_and_verify(logger, VC_REDIST_X64_URL, VC_REDIST_X64_SHA256)?;
            std::fs::write(&redist_x64_path, &data)?;
        }

        let mut emitter_config = EmitterConfig::new();
        emitter_config.perform_indent = true;

        let bundle_wxs_path = build_path.join("bundle.wxs");
        {
            let fh = std::fs::File::create(&bundle_wxs_path)?;
            let mut emitter = emitter_config.create_writer(fh);
            self.write_bundle_xml(&mut emitter)?;
        }

        let wixobj_paths = vec![run_candle(
            logger,
            &wix_toolset_path,
            &bundle_wxs_path,
            "x64",
            self.preprocess_parameters.iter(),
            None,
        )?];

        run_light(
            logger,
            &wix_toolset_path,
            build_path,
            wixobj_paths.iter(),
            self.variables.iter().map(|(k, v)| (k.clone(), v.clone())),
        )?;

        Ok(())
    }

    fn write_bundle_xml<W: Write>(&self, writer: &mut EventWriter<W>) -> Result<()> {
        writer.write(XmlEvent::StartDocument {
            version: XmlVersion::Version10,
            encoding: Some("utf-8"),
            standalone: None,
        })?;

        writer.write(
            XmlEvent::start_element("Wix")
                .default_ns("http://schemas.microsoft.com/wix/2006/wi")
                .ns("bal", "http://schemas.microsoft.com/wix/BalExtension")
                .ns("util", "http://schemas.microsoft.com/wix/UtilExtension"),
        )?;

        // TODO Condition?
        writer.write(
            XmlEvent::start_element("Bundle")
                .attr("Name", &self.name)
                .attr("Version", &self.version)
                .attr("Manufacturer", &self.manufacturer)
                .attr("UpgradeCode", self.upgrade_code().as_ref()),
        )?;

        writer.write(
            XmlEvent::start_element("BootstrapperApplicationRef")
                .attr("Id", "WixStandardBootstrapperApplication.HyperlinkLicense"),
        )?;

        writer.write(
            XmlEvent::start_element("bal:WixStandardBootstrapperApplication")
                .attr("LicenseUrl", "")
                .attr("SuppressOptionsUI", "yes"),
        )?;
        writer.write(XmlEvent::end_element())?;

        // </BootstrapperApplicationRef>
        writer.write(XmlEvent::end_element())?;

        writer.write(XmlEvent::start_element("Chain"))?;

        if self.include_vc_redist_x86 {
            writer.write(
                XmlEvent::start_element("ExePackage")
                    .attr("Id", "vc_redist.x86.exe")
                    .attr("Cache", "no")
                    .attr("Compressed", "yes")
                    .attr("PerMachine", "yes")
                    .attr("Permanent", "yes")
                    .attr("InstallCondition", "Not VersionNT64")
                    .attr("InstallCommand", "/install /quiet /norestart")
                    .attr("RepairCommand", "/repair /quiet /norestart")
                    .attr("UninstallCommand", "/uninstall /quiet /norestart"),
            )?;

            // </ExePackage>
            writer.write(XmlEvent::end_element())?;
        }

        if self.include_vc_redist_x64 {
            writer.write(
                XmlEvent::start_element("ExePackage")
                    .attr("Id", "vc_redist.x64.exe")
                    .attr("Cache", "no")
                    .attr("Compressed", "yes")
                    .attr("PerMachine", "yes")
                    .attr("Permanent", "yes")
                    .attr("InstallCondition", "VersionNT64")
                    .attr("InstallCommand", "/install /quiet /norestart")
                    .attr("RepairCommand", "/repair /quiet /norestart")
                    .attr("UninstallCommand", "/uninstall /quiet /norestart"),
            )?;

            // </ExePackage>
            writer.write(XmlEvent::end_element())?;
        }

        // </Chain>
        writer.write(XmlEvent::end_element())?;
        // </Bundle>
        writer.write(XmlEvent::end_element())?;
        // </Wix>
        writer.write(XmlEvent::end_element())?;

        Ok(())
    }
}

fn extract_wix<P: AsRef<Path>>(logger: &slog::Logger, path: P) -> Result<()> {
    warn!(logger, "downloading WiX Toolset...");
    let data = download_and_verify(logger, TOOLSET_URL, TOOLSET_SHA256)?;
    let cursor = std::io::Cursor::new(data);
    warn!(logger, "extracting WiX...");
    extract_zip(cursor, path)
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
fn run_candle<P: AsRef<Path>, S: AsRef<str>>(
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
fn run_light<P1: AsRef<Path>, P2: AsRef<Path>, P3: AsRef<Path>, S: AsRef<str>>(
    logger: &slog::Logger,
    wix_toolset_path: P1,
    build_path: P2,
    wixobjs: impl Iterator<Item = P3>,
    variables: impl Iterator<Item = (S, Option<S>)>,
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

    warn!(logger, "running light");

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

/*
pub fn build_wix_app_installer(
    logger: &slog::Logger,
    context: &BuildContext,
    wix_config: &DistributionWixInstaller,
    wix_toolset_path: &Path,
) -> Result<()> {
    let arch = match context.target_triple.as_str() {
        "i686-pc-windows-msvc" => "x86",
        "x86_64-pc-windows-msvc" => "x64",
        target => return Err(anyhow!("unhandled target triple: {}", target)),
    };

    let output_path = context.build_path.join("wix").join(arch);

    let mut data = BTreeMap::new();
    data.insert("product_name", &context.app_name);

    let cargo_package = context
        .cargo_config
        .package
        .clone()
        .ok_or_else(|| anyhow!("no [package] found in Cargo.toml"))?;

    data.insert("version", &cargo_package.version);

    let manufacturer =
        xml::escape::escape_str_attribute(&cargo_package.authors.join(", ")).to_string();
    data.insert("manufacturer", &manufacturer);

    let upgrade_code = if arch == "x86" {
        if let Some(ref code) = wix_config.msi_upgrade_code_x86 {
            code.clone()
        } else {
            uuid::Uuid::new_v5(
                &uuid::Uuid::NAMESPACE_DNS,
                format!("pyoxidizer.{}.app.x86", context.app_name).as_bytes(),
            )
            .to_string()
        }
    } else if arch == "x64" {
        if let Some(ref code) = wix_config.msi_upgrade_code_amd64 {
            code.clone()
        } else {
            uuid::Uuid::new_v5(
                &uuid::Uuid::NAMESPACE_DNS,
                format!("pyoxidizer.{}.app.x64", context.app_name).as_bytes(),
            )
            .to_string()
        }
    } else {
        panic!("unhandled arch: {}", arch);
    };

    data.insert("upgrade_code", &upgrade_code);

    let path_component_guid = uuid::Uuid::new_v4().to_string();
    data.insert("path_component_guid", &path_component_guid);

    let app_exe_name = context
        .app_exe_path
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_string();
    data.insert("app_exe_name", &app_exe_name);

    let app_exe_source = context.app_exe_path.display().to_string();
    data.insert("app_exe_source", &app_exe_source);

    let t = HANDLEBARS.render("main.wxs", &data)?;

    if output_path.exists() {
        std::fs::remove_dir_all(&output_path)?;
    }

    std::fs::create_dir_all(&output_path)?;

    let main_wxs_path = output_path.join("main.wxs");
    std::fs::write(&main_wxs_path, t)?;

    run_heat(
        logger,
        &wix_toolset_path,
        &output_path,
        &context.app_path,
        arch,
    )?;

    let input_basenames = vec!["main", "appdir"];

    // compile the .wxs files into .wixobj with candle.
    for basename in &input_basenames {
        let wxs = format!("{}.wxs", basename);
        run_candle(logger, context, &wix_toolset_path, &output_path, &wxs)?;
    }

    // First produce an MSI for our application.
    let wixobjs = vec!["main.wixobj", "appdir.wixobj"];
    run_light(
        logger,
        &wix_toolset_path,
        &output_path,
        &wixobjs,
        &app_installer_path(context),
    )?;

    Ok(())
}
*/

#[cfg(test)]
mod tests {
    use {super::*, crate::file_resource::FileContent};

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
        let xml = String::from_utf8(emitter.into_inner().into_inner()?)?;

        // TODO validate XML.

        Ok(())
    }
}
