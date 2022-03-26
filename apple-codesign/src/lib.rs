// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Binary code signing for Apple platforms.
//!
//! This crate implements application code signing for Apple operating systems
//! (like macOS and iOS). A goal of this crate is to serve as a stand-in
//! replacement for Apple's `codesign` (and similar tools) without a dependency
//! on an Apple hardware device or operating system: you should be able to
//! sign and release Apple binaries from Linux, Windows, or other non-Apple
//! environments if you want to.
//!
//! Apple code signing is complex and there are likely several areas where
//! this crate and Apple's implementations don't align. It is highly recommended
//! to validate output against what Apple's official tools produce.
//!
//! # Features and Capabilities
//!
//! This crate can:
//!
//! * Find code signature data embedded in Mach-O binaries (both single and
//!   multi-arch/fat/universal binaries). (See [AppleSignable] trait and its
//!   methods.)
//! * Deeply parse code signature data into Rust structs. (See
//!   [EmbeddedSignature], [BlobData], and e.g. [CodeDirectoryBlob].
//! * Parse and verify the RFC 5652 Cryptographic Message Syntax (CMS)
//!   signature data. This includes using a Time-Stamp Protocol (TSP) / RFC 3161
//!   server for including a signed time-stamp token for that signature.
//!   (Functionality provided by the `cryptographic-message-syntax` crate,
//!   developed in the same repository as this crate.)
//! * Generate new embedded signature data, including cryptographically
//!   signing that data using any signing key and X.509 certificate chain
//!   you provide. (See [MachOSigner] and [BundleSigner].)
//! * Writing a new Mach-O file containing new signature data. (See
//!   [MachOSigner].)
//! * Parse `CodeResources` XML plist files defining information on nested/signed
//!   resources within bundles. This includes parsing and applying the filtering
//!   rules defining in these files.
//! * Sign bundles. Nested bundles will automatically be signed. Additional
//!   Mach-O binaries outside the main executable will also be signed. Non
//!   Mach-O/code files will be digested. A `CodeResources` XML file will be
//!   produced.
//! * Submit notarization requests to Apple and query notarization status. (Only
//!   macOS `.app` bundles are currently supported.)
//! * Retrieve notarization tickets from Apple and staple. (Only bundles are currently
//!   supported.)
//!
//! There are a number of missing features and capabilities from this crate
//! that we hope are eventually implemented:
//!
//! * Only embedded signatures are supported. (No support for detached signatures.)
//! * No parsing of the Code Signing Requirements DSL. We support parsing the binary
//!   requirements to Rust structs, serializing back to binary, and rendering to the
//!   human friendly DSL. You will need to use the `csreq` tool to compile an
//!   expression to binary and then give that binary blob to this crate. Alternatively,
//!   you can write Rust code to construct a code requirements expression and serialize
//!   that to binary.
//! * No turnkey support for signing keys. We want to make it easier for obtaining
//!   signing keys (and their X.509 certificate chain) for use with this crate. It
//!   should be possible to easily integrate with the OS's key store or hardware
//!   based stores (such as Yubikeys). We also don't look for necessary X.509
//!   certificate extensions that Apple's verification likely mandates, which we should
//!   do and enforce.
//! * Not all signable formats can be notarized. Support for `.dmg` files is a major
//!   limitation.
//!
//! There is missing features and functionality that will likely never be implemented:
//!
//! * Binary verification compliant with Apple's operating systems. We are capable
//!   of verifying the digests of code and other embedded signature data. We can also
//!   verify that a cryptographic signature came from the annotated public key in
//!   that signature. We can also write heuristics to look for certain common problems
//!   with signatures. But we can't and likely never will implement all the rules Apple
//!   uses to verify a binary for execution because we perceive there to be little
//!   value in doing this. This crate could be used to build such functionality
//!   elsewhere, however.
//!
//! # End-User Documentation
//!
//! See [tutorial] for end-user documentation showing how this crate can be used.
//!
//! # Getting Started
//!
//! This crate is still in early phases of development. Until things are more mature,
//! a good place to start with the source code is `main.rs` to get a feel for what
//! CLI commands do.
//!
//! The [MachOSigner] type is your gateway to how code signing
//! is performed.
//!
//! The [AppleSignable] trait extends the [goblin::mach::MachO] type with code
//! signing functionality.
//!
//! The [EmbeddedSignature] type describes existing code signatures on Mach-O
//! binaries.
//!
//! If you'd like to learn about the technical underpinnings of code signing on Apple
//! platforms, see [specification].
//!
//! # Accessing Apple Code Signing Certificates
//!
//! This crate doesn't yet support integrating with the macOS keychain to obtain
//! or use the code signing certificate private key. However, it does support
//! importing the certificate key from a `.p12` file exported from the `Keychain
//! Access` application. It also supports exporting the x509 certificate chain
//! for a given certificate by speaking directly to the macOS keychain APIs.
//!
//! See the `keychain-export-certificate-chain` CLI command for exporting a
//! code signing certificate's x509 chain as PEM.

mod apple_certificates;
pub use apple_certificates::*;
pub mod app_metadata;
pub mod app_store_connect;
mod bundle_signing;
pub use bundle_signing::*;
mod certificate;
pub use certificate::*;
mod code_directory;
pub use code_directory::*;
mod code_hash;
pub use code_hash::*;
pub mod code_requirement;
pub use code_requirement::*;
mod code_resources;
pub use code_resources::*;
pub mod embedded_signature;
pub use embedded_signature::*;
pub mod entitlements;
mod error;
pub use error::*;
mod macho;
pub use macho::*;
#[cfg(target_os = "macos")]
#[allow(non_upper_case_globals)]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::*;
mod macho_signing;
pub use macho_signing::*;
pub mod notarization;
pub use notarization::*;
mod policy;
pub use policy::*;
mod signing;
pub use signing::*;
pub mod specification;
pub mod stapling;
pub mod ticket_lookup;
pub mod tutorial;
mod verify;
pub use verify::*;
