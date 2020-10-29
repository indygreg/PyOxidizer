// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    crate::{
        file_resource::FileManifest,
        wix::{WiXInstallerBuilder, WxsBuilder},
    },
    anyhow::Result,
    std::{
        borrow::Cow,
        io::Write,
        path::{Path, PathBuf},
    },
    uuid::Uuid,
    xml::{
        common::XmlVersion,
        writer::{EmitterConfig, EventWriter, XmlEvent},
    },
};

/// Entity used to emit a simple `.wxs` for building an msi installer.
///
/// Instances are constructed with mandatory fields, such as the
/// product name, version, and author.
///
/// Various optional fields can be provided and will be used in the
/// installer if provided.
///
/// The MSI installer will materialize registered files in the
/// `Program Files` directory on the target machine.
#[derive(Default)]
pub struct WiXSimpleMSIBuilder {
    id_prefix: String,
    product_name: String,
    product_version: String,
    product_manufacturer: String,
    product_codepage: String,
    product_language: String,

    package_languages: String,
    package_installer_version: String,

    /// Files to materialize in `Program Files`.
    program_files_manifest: FileManifest,

    upgrade_code: Option<String>,
    package_keywords: Option<String>,
    package_description: Option<String>,
    license_source: Option<PathBuf>,
    product_icon: Option<PathBuf>,
    help_url: Option<String>,
    eula_rtf: Option<PathBuf>,
    /// Banner BMP image.
    ///
    /// Dimensions are 493 x 58.
    banner_bmp: Option<PathBuf>,

    /// Dialog BMP image.
    ///
    /// Dimensions are 493 x 312.
    dialog_bmp: Option<PathBuf>,
}

impl WiXSimpleMSIBuilder {
    pub fn new(id_prefix: &str, product_name: &str, version: &str, manufacturer: &str) -> Self {
        Self {
            id_prefix: id_prefix.to_string(),
            product_name: product_name.to_string(),
            product_version: version.to_string(),
            product_manufacturer: manufacturer.to_string(),
            product_codepage: "1252".to_string(),
            product_language: "1033".to_string(),
            package_languages: "1033".to_string(),
            package_installer_version: "450".to_string(),
            ..Self::default()
        }
    }

    /// Add files to install to `Program Files` via a `FileManifest`.
    ///
    /// All files in the provided manifest will be materialized in `Program Files`
    /// by the built installer.
    pub fn add_program_files_manifest(&mut self, manifest: &FileManifest) -> Result<()> {
        self.program_files_manifest.add_manifest(manifest)
    }

    /// Set the `<Product UpgradeCode` attribute value.
    ///
    /// If not called, a deterministic value will be derived from the product name.
    pub fn upgrade_code(mut self, value: String) -> Self {
        self.upgrade_code = Some(value);
        self
    }

    /// Set the `<Package Keywords` attribute value.
    pub fn package_keywords(mut self, value: String) -> Self {
        self.package_keywords = Some(value);
        self
    }

    /// Set the `<Package Description` attribute value.
    pub fn package_description(mut self, value: String) -> Self {
        self.package_description = Some(value);
        self
    }

    /// Set the path to the file containing the license for this application.
    pub fn license_path<P: AsRef<Path>>(mut self, path: P) -> Self {
        self.license_source = Some(path.as_ref().to_path_buf());
        self
    }

    /// Set the path to the file containing the icon for this installer.
    pub fn product_icon_path<P: AsRef<Path>>(mut self, path: P) -> Self {
        self.product_icon = Some(path.as_ref().to_path_buf());
        self
    }

    /// Set the help URL for this application.
    pub fn help_url(mut self, value: String) -> Self {
        self.help_url = Some(value);
        self
    }

    /// Set the path to an rtf file containing the end user license agreement.
    pub fn eula_rtf_path<P: AsRef<Path>>(mut self, path: P) -> Self {
        self.eula_rtf = Some(path.as_ref().to_path_buf());
        self
    }

    /// Set the path to a bmp file containing a banner to use for install.
    ///
    /// The dimensions of the banner should be 493 x 58.
    pub fn banner_bmp_path<P: AsRef<Path>>(mut self, path: P) -> Self {
        self.banner_bmp = Some(path.as_ref().to_path_buf());
        self
    }

    /// Set the path to a bmp file containing an image for the install dialog.
    ///
    /// The dimensions of the image should be 493 x 312.
    pub fn dialog_bmp_path<P: AsRef<Path>>(mut self, path: P) -> Self {
        self.dialog_bmp = Some(path.as_ref().to_path_buf());
        self
    }

