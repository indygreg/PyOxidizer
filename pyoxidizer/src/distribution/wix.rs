// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use handlebars::Handlebars;
use lazy_static::lazy_static;
use sha2::Digest;
use slog::warn;
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};

use crate::app_packaging::config::DistributionWixInstaller;
use crate::app_packaging::state::BuildContext;

const TOOLSET_URL: &str =
    "https://github.com/wixtoolset/wix3/releases/download/wix3111rtm/wix311-binaries.zip";
const TOOLSET_SHA256: &str = "37f0a533b0978a454efb5dc3bd3598becf9660aaf4287e55bf68ca6b527d051d";

const VC_REDIST_X86_URL: &str =
    "https://download.visualstudio.microsoft.com/download/pr/c8edbb87-c7ec-4500-a461-71e8912d25e9/99ba493d660597490cbb8b3211d2cae4/vc_redist.x86.exe";

const VC_REDIST_X86_SHA256: &str =
    "3a43e8a55a3f3e4b73d01872c16d47a19dd825756784f4580187309e7d1fcb74";

const VC_REDIST_X64_URL: &str =
    "https://download.visualstudio.microsoft.com/download/pr/9e04d214-5a9d-4515-9960-3d71398d98c3/1e1e62ab57bbb4bf5199e8ce88f040be/vc_redist.x64.exe";

const VC_REDIST_X64_SHA256: &str =
    "d6cd2445f68815fe02489fafe0127819e44851e26dfbe702612bc0d223cbbc2b";

lazy_static! {
    static ref HANDLEBARS: Handlebars = {
        let mut handlebars = Handlebars::new();

        handlebars
            .register_template_string("main.wxs", include_str!("templates/main.wxs"))
            .unwrap();

        handlebars
            .register_template_string("bundle.wxs", include_str!("templates/bundle.wxs"))
            .unwrap();

        handlebars
    };
}

fn download_and_verify(logger: &slog::Logger, url: &str, hash: &str) -> Result<Vec<u8>, String> {
    warn!(logger, "downloading {}", url);
    let mut response = reqwest::get(url).or_else(|e| Err(e.to_string()))?;

    let mut data: Vec<u8> = Vec::new();
    response
        .read_to_end(&mut data)
        .or_else(|e| Err(e.to_string()))?;

    warn!(logger, "validating hash...");
    let mut hasher = sha2::Sha256::new();
    hasher.input(&data);

    let url_hash = hasher.result().to_vec();
    let expected_hash = hex::decode(hash).or_else(|e| Err(e.to_string()))?;

    if expected_hash == url_hash {
        Ok(data)
    } else {
        Err("hash mismatch of downloaded file".to_string())
    }
}

fn extract_zip(data: &[u8], path: &Path) -> Result<(), String> {
    let cursor = std::io::Cursor::new(data);
    let mut za = zip::ZipArchive::new(cursor).or_else(|e| Err(e.to_string()))?;

    for i in 0..za.len() {
        let mut file = za.by_index(i).or_else(|e| Err(e.to_string()))?;

        let dest_path = path.join(file.name());
        let parent = dest_path.parent().unwrap();

        if !parent.exists() {
            std::fs::create_dir_all(parent).or_else(|e| Err(e.to_string()))?;
        }

        let mut b: Vec<u8> = Vec::new();
        file.read_to_end(&mut b).or_else(|e| Err(e.to_string()))?;
        let mut fh = std::fs::File::create(dest_path).unwrap();

        fh.write_all(&b).or_else(|e| Err(e.to_string()))?;
    }

    Ok(())
}

fn extract_wix(logger: &slog::Logger, path: &Path) -> Result<(), String> {
    warn!(logger, "downloading WiX Toolset...");
    let data = download_and_verify(logger, TOOLSET_URL, TOOLSET_SHA256)?;
    warn!(logger, "extracting WiX...");
    extract_zip(&data, path)
}

fn app_installer_path(context: &BuildContext) -> PathBuf {
    let arch = match context.target_triple.as_str() {
        "i686-pc-windows-msvc" => "x86",
        "x86_64-pc-windows-msvc" => "amd64",
        target => panic!("unsupported target: {}", target),
    };
    context
        .distributions_path
        .join(format!("{}.{}.msi", context.app_name, arch))
}

