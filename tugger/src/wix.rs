// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    crate::{
        file_resource::{FileContent, FileManifest},
        http::{download_to_path, RemoteContent},
        zipfile::extract_zip,
    },
    anyhow::{anyhow, Result},
    duct::cmd,
    handlebars::Handlebars,
    lazy_static::lazy_static,
    slog::warn,
    std::{
        borrow::Cow,
        collections::BTreeMap,
        convert::TryFrom,
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

lazy_static! {
    static ref WIX_TOOLSET: RemoteContent = RemoteContent {
        url: "https://github.com/wixtoolset/wix3/releases/download/wix3112rtm/wix311-binaries.zip"
            .to_string(),
        sha256: "2c1888d5d1dba377fc7fa14444cf556963747ff9a0a289a3599cf09da03b9e2e".to_string(),
    };

    // Latest versions of the VC++ Redistributable can be found at
    // https://support.microsoft.com/en-us/help/2977003/the-latest-supported-visual-c-downloads.
    // The download URL will redirect to a deterministic artifact, which is what we
    // record here.

    static ref VC_REDIST_X86: RemoteContent = RemoteContent {
        url: "https://download.visualstudio.microsoft.com/download/pr/48431a06-59c5-4b63-a102-20b66a521863/CAA38FD474164A38AB47AC1755C8CCCA5CCFACFA9A874F62609E6439924E87EC/VC_redist.x86.exe".to_string(),
        sha256: "caa38fd474164a38ab47ac1755c8ccca5ccfacfa9a874f62609e6439924e87ec".to_string(),
    };

    static ref VC_REDIST_X64: RemoteContent = RemoteContent {
        url: "https://download.visualstudio.microsoft.com/download/pr/48431a06-59c5-4b63-a102-20b66a521863/4B5890EB1AEFDF8DFA3234B5032147EB90F050C5758A80901B201AE969780107/VC_redist.x64.exe".to_string(),
        sha256: "4b5890eb1aefdf8dfa3234b5032147eb90f050c5758a80901b201ae969780107".to_string(),
    };

    static ref VC_REDIST_ARM64: RemoteContent = RemoteContent {
        url: "https://download.visualstudio.microsoft.com/download/pr/48431a06-59c5-4b63-a102-20b66a521863/A950A1C9DB37E2F784ABA98D484A4E0F77E58ED7CB57727672F9DC321015469E/VC_redist.arm64.exe".to_string(),
        sha256: "a950a1c9db37e2f784aba98d484a4e0f77e58ed7cb57727672f9dc321015469e".to_string(),
    };

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
fn component_guid(prefix: &str, path: &Path) -> String {
    Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!("{}/{}/component/{}", GUID_NAMESPACE, prefix, path.display()).as_bytes(),
    )
    .to_hyphenated()
    .encode_upper(&mut Uuid::encode_buffer())
    .to_string()
}

fn component_id(prefix: &str, path: &Path) -> String {
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

fn component_group_id(prefix: &str, path: &Path) -> String {
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
            None => component_group_id(id_prefix, Path::new(root_directory_id)),
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

fn target_triple_to_wix_arch(triple: &str) -> &'static str {
    if triple.contains("x86_64") {
        "x64"
    } else {
        "x86"
    }
}

/// Entity representing the build context for a .wxs file.
#[derive(Debug)]
pub struct WxsBuilder {
    /// Relative path/filename of this wxs file.
    path: PathBuf,

    /// Raw content of the wxs file.
    data: Vec<u8>,

    /// Keys to define in the preprocessor when running candle.
    preprocessor_parameters: BTreeMap<String, String>,
}

impl WxsBuilder {
    /// Create a new instance from data.
    pub fn from_data<P: AsRef<Path>>(path: P, data: Vec<u8>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
            data,
            preprocessor_parameters: BTreeMap::new(),
        }
    }

    /// Create a new instance from a filesystem file.
    pub fn from_path<P: AsRef<Path>>(path: P) -> Result<Self> {
        let filename = path
            .as_ref()
            .file_name()
            .ok_or_else(|| anyhow!("unable to determine filename"))?;

        let content = FileContent::try_from(path.as_ref())?;

        Ok(Self {
            path: PathBuf::from(filename),
            data: content.data,
            preprocessor_parameters: BTreeMap::new(),
        })
    }

    /// Set a preprocessor parameter value.
    ///
    /// These are passed to `candle.exe`.
    pub fn set_preprocessor_parameter<S: ToString>(&mut self, key: S, value: S) {
        self.preprocessor_parameters
            .insert(key.to_string(), value.to_string());
    }
}

/// Entity used to build .msi installers using WiX.
pub struct WiXInstallerBuilder {
    /// Prefix to use in generated WiX IDs.
    ///
    /// This is also used to derive the GUID for autogenerated
    /// components. It should uniquely identify the application/installer.
    id_prefix: String,

    /// Rust target triple we are building for.
    target_triple: String,

    /// Files to install in primary install location.
    install_files: FileManifest,

    /// Directory to use for build state.
    build_path: PathBuf,

    /// Variables to define when running light.
    variables: BTreeMap<String, Option<String>>,

    /// wxs files defining the WiX installer.
    ///
    /// These files will be materialized and processed when building.
    wxs_files: BTreeMap<PathBuf, WxsBuilder>,
}

impl WiXInstallerBuilder {
    /// Create a new instance.
    pub fn new<P: AsRef<Path>>(id_prefix: String, target_triple: String, build_path: P) -> Self {
        Self {
            id_prefix,
            target_triple,
            build_path: build_path.as_ref().to_path_buf(),
            install_files: FileManifest::default(),
            variables: BTreeMap::new(),
            wxs_files: BTreeMap::new(),
        }
    }

    /// Set a WiX variable with an optional value.
    ///
    /// These are passed to `light.exe`.
    pub fn set_variable<S: ToString>(&mut self, key: S, value: Option<S>) {
        self.variables
            .insert(key.to_string(), value.map(|x| x.to_string()));
    }

    /// Add a wxs file to build.
    pub fn add_wxs(&mut self, wxs: WxsBuilder) {
        self.wxs_files.insert(wxs.path.clone(), wxs);
    }

    /// Add a simple wxs file defining an installer.
    ///
    /// The wxs file is maintained as part of Tugger and contains defaults
    /// for simple program installs.
    pub fn add_simple_wxs(
        &mut self,
        product_name: &str,
        version: &str,
        manufacturer: &str,
    ) -> Result<()> {
        let mut data = BTreeMap::new();

        let upgrade_code = self.upgrade_code(product_name);

        data.insert(
            "product_name",
            xml::escape::escape_str_attribute(product_name),
        );
        data.insert(
            "upgrade_code",
            xml::escape::escape_str_attribute(&upgrade_code),
        );
        data.insert(
            "manufacturer",
            xml::escape::escape_str_attribute(manufacturer),
        );
        data.insert("version", xml::escape::escape_str_attribute(version));
        // path_component_guid

        let t = HANDLEBARS.render("main.wxs", &data)?;

        self.add_wxs(WxsBuilder::from_data(Path::new("main.wxs"), t.into_bytes()));
        self.add_files_manifest_wxs()?;

        Ok(())
    }

    fn stage_path(&self) -> PathBuf {
        self.build_path.join("staged_files")
    }

    /// Generate a wxs file containing fragments for all files registered for install.
    pub fn add_files_manifest_wxs(&mut self) -> Result<()> {
        let mut emitter_config = EmitterConfig::new();
        emitter_config.perform_indent = true;

        let buffer = Vec::new();
        let writer = std::io::BufWriter::new(buffer);
        let mut emitter = emitter_config.create_writer(writer);
        write_file_manifest_to_wix(
            &mut emitter,
            &self.install_files,
            &self.stage_path(),
            "ROOT",
            &self.id_prefix,
        )?;

        self.add_wxs(WxsBuilder::from_data(
            Path::new("install-files.wxs"),
            emitter.into_inner().into_inner()?,
        ));

        Ok(())
    }

    /// Produce an MSI installer using the configuration in this builder.
    pub fn build_msi<P: AsRef<Path>>(&self, logger: &slog::Logger, output_path: P) -> Result<()> {
        let wix_toolset_path = self.build_path.join("wix-toolset");
        extract_wix(logger, &wix_toolset_path)?;

        // Materialize FileManifest so we can reference files from WiX.
        self.install_files.write_to_path(&self.stage_path())?;

        let wxs_path = self.build_path.join("wxs");

        let mut wixobj_paths = Vec::new();

        for (path, wxs) in &self.wxs_files {
            let dest_path = wxs_path.join(path);
            let parent = dest_path
                .parent()
                .ok_or_else(|| anyhow!("could not determine parent directory"))?;
            if !parent.exists() {
                std::fs::create_dir_all(parent)?;
            }

            std::fs::write(&dest_path, &wxs.data)?;

            wixobj_paths.push(run_candle(
                logger,
                &wix_toolset_path,
                &dest_path,
                target_triple_to_wix_arch(&self.target_triple),
                wxs.preprocessor_parameters.iter(),
                None,
            )?);
        }

        run_light(
            logger,
            &wix_toolset_path,
            &self.build_path,
            wixobj_paths.iter(),
            self.variables.iter().map(|(k, v)| (k.clone(), v.clone())),
            output_path,
        )?;

        Ok(())
    }

    fn upgrade_code(&self, name: &str) -> String {
        Uuid::new_v5(
            &Uuid::NAMESPACE_DNS,
            format!("tugger.installer.{}.{}", name, &self.target_triple).as_bytes(),
        )
        .to_string()
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

    /// Whether to include an arm64 Visual C++ Redistributable.
    include_vc_redist_arm64: bool,

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
    pub fn build_exe<P: AsRef<Path>>(
        &self,
        logger: &slog::Logger,
        build_path: P,
        output_path: P,
    ) -> Result<()> {
        let build_path = build_path.as_ref();

        let wix_toolset_path = build_path.join("wix-toolset");
        extract_wix(logger, &wix_toolset_path)?;

        let redist_x86_path = build_path.join("vc_redist.x86.exe");
        let redist_x64_path = build_path.join("vc_redist.x64.exe");
        let redist_arm64_path = build_path.join("vc_redist.arm64.exe");

        if self.include_vc_redist_x86 {
            warn!(logger, "fetching Visual C++ Redistribution (x86)");
            download_to_path(logger, &VC_REDIST_X86, &redist_x86_path)?;
        }

        if self.include_vc_redist_x64 {
            warn!(logger, "fetching Visual C++ Redistributable (x64)");
            download_to_path(logger, &VC_REDIST_X64, &redist_x64_path)?;
        }

        if self.include_vc_redist_arm64 {
            warn!(logger, "fetching Visual C++ Redistribution (arm64)");
            download_to_path(logger, &VC_REDIST_ARM64, &redist_arm64_path)?;
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
            output_path,
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

        if self.include_vc_redist_arm64 {
            writer.write(
                XmlEvent::start_element("ExePackage")
                    .attr("Id", "vc_redist.arm64.exe")
                    .attr("Cache", "no")
                    .attr("Compressed", "yes")
                    .attr("PerMachine", "yes")
                    .attr("Permanent", "yes")
                    // TODO properly detect ARM64 here.
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
    let dest_path = path
        .as_ref()
        .parent()
        .ok_or_else(|| anyhow!("unable to resolve parent directory"))?
        .join("wix-toolset.zip");
    download_to_path(logger, &WIX_TOOLSET, &dest_path)?;
    let fh = std::fs::File::open(&dest_path)?;
    let cursor = std::io::BufReader::new(fh);
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
fn run_light<P1: AsRef<Path>, P2: AsRef<Path>, P3: AsRef<Path>, P4: AsRef<Path>, S: AsRef<str>>(
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

#[cfg(test)]
mod tests {
    use {super::*, crate::file_resource::FileContent, crate::testutil::*};

    #[test]
    fn test_wix_download() -> Result<()> {
        let logger = get_logger()?;

        extract_wix(&logger, &DEFAULT_DOWNLOAD_DIR.join("wix-toolset"))?;

        Ok(())
    }

    #[test]
    fn test_vcredist_download() -> Result<()> {
        let logger = get_logger()?;

        download_to_path(
            &logger,
            &VC_REDIST_X86,
            &DEFAULT_DOWNLOAD_DIR.join("vc_redist.x86.exe"),
        )?;
        download_to_path(
            &logger,
            &VC_REDIST_X64,
            &DEFAULT_DOWNLOAD_DIR.join("vc_redist.x64.exe"),
        )?;
        download_to_path(
            &logger,
            &VC_REDIST_ARM64,
            &DEFAULT_DOWNLOAD_DIR.join("vc_redist.arm64.exe"),
        )?;

        Ok(())
    }

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

    #[cfg(windows)]
    #[test]
    fn test_installer_builder_simple() -> Result<()> {
        let temp_dir = tempdir::TempDir::new("tugger-test")?;

        let logger = get_logger()?;

        let mut builder = WiXInstallerBuilder::new(
            "testapp".to_string(),
            env!("HOST").to_string(),
            temp_dir.path(),
        );
        builder.add_simple_wxs("testapp", "0.1", "manufacturer")?;

        let output_path = temp_dir.path().join("test.msi");

        builder.build_msi(&logger, &output_path)?;

        Ok(())
    }
}