    /// Add this instance to a `WiXInstallerBuilder`.
    pub fn add_to_installer_builder(&self, builder: &mut WiXInstallerBuilder) -> Result<()> {
        let mut emitter_config = EmitterConfig::new();
        emitter_config.perform_indent = true;

        let buffer = Vec::new();
        let writer = std::io::BufWriter::new(buffer);
        let mut emitter = emitter_config.create_writer(writer);
        self.write_xml(&mut emitter)?;

        builder.add_wxs(WxsBuilder::from_data(
            Path::new("main.wxs"),
            emitter.into_inner().into_inner()?,
        ));

        builder.add_install_files_manifest(&self.program_files_manifest)?;
        builder.add_files_manifest_wxs("APPLICATIONFOLDER")?;

        Ok(())
    }

    /// Convert to a `WiXInstallerBuilder`.
    ///
    /// This will construct a new `WiXInstallerBuilder` suitable for building
    /// this msi installer.
    pub fn to_installer_builder<P: AsRef<Path>>(
        &self,
        target_triple: &str,
        build_path: P,
    ) -> Result<WiXInstallerBuilder> {
        let mut builder = WiXInstallerBuilder::new(
            self.id_prefix.clone(),
            target_triple.to_string(),
            build_path.as_ref().to_path_buf(),
        );

        self.add_to_installer_builder(&mut builder)?;

        Ok(builder)
    }

