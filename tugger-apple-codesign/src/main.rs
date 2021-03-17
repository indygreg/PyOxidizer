// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

#[allow(unused)]
mod code_hash;
#[allow(unused)]
mod macho;
#[allow(unused)]
mod signing;
#[allow(unused)]
mod specification;

use {
    crate::{
        code_hash::{compute_code_hashes, SignatureError},
        macho::{
            find_signature_data, parse_signature_data, Blob, CodeDirectoryBlob, CodeSigningSlot,
            HashType, RequirementsBlob,
        },
        signing::{MachOSigner, NotSignableError, SigningError},
    },
    clap::{App, AppSettings, Arg, ArgMatches, SubCommand},
    cryptographic_message_syntax::{CmsError, SignedData},
    goblin::mach::{Mach, MachO},
    std::{convert::TryFrom, io::Write, str::FromStr},
};

const EXTRACT_ABOUT: &str = "\
Extract code signature data from a Mach-O binary.

Given the path to a Mach-O binary (including fat/universal) binaries, this
command will parse and print requested data to stdout.

The --data argument controls which data to extract and how to print it.
Possible values are:

blobs
   Low-level information on the records in the embedded code signature.
cms-ber
   BER encoded ASN.1 of the CMS SignedObject message containing a
   cryptographic signature over content. (This will print binary
   to stdout.)
cms-pem
   Like cms-ber except it prints PEM encoded data, which is ASCII and
   safe to print to terminals.
cms
   Print the ASN.1 decoded CMS data.
code-directory-raw
   Raw binary data composing the code directory data structure.
code-directory
   Information on the main code directory data structure.
code-directory-serialized
   Reserialize the parsed code directory, parse it again, and then print
   it like `code-directory` would.
code-directory-serialized-raw
   Reserialize the parsed code directory and emit its binary. Useful
   for comparing round-tripping of code directory data.
linkededit-segment-raw
   Complete content of the __LINKEDIT Mach-O segment as binary.
requirements-raw
   Raw binary data composing the requirements blob/slot.
requirements
   Parsed code requirement statement/expression.
requirements-serialized
   Reserialize the code requirements blob, parse it again, and then
   print it like `requirements` would.
requirements-serialized-raw
   Reserialize the code requirements blob and emit its binary.
signature-raw
   Raw binary data composing the signature data embedded in the binary.
segment-info
   Information about Mach-O segments in the binary and where the
   __LINKEDIT is in relationship to the binary.
superblob
   The SuperBlob record and high-level details of embedded Blob
   records, including digests of every Blob.
";

const SUPPORTED_HASHES: &[&str; 5] = &["none", "sha1", "sha256", "sha256-truncated", "sha384"];

