// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

#[allow(unused)]
mod code_hash;
#[allow(unused)]
mod macho;

use {
    crate::macho::{find_signature_data, parse_signature_data},
    clap::{App, AppSettings, Arg, ArgMatches, SubCommand},
    goblin::mach::{Mach, MachO},
    std::{io::Write, str::FromStr},
};

#[derive(Debug)]
enum AppError {
    UnknownCommand,
    BadArgument,
    Io(std::io::Error),
    Goblin(goblin::error::Error),
    MachOError(crate::macho::MachOParseError),
    NoCodeSignature,
    NoCmsData,
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BadArgument => f.write_str("bad argument"),
            Self::UnknownCommand => f.write_str("unknown command"),
            Self::Io(e) => f.write_fmt(format_args!("I/O error: {:?}", e)),
            Self::Goblin(e) => f.write_fmt(format_args!("error parsing binary: {:?}", e)),
            Self::MachOError(e) => f.write_fmt(format_args!("Mach-O parsing error: {:?}", e)),
            Self::NoCodeSignature => f.write_str("code signature data not found"),
            Self::NoCmsData => f.write_str("CMS data structure not found"),
        }
    }
}

impl std::error::Error for AppError {}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<goblin::error::Error> for AppError {
    fn from(e: goblin::error::Error) -> Self {
        Self::Goblin(e)
    }
}

impl From<crate::macho::MachOParseError> for AppError {
    fn from(e: crate::macho::MachOParseError) -> Self {
        Self::MachOError(e)
    }
}

fn extract_cms_blob(macho: &MachO, format: &str) -> Result<(), AppError> {
    let codesign_data = find_signature_data(macho)?.ok_or(AppError::NoCodeSignature)?;
    let signature = parse_signature_data(codesign_data)?;

    let cms_blob = signature.signature_data()?.ok_or(AppError::NoCmsData)?;

    if format == "ber" {
        std::io::stdout().write_all(cms_blob)?;
    } else if format == "pem" {
        print!(
            "{}",
            pem::encode(&pem::Pem {
                tag: "PKCS7".to_string(),
                contents: cms_blob.to_vec()
            })
        );
    } else {
        panic!("unhandled format: {}", format)
    };

    Ok(())
}

fn command_extract_cms_blob(args: &ArgMatches) -> Result<(), AppError> {
    let path = args.value_of("path").ok_or(AppError::BadArgument)?;
    let format = args.value_of("format").ok_or(AppError::BadArgument)?;

    let data = std::fs::read(path)?;

    let mach = Mach::parse(&data)?;

    match mach {
        Mach::Binary(macho) => extract_cms_blob(&macho, format),
        Mach::Fat(multiarch) => {
            let index = args.value_of("universal_index").unwrap();
            let index = usize::from_str(index).map_err(|_| AppError::BadArgument)?;

            eprintln!(
                "found fat/universal Mach-O binary with {} architectures; examining binary at index {}",
                multiarch.narches, index
            );

            let macho = multiarch.get(index)?;

            extract_cms_blob(&macho, format)
        }
    }
}

fn main_impl() -> Result<(), AppError> {
    let matches = App::new("Oxidized Apple Codesigning")
        .setting(AppSettings::ArgRequiredElseHelp)
        .version("0.1")
        .author("Gregory Szorc <gregory.szorc@gmail.com>")
        .about("Do things related to code signing of Apple binaries")
        .subcommand(
            SubCommand::with_name("extract-cms-blob")
                .about("Extracts a Cryptographic Message Syntax ASN.1 blob from a Mach-O binary")
                .arg(
                    Arg::with_name("path")
                        .required(true)
                        .help("Path to Mach-O binary to examine"),
                )
                .arg(
                    Arg::with_name("format")
                        .long("format")
                        .takes_value(true)
                        .possible_values(&["ber", "pem"])
                        .default_value("pem")
                        .help("Output format"),
                )
                .arg(
                    Arg::with_name("universal_index")
                        .long("universal-index")
                        .takes_value(true)
                        .default_value("0")
                        .help("Index of Mach-O binary to operate on within a universal/fat binary"),
                ),
        )
        .get_matches();

    match matches.subcommand() {
        ("extract-cms-blob", Some(args)) => command_extract_cms_blob(args),
        _ => Err(AppError::UnknownCommand),
    }
}

fn main() {
    let exit_code = match main_impl() {
        Ok(()) => 0,
        Err(err) => {
            eprintln!("Error: {:?}", err);
            1
        }
    };

    std::process::exit(exit_code)
}
