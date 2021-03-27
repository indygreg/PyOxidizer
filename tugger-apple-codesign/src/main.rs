// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

#[allow(unused)]
mod certificate;
#[allow(unused)]
mod code_hash;
#[allow(unused)]
mod code_resources;
#[allow(unused)]
mod macho;
#[allow(unused)]
mod signing;
#[allow(unused)]
mod specification;
#[allow(unused)]
mod verify;

use {
    crate::{
        certificate::{create_self_signed_code_signing_certificate, CertificateError},
        code_hash::compute_code_hashes,
        macho::{
            find_signature_data, parse_signature_data, Blob, CodeDirectoryBlob, CodeSigningSlot,
            DigestType, RequirementsBlob,
        },
        signing::{MachOSigner, NotSignableError, SigningError},
    },
    clap::{App, AppSettings, Arg, ArgMatches, SubCommand},
    cryptographic_message_syntax::{
        Certificate, CertificateKeyAlgorithm, CmsError, SignedData, SigningKey,
    },
    goblin::mach::{Mach, MachO},
    std::{convert::TryFrom, io::Write, path::PathBuf, str::FromStr},
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
cms-raw
   Print the payload of the CMS blob. This should be well-formed BER
   encoded ASN.1 data.
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

const GENERATE_SELF_SIGNED_CERTIFICATE_ABOUT: &str = "\
Generate a self-signed certificate that can be used for code signing.

This command will generate a new key pair using the algorithm of choice
then create an X.509 certificate wrapper for it that is signed with the
just-generated private key. The created X.509 certificate has extensions
that mark it as appropriate for code signing.

Certificates generated with this command can be useful for local testing.
However, because it is a self-signed certificate and isn't signed by a
trusted certificate authority, Apple operating systems may refuse to
load binaries signed with it.

The command prints 2 PEM encoded blocks. One block is for the X.509 public
certificate. The other is for the PKCS#8 private key (which can include
the public key).
";

const SIGN_ABOUT: &str = "\
Adds code signatures to a Mach-O binary.

This command will at an embedded code signature to the specified Mach-O binary.
It will then produce a new Mach-O binary at the path specified with embedded
code signature data.

The input path can be a single or multiple/fat/universal Mach-O binary. If
a fat binary is given, each Mach-O within that binary will be signed using
identical signing options.