#[derive(Debug)]
enum AppError {
    UnknownCommand,
    BadArgument,
    Io(std::io::Error),
    Goblin(goblin::error::Error),
    MachOError(crate::macho::MachOError),
    NoCodeSignature,
    NoCmsData,
    Digest(crate::macho::DigestError),
    Signature(SignatureError),
    Cms(CmsError),
    NotSignable(NotSignableError),
    Signing(SigningError),
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
            Self::Digest(e) => f.write_fmt(format_args!("digest error: {}", e)),
            Self::Signature(e) => e.fmt(f),
            Self::Cms(e) => f.write_fmt(format_args!("CMS error: {}", e)),
            Self::NotSignable(e) => f.write_fmt(format_args!("binary not signable: {}", e)),
            Self::Signing(e) => f.write_fmt(format_args!("signing error: {}", e)),
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

impl From<crate::macho::MachOError> for AppError {
    fn from(e: crate::macho::MachOError) -> Self {
        Self::MachOError(e)
    }
}

impl From<crate::macho::DigestError> for AppError {
    fn from(e: crate::macho::DigestError) -> Self {
        Self::Digest(e)
    }
}

impl From<SignatureError> for AppError {
    fn from(e: SignatureError) -> Self {
        Self::Signature(e)
    }
}

impl From<CmsError> for AppError {
    fn from(e: CmsError) -> Self {
        Self::Cms(e)
    }
}

impl From<NotSignableError> for AppError {
    fn from(e: NotSignableError) -> Self {
        Self::NotSignable(e)
    }
}

impl From<SigningError> for AppError {
    fn from(e: SigningError) -> Self {
        Self::Signing(e)
    }
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

fn command_compute_code_hashes(args: &ArgMatches) -> Result<(), AppError> {
    let path = args.value_of("path").ok_or(AppError::BadArgument)?;
    let index = args.value_of("universal_index").unwrap();
    let index = usize::from_str(index).map_err(|_| AppError::BadArgument)?;
    let hash_type = HashType::try_from(args.value_of("hash").unwrap())?;
    let page_size = if let Some(page_size) = args.value_of("page_size") {
        Some(usize::from_str(page_size).map_err(|_| AppError::BadArgument)?)
    } else {
        None
    };

    let data = std::fs::read(path)?;
    let macho = get_macho_from_data(&data, index)?;

    let hashes = compute_code_hashes(&macho, hash_type, page_size)?;

    for hash in hashes {
        println!("{}", hex::encode(hash));
    }

    Ok(())
}

fn command_extract(args: &ArgMatches) -> Result<(), AppError> {
    let path = args.value_of("path").ok_or(AppError::BadArgument)?;
    let format = args.value_of("data").ok_or(AppError::BadArgument)?;
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
        "cms-ber" => {
            let embedded = parse_signature_data(&sig.signature_data)?;
            if let Some(cms) = embedded.signature_data()? {
                std::io::stdout().write_all(cms)?;
            } else {
                eprintln!("no CMS data");
            }
        }
        "cms-pem" => {
            let embedded = parse_signature_data(&sig.signature_data)?;
            if let Some(cms) = embedded.signature_data()? {
                print!(
                    "{}",
                    pem::encode(&pem::Pem {
                        tag: "PKCS7".to_string(),
                        contents: cms.to_vec(),
                    })
                );
            } else {
                eprintln!("no CMS data");
            }
        }
        "cms" => {
            let embedded = parse_signature_data(&sig.signature_data)?;
            if let Some(cms) = embedded.signature_data()? {
                let signed_data = SignedData::parse_ber(cms)?;

                println!("{:#?}", signed_data);
            } else {
                eprintln!("no CMS data");
            }
        }
        "code-directory-raw" => {
            let embedded = parse_signature_data(&sig.signature_data)?;

            if let Some(blob) = embedded.find_slot(CodeSigningSlot::CodeDirectory) {
                std::io::stdout().write_all(blob.data)?;
            } else {
                eprintln!("no code directory");
            }
        }
        "code-directory-serialized-raw" => {
            let embedded = parse_signature_data(&sig.signature_data)?;

            if let Ok(Some(cd)) = embedded.code_directory() {
                std::io::stdout().write_all(&cd.to_vec()?)?;
            } else {
                eprintln!("no code directory");
            }
        }
        "code-directory-serialized" => {
            let embedded = parse_signature_data(&sig.signature_data)?;

            if let Ok(Some(cd)) = embedded.code_directory() {
                let serialized = cd.to_vec()?;
                println!("{:#?}", CodeDirectoryBlob::from_bytes(&serialized)?);
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
        "requirements-raw" => {
            let embedded = parse_signature_data(&sig.signature_data)?;

            if let Some(blob) = embedded.find_slot(CodeSigningSlot::Requirements) {
                std::io::stdout().write_all(blob.data)?;
            } else {
                eprintln!("no requirements");
            }
        }
        "requirements-serialized-raw" => {
            let embedded = parse_signature_data(&sig.signature_data)?;

            if let Some(reqs) = embedded.code_requirements()? {
                std::io::stdout().write_all(&reqs.to_vec()?)?;
            } else {
                eprintln!("no requirements");
            }
        }
        "requirements-serialized" => {
            let embedded = parse_signature_data(&sig.signature_data)?;

            if let Some(reqs) = embedded.code_requirements()? {
                let serialized = reqs.to_vec()?;
                println!("{:#?}", RequirementsBlob::from_bytes(&serialized)?);
            } else {
                eprintln!("no requirements");
            }
        }
        "requirements" => {
            let embedded = parse_signature_data(&sig.signature_data)?;

            if let Some(reqs) = embedded.code_requirements()? {
                println!("{:#?}", reqs)
            } else {
                eprintln!("no requirements");
            }
        }
        "segment-info" => {
            println!("segments count: {}", sig.segments_count);
            println!("__LINKEDIT segment index: {}", sig.linkedit_segment_index);
            println!(
                "__LINKEDIT segment start offset: {}",
                sig.linkedit_segment_start_offset
            );
            println!(
                "__LINKEDIT segment end offset: {}",
                sig.linkedit_segment_end_offset
            );
            println!(
                "__LINKEDIT segment size: {}",
                sig.linkedit_segment_data.len()
            );
            println!(
                "__LINKEDIT signature global start offset: {}",
                sig.linkedit_signature_start_offset
            );
            println!(
                "__LINKEDIT signature global end offset: {}",
                sig.linkedit_signature_end_offset
            );
            println!(
                "__LINKEDIT signature local segment start offset: {}",
                sig.signature_start_offset
            );
            println!(
                "__LINKEDIT signature local segment end offset: {}",
                sig.signature_end_offset
            );
            println!("__LINKEDIT signature size: {}", sig.signature_data.len());
        }
        "signature-raw" => {
            std::io::stdout().write_all(&sig.signature_data)?;
        }
        "superblob" => {
            let embedded = parse_signature_data(&sig.signature_data)?;

            println!("file start offset: {}", sig.linkedit_signature_start_offset);
            println!("file end offset: {}", sig.linkedit_signature_end_offset);
            println!("__LINKEDIT start offset: {}", sig.signature_start_offset);
            println!("__LINKEDIT end offset: {}", sig.signature_end_offset);
            println!("length: {}", embedded.length);
            println!("blob count: {}", embedded.count);
            println!("blobs:");
            for blob in embedded.blobs {
                println!("- index: {}", blob.index);
                println!("  offset: {}", blob.offset);
                println!("  length: {}", blob.length);
                println!("  end offset: {}", blob.offset + blob.length - 1);
                println!("  slot: {:?}", blob.slot);
                println!("  magic: {:?}", blob.magic);
                println!("  sha1: {}", hex::encode(blob.digest_with(HashType::Sha1)?));
                println!(
                    "  sha256: {}",
                    hex::encode(blob.digest_with(HashType::Sha256)?)
                );
                println!(
                    "  sha384: {}",
                    hex::encode(blob.digest_with(HashType::Sha384)?)
                );
            }
        }
        _ => panic!("unhandled format: {}", format),
    }

    Ok(())
}

fn command_sign(args: &ArgMatches) -> Result<(), AppError> {
    let input_path = args.value_of("input_path").ok_or(AppError::BadArgument)?;
    let output_path = args.value_of("output_path").ok_or(AppError::BadArgument)?;

    println!("signing {}", input_path);
    let macho_data = std::fs::read(input_path)?;

    println!("parsing Mach-O");
    let mut signer = MachOSigner::new(&macho_data)?;
    signer.load_existing_signature_context()?;

    println!("writing {}", output_path);
    let mut fh = std::fs::File::create(output_path)?;
    signer.write_signed_binary(&mut fh)?;

    Ok(())
}

fn main_impl() -> Result<(), AppError> {
    let matches = App::new("Oxidized Apple Codesigning")
        .setting(AppSettings::ArgRequiredElseHelp)
        .version("0.1")
        .author("Gregory Szorc <gregory.szorc@gmail.com>")
        .about("Do things related to code signing of Apple binaries")
        .subcommand(
            SubCommand::with_name("compute-code-hashes")
                .about("Compute code hashes for a binary")
                .arg(
                    Arg::with_name("path")
                        .required(true)
                        .help("path to Mach-O binary to examine"),
                )
                .arg(
                    Arg::with_name("hash")
                        .long("hash")
                        .takes_value(true)
                        .possible_values(SUPPORTED_HASHES)
                        .default_value("sha256")
                        .help("Hashing algorithm to use"),
                )
                .arg(
                    Arg::with_name("page_size")
                        .long("page-size")
                        .takes_value(true)
                        .help("Chunk size to digest over"),
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
            SubCommand::with_name("extract")
                .about("Extracts code signature data from a Mach-O binary")
                .long_about(EXTRACT_ABOUT)
                .arg(
                    Arg::with_name("path")
                        .required(true)
                        .help("Path to Mach-O binary to examine"),
                )
                .arg(
                    Arg::with_name("data")
                        .long("data")
                        .takes_value(true)
                        .possible_values(&[
                            "blobs",
                            "cms-ber",
                            "cms-pem",
                            "cms",
                            "code-directory-raw",
                            "code-directory-serialized-raw",
                            "code-directory-serialized",
                            "code-directory",
                            "linkedit-segment-raw",
                            "requirements-raw",
                            "requirements-serialized-raw",
                            "requirements-serialized",
                            "requirements",
                            "segment-info",
                            "signature-raw",
                            "superblob",
                        ])
                        .default_value("segment-info")
                        .help("Which data to extract and how to format it"),
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
            SubCommand::with_name("sign")
                .about("Adds a code signature to a Mach-O binary")
                .arg(
                    Arg::with_name("input_path")
                        .required(true)
                        .help("Path to Mach-O binary to sign"),
                )
                .arg(
                    Arg::with_name("output_path")
                        .required(true)
                        .help("Path to signed Mach-O binary to write"),
                ),
        )
        .get_matches();

    match matches.subcommand() {
        ("compute-code-hashes", Some(args)) => command_compute_code_hashes(args),
        ("extract", Some(args)) => command_extract(args),
        ("sign", Some(args)) => command_sign(args),
        _ => Err(AppError::UnknownCommand),
    }
}

fn main() {
    let exit_code = match main_impl() {
        Ok(()) => 0,
        Err(err) => {
            eprintln!("Error: {}", err);
            1
        }
    };

    std::process::exit(exit_code)
}