    /// Write XML describing this builder.
    pub fn write_xml<W: Write>(&self, writer: &mut EventWriter<W>) -> Result<()> {
        writer.write(XmlEvent::StartDocument {
            version: XmlVersion::Version10,
            encoding: Some("utf-8"),
            standalone: None,
        })?;

        writer.write(XmlEvent::ProcessingInstruction {
            name: "if",
            data: Some("$(sys.BUILDARCH) = x64 or $(sys.BUILDARCH) = arm64"),
        })?;
        writer.write(XmlEvent::ProcessingInstruction {
            name: "define",
            data: Some("Win64 = \"yes\""),
        })?;
        writer.write(XmlEvent::ProcessingInstruction {
            name: "define",
            data: Some("PlatformProgramFilesFolder = \"ProgramFiles64Folder\""),
        })?;
        writer.write(XmlEvent::ProcessingInstruction {
            name: "else",
            data: None,
        })?;
        writer.write(XmlEvent::ProcessingInstruction {
            name: "define",
            data: Some("Win64 = \"no\""),
        })?;
        writer.write(XmlEvent::ProcessingInstruction {
            name: "define",
            data: Some("PlatformProgramFilesFolder = \"ProgramFilesFolder\""),
        })?;
        writer.write(XmlEvent::ProcessingInstruction {
            name: "endif",
            data: None,
        })?;

        writer.write(
            XmlEvent::start_element("Wix").default_ns("http://schemas.microsoft.com/wix/2006/wi"),
        )?;

        writer.write(
            XmlEvent::start_element("Product")
                .attr("Id", "*")
                .attr("Name", &self.product_name)
                .attr("Version", &self.product_version)
                .attr("Manufacturer", &self.product_manufacturer)
                .attr("UpgradeCode", &self.get_upgrade_code())
                .attr("Language", &self.product_language)
                .attr("Codepage", &self.product_codepage),
        )?;

        let package = XmlEvent::start_element("Package")
            .attr("Id", "*")
            .attr("Manufacturer", &self.product_manufacturer)
            .attr("InstallerVersion", &self.package_installer_version)
            .attr("Languages", &self.package_languages)
            .attr("Compressed", "yes")
            .attr("InstallScope", "perMachine")
            .attr("SummaryCodepage", "1252")
            .attr("Platform", "$(sys.BUILDARCH)");

        let package = if let Some(keywords) = &self.package_keywords {
            package.attr("Keywords", keywords)
        } else {
            package
        };

        let package = if let Some(description) = &self.package_description {
            package.attr("Description", description)
        } else {
            package
        };
        writer.write(package)?;
        writer.write(XmlEvent::end_element().name("Package"))?;

        writer.write(
            XmlEvent::start_element("MajorUpgrade")
                .attr("Schedule", "afterInstallInitialize")
                .attr(
                    "DowngradeErrorMessage",
                    "A newer version of [ProductName] is already installed. Setup will now exit.",
                ),
        )?;
        writer.write(XmlEvent::end_element().name("MajorUpgrade"))?;

        writer.write(
            XmlEvent::start_element("Media")
                .attr("Id", "1")
                .attr("Cabinet", "media1.cab")
                .attr("EmbedCab", "yes")
                .attr("DiskPrompt", "CD-ROM #1"),
        )?;
        writer.write(XmlEvent::end_element().name("Media"))?;

        writer.write(
            XmlEvent::start_element("Property")
                .attr("Id", "DiskPrompt")
                .attr("Value", &format!("{} Installation", &self.product_name)),
        )?;
        writer.write(XmlEvent::end_element().name("Property"))?;

        writer.write(
            XmlEvent::start_element("Directory")
                .attr("Id", "TARGETDIR")
                .attr("Name", "SourceDir"),
        )?;
        writer.write(
            XmlEvent::start_element("Directory")
                .attr("Id", "$(var.PlatformProgramFilesFolder)")
                .attr("Name", "PFiles"),
        )?;
        writer.write(
            XmlEvent::start_element("Directory")
                .attr("Id", "APPLICATIONFOLDER")
                .attr("Name", &self.product_name),
        )?;

        writer.write(
            XmlEvent::start_element("Component")
                .attr("Id", "Path")
                .attr("Guid", &self.path_component_guid())
                .attr("Win64", "$(var.Win64)")
                .attr("KeyPath", "yes"),
        )?;
        writer.write(
            XmlEvent::start_element("Environment")
                .attr("Id", "PATH")
                .attr("Name", "PATH")
                .attr("Value", "[Bin]")
                .attr("Permanent", "no")
                .attr("Part", "last")
                .attr("Action", "set")
                .attr("System", "yes"),
        )?;
        writer.write(XmlEvent::end_element().name("Environment"))?;
        writer.write(XmlEvent::end_element().name("Component"))?;

        if let Some(license_source) = &self.license_source {
            writer.write(
                XmlEvent::start_element("Component")
                    .attr("Id", "License")
                    .attr("Guid", "*")
                    .attr("Win64", "$(var.Win64"),
            )?;

            writer.write(
                XmlEvent::start_element("File")
                    .attr("Id", "LicenseFile")
                    .attr("Name", "LicenseFile")
                    .attr("DiskID", "1")
                    .attr("Source", &license_source.display().to_string())
                    .attr("KeyPath", "yes"),
            )?;
            writer.write(XmlEvent::end_element().name("File"))?;

            writer.write(XmlEvent::end_element().name("Component"))?;
        }

        writer.write(XmlEvent::end_element().name("Directory"))?;
        writer.write(XmlEvent::end_element().name("Directory"))?;
        writer.write(XmlEvent::end_element().name("Directory"))?;

        writer.write(
            XmlEvent::start_element("Feature")
                .attr("Id", "MainProgram")
                .attr("Title", "Application")
                .attr("Description", "Installs all application files")
                .attr("Level", "1")
                .attr("ConfigurableDirectory", "APPLICATIONFOLDER")
                .attr("AllowAdvertise", "no")
                .attr("Display", "expand")
                .attr("Absent", "disallow"),
        )?;

        // Add group for all files derived from self.program_files_manifest.
        writer.write(
            XmlEvent::start_element("ComponentGroupRef")
                .attr("Id", &format!("{}.group.ROOT", self.id_prefix)),
        )?;
        writer.write(XmlEvent::end_element().name("ComponentGroupRef"))?;

        if self.license_source.is_some() {
            writer.write(XmlEvent::start_element("ComponentRef").attr("Id", "License"))?;
            writer.write(XmlEvent::end_element().name("ComponentRef"))?;
        }

        writer.write(
            XmlEvent::start_element("Feature")
                .attr("Id", "Environment")
                .attr("Title", "PATH Environment Variable")
                .attr(
                    "Description",
                    "Add the install location to the PATH system environment variable",
                )
                .attr("Level", "1")
                .attr("Absent", "allow"),
        )?;
        writer.write(XmlEvent::start_element("ComponentRef").attr("Id", "Path"))?;
        writer.write(XmlEvent::end_element().name("ComponentRef"))?;
        writer.write(XmlEvent::end_element().name("Feature"))?;

        writer.write(XmlEvent::end_element().name("Feature"))?;

        writer.write(
            XmlEvent::start_element("SetProperty")
                .attr("Id", "APPINSTALLLOCATION")
                .attr("Value", "[APPLICATIONFOLDER]")
                .attr("After", "CostFinalize"),
        )?;
        writer.write(XmlEvent::end_element().name("SetProperty"))?;

        if let Some(icon_path) = &self.product_icon {
            writer.write(
                XmlEvent::start_element("Icon")
                    .attr("Id", "ProductICO")
                    .attr("SourceFile", &icon_path.display().to_string()),
            )?;
            writer.write(XmlEvent::end_element().name("Icon"))?;

            writer.write(
                XmlEvent::start_element("Property")
                    .attr("Id", "ARPPRODUCTICON")
                    .attr("Value", "ProductICO"),
            )?;
            writer.write(XmlEvent::end_element().name("Property"))?;
        }

        if let Some(help_url) = &self.help_url {
            writer.write(
                XmlEvent::start_element("Property")
                    .attr("Id", "ARPHELPLINK")
                    .attr("Value", help_url),
            )?;
            writer.write(XmlEvent::end_element().name("Property"))?;
        }

        writer.write(XmlEvent::start_element("UI"))?;
        writer.write(XmlEvent::start_element("UIRef").attr("Id", "WixUI_FeatureTree"))?;
        writer.write(XmlEvent::end_element().name("UIRef"))?;

        if self.eula_rtf.is_none() {
            writer.write(
                XmlEvent::start_element("Publish")
                    .attr("Dialog", "WelcomeDlg")
                    .attr("Control", "Next")
                    .attr("Event", "NewDialog")
                    .attr("Value", "CustomizeDlg")
                    .attr("Order", "99"),
            )?;
            writer.write(XmlEvent::Characters("1"))?;
            writer.write(XmlEvent::end_element().name("Publish"))?;
            writer.write(
                XmlEvent::start_element("Publish")
                    .attr("Dialog", "CustomizeDlg")
                    .attr("Control", "Back")
                    .attr("Event", "NewDialog")
                    .attr("Value", "WelcomeDlg")
                    .attr("Order", "99"),
            )?;
            writer.write(XmlEvent::Characters("1"))?;
            writer.write(XmlEvent::end_element().name("Publish"))?;
        }

        writer.write(XmlEvent::end_element().name("UI"))?;

        if let Some(eula_path) = &self.eula_rtf {
            writer.write(
                XmlEvent::start_element("WixVariable")
                    .attr("Id", "WixUILicenseRTF")
                    .attr("Value", &eula_path.display().to_string()),
            )?;
            writer.write(XmlEvent::end_element().name("WixVariable"))?;
        }

        if let Some(banner_path) = &self.banner_bmp {
            writer.write(
                XmlEvent::start_element("WixVariable")
                    .attr("Id", "WixUIBannerBMP")
                    .attr("Value", &banner_path.display().to_string()),
            )?;
            writer.write(XmlEvent::end_element().name("WixVariable"))?;
        }

        if let Some(dialog_bmp) = &self.dialog_bmp {
            writer.write(
                XmlEvent::start_element("WixVariable")
                    .attr("Id", "WixUIDialogBmp")
                    .attr("Value", &dialog_bmp.display().to_string()),
            )?;
            writer.write(XmlEvent::end_element().name("WixVariable"))?;
        }

        writer.write(XmlEvent::end_element().name("Product"))?;
        writer.write(XmlEvent::end_element().name("Wix"))?;

        Ok(())
    }

