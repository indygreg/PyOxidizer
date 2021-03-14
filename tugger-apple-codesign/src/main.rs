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
    let signature = parse_signature_data(codesign_data.signature_data)?;

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

fn get_macho_from_data(data: &[u8], universal_index: usize) -> Result<MachO, AppError> {
    let mach = Mach::parse(data)?;

    match mach {
        Mach::Binary(macho) => Ok(macho),
        Mach::Fat(multiarch) => {
            eprintln!(
                "found fat/universal Mach-O binary with {} architectures; examining binary at index {}",
                multiarch.narches, universal_index
            );

            Ok(multiarch.get(universal_index)?)
        }
    }
}

fn command_extract_cms_blob(args: &ArgMatches) -> Result<(), AppError> {
    let path = args.value_of("path").ok_or(AppError::BadArgument)?;
    let format = args.value_of("format").ok_or(AppError::BadArgument)?;
    let index = args.value_of("universal_index").unwrap();
    let index = usize::from_str(index).map_err(|_| AppError::BadArgument)?;

    let data = std::fs::read(path)?;

    let macho = get_macho_from_data(&data, index)?;

    extract_cms_blob(&macho, format)
}

fn command_extract_macho_signature(args: &ArgMatches) -> Result<(), AppError> {
    let path = args.value_of("path").ok_or(AppError::BadArgument)?;
    let format = args.value_of("format").ok_or(AppError::BadArgument)?;
    let index = args.value_of("universal_index").unwrap();
    let index = usize::from_str(index).map_err(|_| AppError::BadArgument)?;

    let data = std::fs::read(path)?;

    let macho = get_macho_from_data(&data, index)?;

    let sig = find_signature_data(&macho)?.ok_or(AppError::NoCodeSignature)?;

    match format {
        "blobs" => {
            let embedded = parse_signature_data(&sig.signature_data)?;

            for blob in embedded.blobs {
                let parsed = blob.into_parsed_blob()?;
                println!("{:#?}", parsed);
            }
        }
        "code-directory" => {
            let embedded = parse_signature_data(&sig.signature_data)?;

            if let Some(cd) = embedded.code_directory()? {
                println!("{:#?}", cd);
            } else {
                eprintln!("no code directory");
            }
        }
        "linkedit-segment-raw" => {
            std::io::stdout().write_all(sig.linkedit_segment_data)?;
        }
        "requirements" => {
            let embedded = parse_signature_data(&sig.signature_data)?;

            if let Some(reqs) = embedded.requirements()? {
                println!("{:#?}", reqs)
            } else {
                eprintln!("no requirements");
            }
        }
        "segment-info" => {
            println!("segments count: {}", sig.segments_count);
            println!("__LINKEDIT segment index: {}", sig.linkedit_segment_index);
            println!(
                "__LINKEDIT segment size: {}",
                sig.linkedit_segment_data.len()
            );
            println!(
                "__LINKEDIT signature start offset: {}",
                sig.signature_start_offset
            );
            println!(
                "__LINKEDIT signature end offset: {}",
                sig.signature_end_offset
            );
            println!("__LINKEDIT signature size: {}", sig.signature_data.len());
        }
        "signature-raw" => {
            std::io::stdout().write_all(sig.signature_data)?;
        }
        "superblob" => {
            let embedded = parse_signature_data(&sig.signature_data)?;

            println!("length: {}", embedded.length);
            println!("blob count: {}", embedded.count);
            println!("blobs:");
            for blob in embedded.blobs {
                println!("- index: {}", blob.index);
                println!("  offset: {}", blob.offset);
                println!("  length: {}", blob.length);
                println!("  end offset: {}", blob.offset + blob.length - 1);
                println!("  magic: {:?}", blob.magic);
            }
        }
        _ => panic!("unhandled format: {}", format),
    }

    Ok(())
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
        .subcommand(
            SubCommand::with_name("extract-macho-signature")
                .about("Extracts code signature data from a Mach-O binary")
                .arg(
                    Arg::with_name("path")
                        .required(true)
                        .help("Path to Mach-O binary to examine"),
                )
                .arg(
                    Arg::with_name("format")
                        .long("format")
                        .takes_value(true)
                        .possible_values(&[
                            "blobs",
                            "code-directory",
                            "linkedit-segment-raw",
                            "requirements",
                            "segment-info",
                            "signature-raw",
                            "superblob",
                        ])
                        .default_value("segment-info")
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
        ("extract-macho-signature", Some(args)) => command_extract_macho_signature(args),
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