fn run_heat(
    logger: &slog::Logger,
    wix_toolset_path: &Path,
    build_path: &Path,
    harvest_dir: &Path,
    platform: &str,
) -> Result<(), String> {
    let mut args = vec!["dir"];

    let harvest_str = harvest_dir.display().to_string();

    args.push(&harvest_str);
    args.push("-nologo");
    args.push("-platform");
    args.push(platform);
    args.push("-cg");
    args.push("AppFiles");
    args.push("-dr");
    args.push("APPLICATIONFOLDER");
    args.push("-gg");
    args.push("-srd");
    args.push("-out");
    args.push("appdir.wxs");
    args.push("-var");
    args.push("var.SourceDir");

    let heat_exe = wix_toolset_path.join("heat.exe");

    let mut cmd = std::process::Command::new(&heat_exe)
        .args(&args)
        .stdout(std::process::Stdio::piped())
        .current_dir(build_path)
        .spawn()
        .expect("error running heat.exe");
    {
        let stdout = cmd.stdout.as_mut().unwrap();
        let reader = BufReader::new(stdout);

        for line in reader.lines() {
            warn!(logger, "{}", line.unwrap());
        }
    }

    let status = cmd.wait().unwrap();
    if status.success() {
        Ok(())
    } else {
        Err("error running light.exe".to_string())
    }
}

fn run_candle(
    logger: &slog::Logger,
    context: &BuildContext,
    wix_toolset_path: &Path,
    build_path: &Path,
    wxs_file_name: &str,
) -> Result<(), String> {
    let arch = match context.target_triple.as_str() {
        "i686-pc-windows-msvc" => "x86",
        "x86_64-pc-windows-msvc" => "x64",
        triple => return Err(format!("unhandled target triple: {}", triple)),
    };

    let args = vec![
        "-nologo".to_string(),
        "-ext".to_string(),
        "WixBalExtension".to_string(),
        "-ext".to_string(),
        "WixUtilExtension".to_string(),
        "-arch".to_string(),
        arch.to_string(),
        wxs_file_name.to_string(),
        format!("-dSourceDir={}", context.app_path.display()),
    ];

    let candle_exe = wix_toolset_path.join("candle.exe");
    warn!(logger, "running candle for {}", wxs_file_name);

    let mut cmd = std::process::Command::new(&candle_exe)
        .args(&args)
        .stdout(std::process::Stdio::piped())
        .current_dir(build_path)
        .spawn()
        .expect("error running candle.exe");
    {
        let stdout = cmd.stdout.as_mut().unwrap();
        let reader = BufReader::new(stdout);

        for line in reader.lines() {
            warn!(logger, "{}", line.unwrap());
        }
    }

    let status = cmd.wait().unwrap();
    if status.success() {
        Ok(())
    } else {
        Err("error running candle.exe".to_string())
    }
}

fn run_light(
    logger: &slog::Logger,
    wix_toolset_path: &Path,
    build_path: &Path,
    wixobjs: &[&str],
    output_path: &Path,
) -> Result<(), String> {
    let light_exe = wix_toolset_path.join("light.exe");

    let mut args: Vec<String> = vec![
        "-nologo".to_string(),
        "-ext".to_string(),
        "WixUIExtension".to_string(),
        "-ext".to_string(),
        "WixBalExtension".to_string(),
        "-ext".to_string(),
        "WixUtilExtension".to_string(),
        "-o".to_string(),
        output_path.display().to_string(),
    ];

    for p in wixobjs {
        args.push(p.to_string());
    }

    warn!(logger, "running light to produce {}", output_path.display());

    let mut cmd = std::process::Command::new(&light_exe)
        .args(&args)
        .stdout(std::process::Stdio::piped())
        .current_dir(build_path)
        .spawn()
        .expect("error running light.exe");
    {
        let stdout = cmd.stdout.as_mut().unwrap();
        let reader = BufReader::new(stdout);

        for line in reader.lines() {
            warn!(logger, "{}", line.unwrap());
        }
    }

    let status = cmd.wait().unwrap();
    if status.success() {
        Ok(())
    } else {
        Err("error running light.exe".to_string())
    }
}