By default, the embedded code signature will only contains hashes of
the binary and other important entities (such as entitlements and resources).
To use a code signing key/certificate to derive a cryptographic signature,
use the --pem-source argument to define paths to files containing PEM encoded
certificate/key data. (e.g. files with \"===== BEGIN CERTIFICATE =====\").

When reading PEM data for signing, there MUST be at least 1
`BEGIN CERTIFICATE` and 1 `BEGIN PRIVATE KEY` section in the read data.
(If you use the output from the `generate-self-signed-certificate` command,
this should just work.) There must be exactly 1 `PRIVATE KEY` defined.
And, the first `CERTIFICATE` is assumed to be paired with the `PRIVATE KEY`.
All extra `CERTIFICATE` sections are assumed to belong to the CA chain.

For best results, put your private key and its corresponding X.509 certificate
in a single file. Then make it the first --pem-source argument. It is highly
recommended to also include the X.509 certificates of the certificate signing
chain, up to the root CA, as this lowers the risk of verification failures at
run-time.

When using a code signing key/certificate, a Time-Stamp Protocol server URL
can be specified via --timestamp-url. By default, Apple's server is used. The
special value \"none\" can disable using a timestamp server.
";

const APPLE_TIMESTAMP_URL: &str = "http://timestamp.apple.com/ts01";

const SUPPORTED_HASHES: &[&str; 5] = &["none", "sha1", "sha256", "sha256-truncated", "sha384"];

#[derive(Debug)]
enum AppError {
    UnknownCommand,
    BadArgument,
    Io(std::io::Error),
    Goblin(goblin::error::Error),
    MachOError(crate::macho::MachOError),
    NoCodeSignature,
    Digest(crate::macho::DigestError),
    Cms(CmsError),
    NotSignable(NotSignableError),
    Signing(SigningError),
    VerificationProblems,
    Certificate(CertificateError),
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
            Self::Digest(e) => f.write_fmt(format_args!("digest error: {}", e)),
            Self::Cms(e) => f.write_fmt(format_args!("CMS error: {}", e)),
            Self::NotSignable(e) => f.write_fmt(format_args!("binary not signable: {}", e)),
            Self::Signing(e) => f.write_fmt(format_args!("signing error: {}", e)),
            Self::VerificationProblems => f.write_str("problems reported during verification"),
            Self::Certificate(e) => f.write_fmt(format_args!("certificate error: {}", e)),
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

impl From<CertificateError> for AppError {
    fn from(e: CertificateError) -> Self {
        Self::Certificate(e)
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
    let hash_type = DigestType::try_from(args.value_of("hash").unwrap())?;
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
        "cms-raw" => {
            let embedded = parse_signature_data(&sig.signature_data)?;
            if let Some(cms) = embedded.signature_data()? {
                std::io::stdout().write_all(cms)?;
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
                std::io::stdout().write_all(&cd.to_blob_bytes()?)?;
            } else {
                eprintln!("no code directory");
            }
        }
        "code-directory-serialized" => {
            let embedded = parse_signature_data(&sig.signature_data)?;

            if let Ok(Some(cd)) = embedded.code_directory() {
                let serialized = cd.to_blob_bytes()?;
                println!("{:#?}", CodeDirectoryBlob::from_blob_bytes(&serialized)?);
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
                std::io::stdout().write_all(&reqs.to_blob_bytes()?)?;
            } else {
                eprintln!("no requirements");
            }
        }
        "requirements-serialized" => {
            let embedded = parse_signature_data(&sig.signature_data)?;

            if let Some(reqs) = embedded.code_requirements()? {
                let serialized = reqs.to_blob_bytes()?;
                println!("{:#?}", RequirementsBlob::from_blob_bytes(&serialized)?);
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
                println!(
                    "  sha1: {}",
                    hex::encode(blob.digest_with(DigestType::Sha1)?)
                );
                println!(
                    "  sha256: {}",
                    hex::encode(blob.digest_with(DigestType::Sha256)?)
                );
                println!(
                    "  sha384: {}",
                    hex::encode(blob.digest_with(DigestType::Sha384)?)
                );
            }
        }
        _ => panic!("unhandled format: {}", format),
    }

    Ok(())
}

fn command_generate_self_signed_certificate(args: &ArgMatches) -> Result<(), AppError> {
    let algorithm = match args.value_of("algorithm").ok_or(AppError::BadArgument)? {
        "ecdsa" => CertificateKeyAlgorithm::Ecdsa,
        "ed25519" => CertificateKeyAlgorithm::Ed25519,
        value => panic!(
            "algorithm values should have been validated by arg parser: {}",
            value
        ),
    };

    let common_name = args.value_of("common_name").ok_or(AppError::BadArgument)?;
    let country_name = args.value_of("country_name").ok_or(AppError::BadArgument)?;
    let email_address = args
        .value_of("email_address")
        .ok_or(AppError::BadArgument)?;
    let validity_days = args.value_of("validity_days").unwrap();
    let validity_days = i64::from_str(validity_days).map_err(|_| AppError::BadArgument)?;

    let validity_duration = chrono::Duration::days(validity_days);

    let (cert, _, raw) = create_self_signed_code_signing_certificate(
        algorithm,
        common_name,
        country_name,
        email_address,
        validity_duration,
    )?;

    print!(
        "{}",
        pem::encode(&pem::Pem {
            tag: "CERTIFICATE".to_string(),
            contents: cert.as_ber()?,
        })
    );
    print!(
        "{}",
        pem::encode(&pem::Pem {
            tag: "PRIVATE KEY".to_string(),
            contents: raw
        })
    );

    Ok(())
}

fn command_sign(args: &ArgMatches) -> Result<(), AppError> {
    let input_path = args.value_of("input_path").ok_or(AppError::BadArgument)?;
    let output_path = args.value_of("output_path").ok_or(AppError::BadArgument)?;
    let code_resources_path = args.value_of("code_resources").map(PathBuf::from);
    let entitlement_path = args.value_of("entitlement").map(PathBuf::from);
    let pem_sources = match args.values_of("pem_source") {
        Some(values) => values.collect::<Vec<_>>(),
        None => vec![],
    };
    let timestamp_url = args
        .value_of("timestamp_url")
        .ok_or(AppError::BadArgument)?;
    let timestamp_url = if timestamp_url == "none" {
        None
    } else {
        Some(timestamp_url)
    };

    println!("signing {}", input_path);
    let macho_data = std::fs::read(input_path)?;

    println!("parsing Mach-O");
    let mut signer = MachOSigner::new(&macho_data)?;
    signer.load_existing_signature_context()?;

    let mut private_keys = vec![];
    let mut public_certificates = vec![];

    for pem_source in pem_sources {
        println!("reading PEM data from {}", pem_source);
        let pem_data = std::fs::read(pem_source)?;

        for pem in pem::parse_many(&pem_data) {
            match pem.tag.as_str() {
                "CERTIFICATE" => public_certificates.push(pem.contents),
                "PRIVATE KEY" => private_keys.push(pem.contents),
                tag => println!("(unhandled PEM tag {}; ignoring)", tag),
            }
        }
    }

    if private_keys.len() > 1 {
        println!("at most 1 PRIVATE KEY can be present; aborting");
        return Err(AppError::BadArgument);
    }

    let private = if private_keys.is_empty() {
        None
    } else {
        Some(SigningKey::from_pkcs8_der(&private_keys[0], None)?)
    };

    if let Some(signing_key) = &private {
        if public_certificates.is_empty() {
            println!("a PRIVATE KEY requires a corresponding CERTIFICATE to pair with it");
            return Err(AppError::BadArgument);
        }

        let cert = public_certificates.remove(0);
        let cert = Certificate::from_der(&cert)?;

        println!("registering signing key");
        signer.signing_key(signing_key, cert);

        if let Some(timestamp_url) = timestamp_url {
            println!("using time-stamp protocol server {}", timestamp_url);
            signer.time_stamp_url(timestamp_url)?;
        }
    }

    for cert in public_certificates {
        println!("registering extra X.509 certificate");
        signer.chain_certificate_der(&cert)?;
    }

    if let Some(code_resources_path) = code_resources_path {
        let code_resources_data = std::fs::read(code_resources_path)?;
        signer.code_resources_data(&code_resources_data)?;
    }

    if let Some(entitlement_path) = entitlement_path {
        let entitlement_data = std::fs::read_to_string(entitlement_path)?;
        signer.set_entitlements_string(&entitlement_data);
    }

    println!("writing {}", output_path);
    let mut fh = std::fs::File::create(output_path)?;
    signer.write_signed_binary(&mut fh)?;

    Ok(())
}

fn command_verify(args: &ArgMatches) -> Result<(), AppError> {
    let path = args.value_of("path").ok_or(AppError::BadArgument)?;

    let data = std::fs::read(path)?;

    let problems = verify::verify_macho_data(&data);

    for problem in &problems {
        println!("{}", problem);
    }

    if problems.is_empty() {
        eprintln!("no problems detected!");
        eprintln!("(we do not verify everything so please do not assume that the signature meets Apple standards)");
        Ok(())
    } else {
        Err(AppError::VerificationProblems)
    }
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
                            "cms-raw",
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
            SubCommand::with_name("generate-self-signed-certificate")
                .about("Generate a self-signed certificate for code signing")
                .long_about(GENERATE_SELF_SIGNED_CERTIFICATE_ABOUT)
                .arg(
                    Arg::with_name("algorithm")
                        .long("algorithm")
                        .takes_value(true)
                        .possible_values(&["ecdsa", "ed25519"])
                        .default_value("ecdsa")
                        .help("Which key type to use"),
                )
                .arg(
                    Arg::with_name("common_name")
                        .long("common-name")
                        .takes_value(true)
                        .default_value("default-name")
                        .help("Common Name (CN) value for certificate identifier"),
                )
                .arg(
                    Arg::with_name("country_name")
                        .long("country-name")
                        .takes_value(true)
                        .default_value("XX")
                        .help("Country Name (C) value for certificate identifier"),
                )
                .arg(
                    Arg::with_name("email_address")
                        .long("email-address")
                        .takes_value(true)
                        .default_value("someone@example.com")
                        .help("Email address value for certificate identifier"),
                )
                .arg(
                    Arg::with_name("validity_days")
                        .long("validity-days")
                        .takes_value(true)
                        .default_value("365")
                        .help("How many days the certificate should be valid for"),
                ),
        )
        .subcommand(
            SubCommand::with_name("sign")
                .about("Adds a code signature to a Mach-O binary")
                .long_about(SIGN_ABOUT)
                .arg(
                    Arg::with_name("code_resources")
                        .long("code-resources")
                        .takes_value(true)
                        .help("Path to an XML plist file containing code resources"),
                )
                .arg(
                    Arg::with_name("entitlement")
                        .long("entitlement")
                        .short("e")
                        .required(false)
                        .takes_value(true)
                        .help("Path to a plist file containing entitlements"),
                )
                .arg(
                    Arg::with_name("pem_source")
                        .long("pem-source")
                        .takes_value(true)
                        .multiple(true)
                        .help("Path to file containing PEM encoded certificate/key data"),
                )
                .arg(
                    Arg::with_name("timestamp_url")
                        .long("timestamp-url")
                        .takes_value(true)
                        .default_value(APPLE_TIMESTAMP_URL)
                        .help(
                            "URL of timestamp server to use to obtain a token of the CMS signature",
                        ),
                )
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
        .subcommand(
            SubCommand::with_name("verify")
                .about("Verifies code signature data")
                .arg(
                    Arg::with_name("path")
                        .required(true)
                        .help("Path of Mach-O binary to examine"),
                ),
        )
        .get_matches();

    match matches.subcommand() {
        ("compute-code-hashes", Some(args)) => command_compute_code_hashes(args),
        ("extract", Some(args)) => command_extract(args),
        ("generate-self-signed-certificate", Some(args)) => {
            command_generate_self_signed_certificate(args)
        }
        ("sign", Some(args)) => command_sign(args),
        ("verify", Some(args)) => command_verify(args),
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
