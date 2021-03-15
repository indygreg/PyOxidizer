// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Binary signing for Apple platforms.

This crate contains code for interfacing with binary code signing on Apple
platforms.

*/

pub mod code_hash;
pub mod macho;
pub mod specification;

use {
    crate::macho::{
        find_signature_data, EmbeddedSignature, EntitlementsBlob, HashType, MachOError,
        RequirementsBlob,
    },
    cryptographic_message_syntax::asn1::rfc5280::Certificate,
    goblin::mach::MachO,
};

/// A generic error during code signing.
pub enum CodeSignError {
    /// An error occurred when decoding a certificate from BER/DER.
    CertificateDecode(bcder::decode::Error),
    /// An error when parsing a PEM encoded certificate.
    CertificatePem(pem::PemError),
}

impl From<bcder::decode::Error> for CodeSignError {
    fn from(e: bcder::decode::Error) -> Self {
        Self::CertificateDecode(e)
    }
}

impl From<pem::PemError> for CodeSignError {
    fn from(e: pem::PemError) -> Self {
        Self::CertificatePem(e)
    }
}

/// Build Apple embedded signatures from parameters.
///
/// This type provides a high-level interface for signing a Mach-O binary.
#[derive(Debug)]
pub struct SignatureBuilder<'a> {
    /// The binary we are signing.
    macho: MachO<'a>,

    /// Identifier string for the binary.
    ///
    /// This is likely the `CFBundleIdentifier` value from the `Info.plist` in a bundle.
    /// e.g. `com.example.my_program`.
    identifier: String,

    /// Digest method to use.
    hash_type: HashType,

    /// Embedded entitlements data.
    entitlements: Option<EntitlementsBlob<'static>>,

    /// Code requirement data.
    code_requirement: Option<RequirementsBlob<'static>>,

    /// Setup and mode flags from CodeDirectory.
    cdflags: Option<u32>,

    /// Flags for Code Directory execseg field.
    ///
    /// These are the `CS_EXECSEG_*` constants.
    ///
    /// `CS_EXECSEG_MAIN_BINARY` should be set on executable binaries.
    executable_segment_flags: Option<u64>,

    /// Runtime minimum version requirement.
    ///
    /// Corresponds to `CodeDirectory`'s `runtime` field.
    runtime: Option<u32>,

    /// Certificate information to include.
    certificates: Vec<Certificate>,
}

impl<'a> SignatureBuilder<'a> {
    /// Create an instance that will sign a MachO binary.
    pub fn new(macho: MachO<'a>, identifier: impl ToString) -> Self {
        Self {
            macho,
            identifier: identifier.to_string(),
            hash_type: HashType::Sha256,
            entitlements: None,
            code_requirement: None,
            cdflags: None,
            executable_segment_flags: None,
            runtime: None,
            certificates: vec![],
        }
    }

    /// Loads context from an existing signature on the binary into this builder.
    ///
    /// By default, newly constructed builders have no context and each field
    /// must be populated manually. When this function is called, existing
    /// signature data in the Mach-O binary will be "imported" to this builder and
    /// settings should be carried forward.
    ///
    /// If the binary has no signature data, this function does nothing.
    pub fn load_existing_signature_context(&mut self) -> Result<(), MachOError> {
        if let Some(signature) = find_signature_data(&self.macho)? {
            let signature = EmbeddedSignature::from_bytes(signature.signature_data)?;

            if let Some(cd) = signature.code_directory()? {
                self.identifier = cd.ident.to_string();
                self.hash_type = cd.hash_type;
                self.cdflags = Some(cd.flags);
                self.executable_segment_flags = cd.exec_seg_flags;
                self.runtime = cd.runtime;
            }

            if let Some(blob) = signature.code_requirements()? {
                self.code_requirement = Some(blob.to_owned());
            }

            if let Some(entitlements) = signature.entitlements()? {
                self.entitlements = Some(EntitlementsBlob::from_string(entitlements));
            }

            Ok(())
        } else {
            Ok(())
        }
    }

    /// Set the value of the entitlements string to sign.
    ///
    /// This should be an XML plist.
    ///
    /// Accepts any argument that converts to a `String`.
    pub fn set_entitlements_string(&mut self, v: impl ToString) -> Option<String> {
        let old = self.entitlements.as_ref().map(|e| e.to_string());

        self.entitlements = Some(EntitlementsBlob::from_string(v));

        old
    }

    /*
    /// Set the code requirement blob data.
    ///
    /// The passed value is the binary serialization of a Code Requirement
    /// expression. See `man csreq` on an Apple machine and
    /// https://developer.apple.com/library/archive/technotes/tn2206/_index.html#//apple_ref/doc/uid/DTS40007919-CH1-TNTAG4
    /// for more info.
    pub fn set_code_requirement(&mut self, v: impl Into<Vec<u8>>) -> Option<Vec<u8>> {
        self.code_requirement.replace(v.into())
    }
     */

    /// Set the executable segment flags for this binary.
    ///
    /// See the `CS_EXECSEG_*` constants in the `macho` module for description.
    pub fn executable_segment_flags(&mut self, flags: u64) -> Option<u64> {
        self.executable_segment_flags.replace(flags)
    }

    /// Add a DER encoded X.509 public certificate to the signing chain.
    ///
    /// Use this to add the raw binary content of an ASN.1 encoded public
    /// certificate.
    ///
    /// The DER data is decoded at function call time. Any error decoding the
    /// certificate will result in `Err`. No validation of the certificate is
    /// performed.
    pub fn add_certificate_der(&mut self, data: &[u8]) -> Result<(), CodeSignError> {
        let cert = bcder::decode::Constructed::decode(data, bcder::Mode::Der, |cons| {
            Certificate::take_from(cons)
        })?;

        self.certificates.push(cert);

        Ok(())
    }

    /// Add a PEM encoded X.509 public certificate to the signing chain.
    ///
    /// PEM data looks like `-----BEGIN CERTIFICATE-----` and is a common method
    /// for encoding certificate data. (PEM is effectively base64 encoded DER data.)
    ///
    /// Only a single certificate is read from the PEM data.
    pub fn add_certificate_pem(&mut self, data: impl AsRef<[u8]>) -> Result<(), CodeSignError> {
        let pem = pem::parse(data)?;

        self.add_certificate_der(&pem.contents)
    }
}