pub fn build_wix_app_installer(
    logger: &slog::Logger,
    context: &BuildContext,
    wix_config: &DistributionWixInstaller,
    wix_toolset_path: &Path,
) -> Result<(), String> {
    let arch = match context.target_triple.as_str() {
        "i686-pc-windows-msvc" => "x86",
        "x86_64-pc-windows-msvc" => "x64",
        target => return Err(format!("unhandled target triple: {}", target)),
    };

    let output_path = context.build_path.join("wix").join(arch);

    let mut data = BTreeMap::new();
    data.insert("product_name", &context.app_name);

    let cargo_package = context
        .cargo_config
        .package
        .clone()
        .ok_or_else(|| "no [package] found in Cargo.toml".to_string())?;

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

    let t = HANDLEBARS
        .render("main.wxs", &data)
        .or_else(|e| Err(e.to_string()))?;

    if output_path.exists() {
        std::fs::remove_dir_all(&output_path).or_else(|e| Err(e.to_string()))?;
    }

    std::fs::create_dir_all(&output_path).or_else(|e| Err(e.to_string()))?;

    let main_wxs_path = output_path.join("main.wxs");
    std::fs::write(&main_wxs_path, t).or_else(|e| Err(e.to_string()))?;

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

pub fn build_wix_installer(
    logger: &slog::Logger,
    context: &BuildContext,
    wix_config: &DistributionWixInstaller,
) -> Result<(), String> {
    // The WiX installer is a unified installer for multiple architectures.
    // So ensure all Windows architectures are built before proceeding. This is
    // a bit hacky and should arguably be handled in a better way. Meh.
    let mut other_context = if context.target_triple == "x86_64-pc-windows-msvc" {
        warn!(logger, "building application for x86");
        super::super::projectmgmt::resolve_build_context(
            logger,
            context.project_path.to_str().unwrap(),
            Some(context.config_path.to_str().unwrap()),
            Some("i686-pc-windows-msvc"),
            true,
            None,
            false,
        )?
    } else if context.target_triple == "i686-pc-windows-msvc" {
        warn!(logger, "building application for x64");
        super::super::projectmgmt::resolve_build_context(
            logger,
            context.project_path.to_str().unwrap(),
            Some(context.config_path.to_str().unwrap()),
            Some("x86_64-pc-windows-msvc"),
            true,
            None,
            false,
        )?
    } else {
        return Err(format!(
            "building for unknown target: {}",
            context.target_triple
        ));
    };

    super::super::projectmgmt::build_project(logger, &mut other_context)?;
    super::super::pyrepackager::repackage::package_project(logger, &mut other_context)?;

    let wix_toolset_path = context.build_path.join("wix-toolset");
    if !wix_toolset_path.exists() {
        extract_wix(logger, &wix_toolset_path)?;
    }

    // Build the standalone MSI installers for the per-architecture application.
    build_wix_app_installer(logger, context, wix_config, &wix_toolset_path)?;
    build_wix_app_installer(logger, &other_context, wix_config, &wix_toolset_path)?;

    // Then build a bundler installer containing all architectures.

    let mut data = BTreeMap::new();
    data.insert("product_name", &context.app_name);

    let bundle_upgrade_code = if let Some(ref code) = wix_config.bundle_upgrade_code {
        code.clone()
    } else {
        uuid::Uuid::new_v5(
            &uuid::Uuid::NAMESPACE_DNS,
            format!("pyoxidizer.{}.bundle", context.app_name).as_bytes(),
        )
        .to_string()
    };
    data.insert("bundle_upgrade_code", &bundle_upgrade_code);

    let distributions_path_s = context.distributions_path.display().to_string();
    data.insert("distributions_path", &distributions_path_s);

    let redist_x86_path = context.build_path.join("vc_redist.x86.exe");
    let redist_x86_path_str = redist_x86_path.display().to_string();
    data.insert("vc_redist_x86_path", &redist_x86_path_str);

    let redist_x64_path = context.build_path.join("vc_redist.x64.exe");
    let redist_x64_path_str = redist_x64_path.display().to_string();
    data.insert("vc_redist_x64_path", &redist_x64_path_str);

    let t = HANDLEBARS
        .render("bundle.wxs", &data)
        .or_else(|e| Err(e.to_string()))?;

    let output_path = context.build_path.join("wix").join("bundle");

    if output_path.exists() {
        std::fs::remove_dir_all(&output_path).or_else(|e| Err(e.to_string()))?;
    }

    std::fs::create_dir_all(&output_path).or_else(|e| Err(e.to_string()))?;

    let bundle_wxs_path = output_path.join("bundle.wxs");
    std::fs::write(&bundle_wxs_path, t).or_else(|e| Err(e.to_string()))?;

    if !redist_x86_path.exists() {
        warn!(logger, "fetching Visual C++ Redistributable (x86)");
        let data = download_and_verify(logger, VC_REDIST_X86_URL, VC_REDIST_X86_SHA256)?;
        std::fs::write(&redist_x86_path, &data).or_else(|e| Err(e.to_string()))?;
    }

    if !redist_x64_path.exists() {
        warn!(logger, "fetching Visual C++ Redistributable (x64)");
        let data = download_and_verify(logger, VC_REDIST_X64_URL, VC_REDIST_X64_SHA256)?;
        std::fs::write(&redist_x64_path, &data).or_else(|e| Err(e.to_string()))?;
    }

    // Then produce a bundle installer for it.

    run_candle(
        logger,
        context,
        &wix_toolset_path,
        &output_path,
        "bundle.wxs",
    )?;

    let wixobjs = vec!["bundle.wixobj"];
    let bundle_installer_path = context
        .distributions_path
        .join(format!("{}.exe", context.app_name));
    run_light(
        logger,
        &wix_toolset_path,
        &output_path,
        &wixobjs,
        &bundle_installer_path,
    )?;

    Ok(())
}