    fn get_upgrade_code(&self) -> Cow<'_, str> {
        if let Some(v) = &self.upgrade_code {
            Cow::Borrowed(v)
        } else {
            Cow::Owned(
                Uuid::new_v5(
                    &Uuid::NAMESPACE_DNS,
                    format!("tugger.upgrade_code.{}", self.product_name).as_bytes(),
                )
                .to_string(),
            )
        }
    }

    fn path_component_guid(&self) -> String {
        Uuid::new_v5(
            &Uuid::NAMESPACE_DNS,
            format!("tugger.path_component.{}", self.product_name).as_bytes(),
        )
        .to_hyphenated()
        .encode_upper(&mut Uuid::encode_buffer())
        .to_string()
    }
}

#[cfg(test)]
mod tests {
    use {super::*, crate::file_resource::FileContent, crate::testutil::*};

    #[test]
    fn test_simple_msi_builder() -> Result<()> {
        let mut builder = WiXSimpleMSIBuilder::new("prefix", "myapp", "0.1", "author");

        let mut m = FileManifest::default();
        m.add_file(
            "foo.txt",
            &FileContent {
                data: vec![42],
                executable: false,
            },
        )?;

        builder.add_program_files_manifest(&m)?;

        let builder = builder.to_installer_builder(env!("HOST"), DEFAULT_TEMP_DIR.path())?;

        assert!(builder.wxs_files.contains_key(&PathBuf::from("main.wxs")));
        assert_eq!(builder.install_files, m);

        Ok(())
    }

    #[cfg(windows)]
    #[test]
    fn test_simple_msi_builder_build() -> Result<()> {
        let temp_dir = tempdir::TempDir::new("tugger-test")?;

        let logger = get_logger()?;

        let mut builder = WiXSimpleMSIBuilder::new("prefix", "testapp", "0.1", "author");

        let mut m = FileManifest::default();
        m.add_file(
            "foo.txt",
            &FileContent {
                data: vec![42],
                executable: false,
            },
        )?;

        builder.add_program_files_manifest(&m)?;

        let builder = builder.to_installer_builder(env!("HOST"), temp_dir.path())?;

        let output_path = temp_dir.path().join("test.msi");

        builder.build(&logger, &output_path)?;

        let package = msi::open(&output_path)?;

        let summary_info = package.summary_info();
        assert_eq!(summary_info.subject(), Some("testapp"));

        Ok(())
    }
}
