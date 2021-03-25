// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Binary code signing for Apple platforms.
//!
//! This crate provides a pure Rust implementation of binary code signing
//! for Apple operating systems (like macOS and iOS). A goal of this crate
//! is to facilitate reimplementing functionality from Apple's `codesign`
//! and other similar tools without a dependency on an Apple machine or
//! operating system: you should be able to sign Apple binaries from Linux
//! or Windows if you wanted to.
//!
//! **This crate is in its early stages of development and there are many
//! rough edges. Use at your own risk. Always validate signed binaries
//! with Apple's `codesign` tool to ensure correctness.**
//!
//! # Features and Capabilities
//!
//! This crate can:
//!
//! * Find code signature data embedded in Mach-O binaries (both single and
//!   multi-arch/fat/universal binaries). (See [find_signature_data],
//!   [parse_signature_data].)
//! * Deeply parse code signature data into Rust structs. (See
//!   [EmbeddedSignature], [BlobData], and e.g. [CodeDirectoryBlob].
//! * Parse and verify the RFC 5652 Cryptographic Message Syntax (CMS)
//!   signature data. This includes using a Time-Stamp Protocol (TSP) / RFC 3161
//!   server for including a signed time-stamp token for that signature.
//!   (Functionality provided by the `cryptographic-message-syntax` crate,
//!   developed in the same repository as this crate.)
//! * Generate new embedded signature data, including cryptographically
//!   signing that data using any signing key and X.509 certificate chain
//!   you provide. (See [MachOSigner] and [MachOSignatureBuilder].)
//! * Writing a new Mach-O file containing new signature data. (See
//!   [MachOSigner].)
//!
//! There are a number of missing features and capabilities from this crate
//! that we hope are eventually implemented:
//!
//! * Only embedded signatures are supported. (No support for detached signatures.)
//! * No support for parsing Code Signing Requirements. There is a binary encoding
//!   of this language which we do not yet parse. There is also a human friendly
//!   DSL that gets compiled to binary which we do not support parsing. To use Code
//!   Signing Requirements, you will need to use the `csreq` tool to compile an
//!   expression to binary and then give that binary blob to this crate. (We can
//!   embed that blob in the signature data without knowing what is inside.)
//! * Minimal signed resources support. We will preserve an existing referenced
//!   file digest when re-signing a binary if signing settings are carried forward.
//!   We support defining the content of the resources XML plist file so it can be
//!   digested. We have minimal support for parsing these XML plist files. We do not
//!   yet support mutating/building new resources XML plist files.
//! * No bundle-based signing. We eventually want to provide an API where you
//!   provide the path to a bundle (e.g. `/Applications/MyProgram.app`) and it
//!   automatically finds and signs binaries, automatically signing resources and
//!   signing nested binaries in the correct order so all content digests are chained
//!   and secure.
//! * No turnkey support for signing keys. We want to make it easier for obtaining
//!   signing keys (and their X.509 certificate chain) for use with this crate. It
//!   should be possible to easily integrate with the OS's key store or hardware
//!   based stores (such as Yubikeys). We also don't look for necessary X.509
//!   certificate extensions that Apple's verification likely mandates, which we should
//!   do and enforce.
//! * Notarization support. The notarization ticket appears to be part of the embedded
//!   signature data. We don't support parsing this blob. It should be possible to
//!   coerce this crate into emitting a notarization blob in the signature data. But
//!   this isn't currently implemented as part of our high-level signing primitives.
//!
//! There is missing features and functionality that will likely never be implemented:
//!
//! * Binary verification compliant with Apple's operating systems. We are capable
//!   of verifying the hashes of code and other embedded signature data. We can also
//!   verify that a cryptographic signature came from the annotated public key in
//!   that signature. We can also write heuristics to look for certain common problems
//!   with signatures. But we can't and likely never will implement all the rules Apple
//!   uses to verify a binary for execution because the rules are complex and we don't
//!   fully understand what they are because the implementation is proprietary.
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
//! [find_signature_data] and [parse_signature_data] are useful for
//! finding and then parsing signature data into an [EmbeddedSignature] instance for
//! examination.
//!
//! If you'd like to learn about the technical underpinnings of code signing on Apple
//! platforms, see [specification].

mod certificate;
pub use certificate::*;
mod code_hash;
pub use code_hash::*;
mod code_resources;
pub use code_resources::*;
mod macho;
pub use macho::*;
mod signing;
pub use signing::*;
pub mod specification;
mod verify;
pub use verify::*;
