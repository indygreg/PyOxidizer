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
    anyhow::Result,
    slog::warn,
    std::{borrow::Cow, collections::BTreeMap, io::Write, path::Path},
    uuid::Uuid,
    xml::{common::XmlVersion, writer::XmlEvent, EmitterConfig, EventWriter},
};

/// Entity used to build a WiX bundle installer.
///
/// Bundle installers have multiple components in them.
#[derive(Default)]
pub struct WiXBundleInstallerBuilder {
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

    /// Whether to include an x86 Visual C++ Redistributable.
    include_vc_redist_x86: bool,

    /// Whether to include an amd64 Visual C++ Redistributable.
    include_vc_redist_x64: bool,

    /// Whether to include an arm64 Visual C++ Redistributable.
    include_vc_redist_arm64: bool,

    /// Keys to define in the preprocessor when running candle.
    preprocess_parameters: BTreeMap<String, String>,
}

impl WiXBundleInstallerBuilder {
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

    /// Add this instance to a `WiXInstallerBuilder`.
    ///
    /// Requisite files will be downloaded and this instance will be converted to
    /// a wxs file and registered with the builder.
    pub fn add_to_installer_builder(
        &self,
        logger: &slog::Logger,
        builder: &mut WiXInstallerBuilder,
    ) -> Result<()> {
        let redist_x86_path = builder.build_path().join("vc_redist.x86.exe");
        let redist_x64_path = builder.build_path().join("vc_redist.x64.exe");
        let redist_arm64_path = builder.build_path().join("vc_redist.arm64.exe");

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

        let buffer = Vec::new();
        let writer = std::io::BufWriter::new(buffer);
        let mut emitter = emitter_config.create_writer(writer);
        self.write_bundle_xml(&mut emitter)?;

        let mut wxs =
            WxsBuilder::from_data(Path::new("bundle.wxs"), emitter.into_inner().into_inner()?);
        for (k, v) in &self.preprocess_parameters {
            wxs.set_preprocessor_parameter(k, v);
        }

        builder.add_wxs(wxs);

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
