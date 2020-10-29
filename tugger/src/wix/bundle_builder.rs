// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    crate::{
        http::download_to_path,
        wix::{
            common::{VC_REDIST_ARM64, VC_REDIST_X64, VC_REDIST_X86},
            *,
        },
    },
    anyhow::{anyhow, Context, Result},
    slog::warn,
    std::{
        borrow::Cow,
        collections::BTreeMap,
        fmt::{Display, Formatter},
        io::Write,
        ops::Deref,
        path::Path,
    },
    uuid::Uuid,
    xml::{common::XmlVersion, writer::XmlEvent, EmitterConfig, EventWriter},
};

/// Available VC++ Redistributable platforms we can add to the bundle.
pub enum VCRedistributablePlatform {
    X86,
    X64,
    Arm64,
}

impl Display for VCRedistributablePlatform {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::X86 => "x86",
            Self::X64 => "x64",
            Self::Arm64 => "arm64",
        })
    }
}

/// Entity used to build a WiX bundle installer.
///
/// Bundle installers have multiple components in them.
#[derive(Default)]
pub struct WiXBundleInstallerBuilder<'a> {
    /// Name of the bundle.
    bundle_name: String,

    /// Version of the application.
    bundle_version: String,

    /// Manufacturer string.
    bundle_manufacturer: String,

    bundle_condition: Option<String>,

    /// UUID upgrade code.
    upgrade_code: Option<String>,

    /// Conditions that must be met to perform the install.
    conditions: Vec<(String, String)>,

    /// Keys to define in the preprocessor when running candle.
    preprocess_parameters: BTreeMap<String, String>,

    chain: Vec<ChainElement<'a>>,
}

impl<'a> WiXBundleInstallerBuilder<'a> {
    pub fn new(name: String, version: String, manufacturer: String) -> Self {
        Self {
            bundle_name: name,
            bundle_version: version,
            bundle_manufacturer: manufacturer,
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
                    format!("tugger.bundle.{}", &self.bundle_name).as_bytes(),
                )
                .to_string(),
            )
        }
    }

    /// Define a `<bal:Condition>` that must be satisfied to run this installer.
    ///
    /// `message` is the message that will be displayed if the condition is not met.
    /// `condition` is the condition expression. e.g. `VersionNT = v8.0`.
    pub fn add_condition(&mut self, message: &str, condition: &str) {
        self.conditions
            .push((message.to_string(), condition.to_string()));
    }

    pub fn add_vc_redistributable<P: AsRef<Path>>(
        &mut self,
        logger: &slog::Logger,
        platform: VCRedistributablePlatform,
        download_path: P,
    ) -> Result<()> {
        let (entry, install_condition) = match platform {
            VCRedistributablePlatform::X86 => (VC_REDIST_X86.deref(), "Not VersionNT64"),
            VCRedistributablePlatform::X64 => (VC_REDIST_X64.deref(), "VersionNT64"),
            VCRedistributablePlatform::Arm64 => {
                // TODO define proper Arm64 install condition.
                (VC_REDIST_ARM64.deref(), "VersionNT64 And Not VersionNT64")
            }
        };

        let url = url::Url::parse(&entry.url)?;
        let filename = url
            .path_segments()
            .ok_or_else(|| anyhow!("could not obtain path segments"))?
            .last()
            .ok_or_else(|| anyhow!("could not obtain final path segment"))?
            .to_string();

        let dest_path = download_path.as_ref().join(&filename);
        warn!(
            logger,
            "fetching Visual C++ Redistributable ({}) to {}",
            platform,
            dest_path.display()
        );
        download_to_path(logger, entry, &dest_path).context("downloading VC++ Redistributable")?;

        self.chain(
            ExePackage {
                id: Some(filename.clone().into()),
                name: Some(filename.into()),
                source_file: Some(dest_path.display().to_string().into()),
                cache: Some("no".into()),
                compressed: Some("yes".into()),
                per_machine: Some("yes".into()),
                permanent: Some("yes".into()),
                install_condition: Some(install_condition.into()),
                install_command: Some("/install /quiet /norestart".into()),
                repair_command: Some("/repair /quiet /norestart".into()),
                uninstall_command: Some("/uninstall /quiet /norestart".into()),
                ..ExePackage::default()
            }
            .into(),
        );

        Ok(())
    }

    /// Add an installable item to the `<Chain>`.
    pub fn chain(&mut self, item: ChainElement<'a>) {
        self.chain.push(item);
    }

    /// Add this instance to a `WiXInstallerBuilder`.
    ///
    /// Requisite files will be downloaded and this instance will be converted to
    /// a wxs file and registered with the builder.
    pub fn add_to_installer_builder(&self, builder: &mut WiXInstallerBuilder) -> Result<()> {
        let mut emitter_config = EmitterConfig::new();
        emitter_config.perform_indent = true;

        let buffer = Vec::new();
        let writer = std::io::BufWriter::new(buffer);
        let mut emitter = emitter_config.create_writer(writer);
        self.write_xml(&mut emitter)?;

        let mut wxs =
            WxsBuilder::from_data(Path::new("main.wxs"), emitter.into_inner().into_inner()?);
        for (k, v) in &self.preprocess_parameters {
            wxs.set_preprocessor_parameter(k, v);
        }

        builder.add_wxs(wxs);

        Ok(())
    }

    pub fn to_installer_builder<P: AsRef<Path>>(
        &self,
        id_prefix: &str,
        target_triple: &str,
        build_path: P,
    ) -> Result<WiXInstallerBuilder> {
        let mut builder = WiXInstallerBuilder::new(
            id_prefix.to_string(),
            target_triple.to_string(),
            build_path.as_ref().to_path_buf(),
        );

        self.add_to_installer_builder(&mut builder)?;

        Ok(builder)
    }

    fn write_xml<W: Write>(&self, writer: &mut EventWriter<W>) -> Result<()> {
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

        let upgrade_code = self.upgrade_code();
        let bundle = XmlEvent::start_element("Bundle")
            .attr("Name", &self.bundle_name)
            .attr("Version", &self.bundle_version)
            .attr("Manufacturer", &self.bundle_manufacturer)
            .attr("UpgradeCode", upgrade_code.as_ref());

        let bundle = if let Some(value) = &self.bundle_condition {
            bundle.attr("Condition", value)
        } else {
            bundle
        };

        writer.write(bundle)?;

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

        for (message, condition) in &self.conditions {
            writer.write(XmlEvent::start_element("bal:Condition").attr("Message", message))?;
            writer.write(XmlEvent::CData(condition))?;
            writer.write(XmlEvent::end_element())?;
        }

        writer.write(XmlEvent::start_element("Chain"))?;

        for element in &self.chain {
            element.write_xml(writer)?;
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

#[cfg(test)]
mod tests {
    use {
        super::*,
        crate::{testutil::*, wix::WiXBundleInstallerBuilder},
    };

    #[test]
    fn test_add_vc_redistributable() -> Result<()> {
        let logger = get_logger()?;

        let mut bundle = WiXBundleInstallerBuilder::new(
            "myapp".to_string(),
            "0.1".to_string(),
            "author".to_string(),
        );

        bundle.add_vc_redistributable(
            &logger,
            VCRedistributablePlatform::X86,
            DEFAULT_DOWNLOAD_DIR.as_path(),
        )?;
        bundle.add_vc_redistributable(
            &logger,
            VCRedistributablePlatform::X64,
            DEFAULT_DOWNLOAD_DIR.as_path(),
        )?;
        bundle.add_vc_redistributable(
            &logger,
            VCRedistributablePlatform::Arm64,
            DEFAULT_DOWNLOAD_DIR.as_path(),
        )?;

        Ok(())
    }

    #[cfg(windows)]
    #[test]
    fn test_vc_redistributable_build() -> Result<()> {
        let temp_dir = tempdir::TempDir::new("tugger-test")?;
        let logger = get_logger()?;

        let mut bundle = WiXBundleInstallerBuilder::new(
            "myapp".to_string(),
            "0.1".to_string(),
            "author".to_string(),
        );

        bundle.add_vc_redistributable(
            &logger,
            VCRedistributablePlatform::X86,
            DEFAULT_DOWNLOAD_DIR.as_path(),
        )?;
        bundle.add_vc_redistributable(
            &logger,
            VCRedistributablePlatform::X64,
            DEFAULT_DOWNLOAD_DIR.as_path(),
        )?;

        let builder = bundle.to_installer_builder("myapp", env!("HOST"), temp_dir.path())?;
        let output_path = temp_dir.path().join("myapp.exe");
        builder.build(&logger, &output_path)?;

        assert!(output_path.exists());

        Ok(())
    }
}
