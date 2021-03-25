// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Cryptographic Message Syntax (RFC 5652) in Pure Rust

This crate attempts to implement parts of
[RFC 5652](https://tools.ietf.org/rfc/rfc5652.txt) in pure, safe Rust.

Functionality includes:

* Partial (de)serialization support for ASN.1 data structures. The
  Rust structs are all defined. But not everything has (de)serialization
  code implemented.
* High-level Rust API for extracting useful attributes from a parsed
  `SignedData` structure and performing common operations, such as verifying
  signature integrity.

RFC 5652 is quite old. If you are looking to digitally sign content, you may
want to look at something newer, such as RPKI (RFC 6488). (RPKI appears to
be the spiritual success to this specification.)

# IMPORTANT SECURITY LIMITATIONS

**The verification functionality in this crate is purposefully limited
and isn't sufficient for trusting signed data. You need to include additional
trust verification if you are using this crate for verifying signed data.**

This crate exposes functionality to verify signatures and content integrity
of *signed data*. Specifically it can verify that an embedded cryptographic
signature over some arbitrary/embedded content was issued by a known signing
certificate. This answers the question *did certificate X sign content Y*.
This is an important question to answer, but it fails to answer other important
questions such as:

* Is the signature cryptographically strong or weak? Do I trust the signature?
* Do I trust the signer?

Answering *do I trust the signer* is an extremely difficult and nuanced
problem. It entails things like:

* Ensuring the signing certificate is using secure cryptography.
* Validating that the signing certificate is one you think it was or was
  issued by a trusted party.
* Validating the certificate isn't expired or hasn't been revoked.
* Validating that the certificate contains attributes/extensions desired
  (e.g. a certificate can be earmarked as used for signing code).

If you are using this crate as part of verifying signed content, you need
to have answers to these hard questions. This will require writing code
beyond what is available in this crate. You ideally want to use existing
libraries for this, as getting this correct is difficult. Ideally you would
consult a security/cryptography domain expert for help.

# Technical Notes

RFC 5652 is based off PKCS #7 version 1.5 (RFC 2315). So common tools/libraries
for interacting with PKCS #7 may have success parsing this format. For example,
you can use OpenSSL to read the data structures:

   $ openssl pkcs7 -inform DER -in <filename> -print
   $ openssl pkcs7 -inform PEM -in <filename> -print
   $ openssl asn1parse -inform DER -in <filename>

RFC 5652 uses BER (not DER) for serialization. There were attempts to use
other, more popular BER/DER/ASN.1 serialization crates. However, we could
only get `bcder` working. In a similar vein, there are other crates
implementing support for common ASN.1 functionality, such as serializing
X.509 certificates. Again, many of these depend on serializers that don't
seem to be compatible with BER. So we've recursively defined ASN.1 data
structures referenced by RFC5652 and taught them to serialize using `bcder`.
*/

mod algorithm;
pub mod asn1;
mod certificate;
mod signing;
mod time_stamp_protocol;

pub use {
    algorithm::{CertificateKeyAlgorithm, DigestAlgorithm, SignatureAlgorithm, SigningKey},
    certificate::{Certificate, RelativeDistinguishedName},
    signing::{SignedDataBuilder, SignerBuilder},
    time_stamp_protocol::{time_stamp_message_http, time_stamp_request_http, TimeStampError},
};

use {
    crate::{
        asn1::{
            rfc3161::OID_TIME_STAMP_TOKEN,
            rfc3280::Name,
            rfc5652::{
                SignerIdentifier, Time, OID_CONTENT_TYPE, OID_MESSAGE_DIGEST, OID_SIGNING_TIME,
            },
        },
        certificate::certificate_is_subset_of,
    },
    bcder::{Integer, OctetString, Oid},
    pem::PemError,
    ring::{digest::Digest, signature::UnparsedPublicKey},
    std::{collections::HashSet, convert::TryFrom, fmt::Display, ops::Deref},
};

#[derive(Debug)]
pub enum CmsError {
    /// An error occurred decoding ASN.1 data.
    DecodeErr(bcder::decode::Error),

    /// The content-type attribute is missing from the SignedAttributes structure.
    MissingSignedAttributeContentType,

    /// The content-type attribute in the SignedAttributes structure is malformed.
    MalformedSignedAttributeContentType,

    /// The message-digest attribute is missed from the SignedAttributes structure.
    MissingSignedAttributeMessageDigest,

    /// The message-digest attribute is malformed.
    MalformedSignedAttributeMessageDigest,

    /// The signing-time signed attribute is malformed.
    MalformedSignedAttributeSigningTime,

    /// The time-stamp token unsigned attribute is malformed.
    MalformedUnsignedAttributeTimeStampToken,

    /// Subject key identifiers in signer info is not supported.
    SubjectKeyIdentifierUnsupported,

    /// A general I/O error occurred.
    Io(std::io::Error),

    /// An unknown message digest algorithm was encountered.
    UnknownDigestAlgorithm(Oid),

    /// An unknown signature algorithm was encountered.
    UnknownSignatureAlgorithm(Oid),

    /// An unknown certificate format was encountered.
    UnknownCertificateFormat,

    /// A certificate was not found.
    CertificateNotFound,

    /// Signature verification fail.
    SignatureVerificationError,

    /// No `SignedAttributes` were present when they should have been.
    NoSignedAttributes,

    /// Two content digests were not equivalent.
    DigestNotEqual,

    /// Error encoding/decoding PEM data.
    Pem(PemError),

    /// Error occurred when creating a signature.
    SignatureCreation,

    /// Attempted to use a `Certificate` but we couldn't find the backing data for it.
    CertificateMissingData,

    /// Error occurred parsing a distinguished name field in a certificate.
    DistinguishedNameParseError,

    /// Ring rejected loading a private key.
    KeyRejected(ring::error::KeyRejected),

    /// Error occurred in Time-Stamp Protocol.
    TimeStampProtocol(TimeStampError),
}

impl std::error::Error for CmsError {}

impl Display for CmsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DecodeErr(e) => e.fmt(f),
            Self::MissingSignedAttributeContentType => {
                f.write_str("content-type attribute missing from SignedAttributes")
            }
            Self::MalformedSignedAttributeContentType => {
                f.write_str("content-type attribute in SignedAttributes is malformed")
            }
            Self::MissingSignedAttributeMessageDigest => {
                f.write_str("message-digest attribute missing from SignedAttributes")
            }
            Self::MalformedSignedAttributeMessageDigest => {
                f.write_str("message-digest attribute in SignedAttributes is malformed")
            }
            Self::MalformedSignedAttributeSigningTime => {
                f.write_str("signing-time attribute in SignedAttributes is malformed")
            }
            Self::MalformedUnsignedAttributeTimeStampToken => {
                f.write_str("time-stamp token attribute in UnsignedAttributes is malformed")
            }
            Self::SubjectKeyIdentifierUnsupported => {
                f.write_str("signer info using subject key identifier is not supported")
            }
            Self::Io(e) => e.fmt(f),
            Self::UnknownDigestAlgorithm(oid) => {
                f.write_fmt(format_args!("unknown digest algorithm: {}", oid))
            }
            Self::UnknownSignatureAlgorithm(oid) => {
                f.write_fmt(format_args!("unknown signature algorithm: {}", oid))
            }
            Self::UnknownCertificateFormat => f.write_str("unknown certificate format"),
            Self::CertificateNotFound => f.write_str("certificate not found"),
            Self::SignatureVerificationError => f.write_str("signature verification failed"),
            Self::NoSignedAttributes => f.write_str("SignedAttributes structure is missing"),
            Self::DigestNotEqual => f.write_str("digests not equivalent"),
            Self::Pem(e) => f.write_fmt(format_args!("PEM error: {}", e)),
            Self::SignatureCreation => f.write_str("error during signature creation"),
            Self::CertificateMissingData => f.write_str("certificate data not available"),
            Self::DistinguishedNameParseError => {
                f.write_str("could not parse distinguished name data")
            }
            Self::KeyRejected(reason) => {
                f.write_fmt(format_args!("private key rejected: {}", reason))
            }
            Self::TimeStampProtocol(e) => {
                f.write_fmt(format_args!("Time-Stamp Protocol error: {}", e))
            }
        }
    }
}

impl From<bcder::decode::Error> for CmsError {
    fn from(e: bcder::decode::Error) -> Self {
        Self::DecodeErr(e)
    }
}

impl From<std::io::Error> for CmsError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<PemError> for CmsError {
    fn from(e: PemError) -> Self {
        Self::Pem(e)
    }
}

impl From<ring::error::KeyRejected> for CmsError {
    fn from(e: ring::error::KeyRejected) -> Self {
        Self::KeyRejected(e)
    }
}

impl From<TimeStampError> for CmsError {
    fn from(e: TimeStampError) -> Self {
        Self::TimeStampProtocol(e)
    }
}

/// Represents a CMS SignedData structure.
///
/// This is the high-level type representing a CMS signature of some data.
/// It contains a description of what was signed, the cryptographic signature
/// of what was signed, and likely the X.509 certificate chain for the
/// signing key.
///
/// This is a high-level data structure that ultimately gets (de)serialized
/// from/to ASN.1. It exists to facilitate common interactions with the
/// low-level ASN.1 without exposing the complexity of ASN.1.
#[derive(Clone, Debug)]
pub struct SignedData {
    /// Content digest algorithms used.
    digest_algorithms: HashSet<DigestAlgorithm>,

    /// Content that was signed.
    ///
    /// This is optional because signed content can also be articulated
    /// via signed attributes inside the `SignerInfo` structure.
    signed_content: Option<Vec<u8>>,

    /// Certificates embedded within the data structure.
    ///
    /// While not required, it is common for the SignedData data structure
    /// to embed the X.509 certificates used to sign the data within. This
    /// field holds those certificates.
    ///
    /// Typically the root CA is first and the actual signing certificate is
    /// last.
    certificates: Option<Vec<Certificate>>,

    /// Describes content signatures.
    signers: Vec<SignerInfo>,
}

impl SignedData {
    /// Construct an instance by parsing BER data.
    pub fn parse_ber(data: &[u8]) -> Result<Self, CmsError> {
        Self::try_from(&crate::asn1::rfc5652::SignedData::decode_ber(data)?)
    }

    /// Compute the digest of the encapsulated content using a specified algorithm.
    ///
    /// The returned value is likely used as the `message-digest` attribute type
    /// for use within signed attributes.
    ///
    /// You can get the raw bytes of the digest by calling its `.as_ref()`.
    pub fn message_digest_with_algorithm(&self, alg: DigestAlgorithm) -> Digest {
        let mut hasher = alg.as_hasher();

        if let Some(content) = &self.signed_content {
            hasher.update(content);
        }

        hasher.finish()
    }

    /// Obtain encapsulated content that was signed.
    ///
    /// This is the defined `encapContentInfo cContent` value.
    pub fn signed_content(&self) -> Option<&[u8]> {
        if let Some(content) = &self.signed_content {
            Some(content)
        } else {
            None
        }
    }

    pub fn certificates(&self) -> Box<dyn Iterator<Item = &Certificate> + '_> {
        match self.certificates.as_ref() {
            Some(certs) => Box::new(certs.iter()),
            None => Box::new(std::iter::empty()),
        }
    }

    /// Obtain signing information attached to this instance.
    ///
    /// Each iterated value represents an entity that cryptographically signed
    /// the content. Use these objects to validate the signed data.
    pub fn signers(&self) -> impl Iterator<Item = &SignerInfo> {
        self.signers.iter()
    }
}

impl TryFrom<&crate::asn1::rfc5652::SignedData> for SignedData {
    type Error = CmsError;

    fn try_from(raw: &crate::asn1::rfc5652::SignedData) -> Result<Self, Self::Error> {
        let digest_algorithms = raw
            .digest_algorithms
            .iter()
            .map(DigestAlgorithm::try_from)
            .collect::<Result<HashSet<_>, CmsError>>()?;

        let signed_content = if let Some(content) = &raw.content_info.content {
            Some(content.to_bytes().to_vec())
        } else {
            None
        };

        let certificates = if let Some(certs) = &raw.certificates {
            Some(
                certs
                    .iter()
                    .map(Certificate::try_from)
                    .collect::<Result<Vec<_>, CmsError>>()?,
            )
        } else {
            None
        };

        let signers = raw
            .signer_infos
            .iter()
            .map(SignerInfo::try_from)
            .collect::<Result<Vec<_>, CmsError>>()?;

        Ok(Self {
            digest_algorithms,
            signed_content,
            certificates,
            signers,
        })
    }
}

/// Represents a CMS SignerInfo structure.
///
/// This is a high-level interface to the SignerInfo ASN.1 type. It supports
/// performing common operations against that type.
///
/// Instances of this type are logically equivalent to a single
/// signed assertion within a `SignedData` payload. There can be multiple
/// signers per `SignedData`, which is why this type exists on its own.
#[derive(Clone, Debug)]
pub struct SignerInfo {
    /// The X.509 certificate issuer.
    issuer: Name,

    /// The X.509 certificate serial number.
    serial_number: Integer,

    /// The algorithm used for digesting signed content.
    digest_algorithm: DigestAlgorithm,

    /// Algorithm used for signing the digest.
    signature_algorithm: SignatureAlgorithm,

    /// The cryptographic signature.
    signature: Vec<u8>,

    /// Parsed signed attributes.
    signed_attributes: Option<SignedAttributes>,

    /// Raw data constituting SignedAttributes that needs to be digested.
    digested_signed_attributes_data: Option<Vec<u8>>,

    /// Parsed unsigned attributes.
    unsigned_attributes: Option<UnsignedAttributes>,
}

impl SignerInfo {
    /// Obtain the signing X.509 certificate's issuer name and its serial number.
    ///
    /// The returned value can be used to locate the certificate so
    /// verification can be performed.
    pub fn certificate_issuer_and_serial(&self) -> Option<(&Name, &Integer)> {
        Some((&self.issuer, &self.serial_number))
    }

    /// Obtain the message digest algorithm used by this signer.
    pub fn digest_algorithm(&self) -> DigestAlgorithm {
        self.digest_algorithm
    }

    /// Obtain the cryptographic signing algorithm used by this signer.
    pub fn signature_algorithm(&self) -> SignatureAlgorithm {
        self.signature_algorithm
    }

    /// Obtain the raw bytes constituting the cryptographic signature.
    ///
    /// This is the signature that should be verified.
    pub fn signature(&self) -> &[u8] {
        &self.signature
    }

    /// Obtain the `SignedAttributes` attached to this instance.
    pub fn signed_attributes(&self) -> Option<&SignedAttributes> {
        self.signed_attributes.as_ref()
    }

    /// Obtain the `UnsignedAttributes` attached to this instance.
    pub fn unsigned_attributes(&self) -> Option<&UnsignedAttributes> {
        self.unsigned_attributes.as_ref()
    }

    /// Verifies the signature defined by this signer given a `SignedData` instance.
    ///
    /// This function will perform cryptographic verification that the signature
    /// contained within this `SignerInfo` instance is valid for the content that
    /// was signed. The content that was signed is the encapsulated content from
    /// the `SignedData` instance (its `.signed_data()` value) combined with
    /// the `SignedAttributes` attached to this instance.
    ///
    /// # IMPORTANT SECURITY LIMITATIONS
    ///
    /// This method only performs signature verification. It:
    ///
    /// * DOES NOT verify the digest hash embedded within `SignedAttributes` (if present).
    /// * DOES NOT validate the signing certificate in any way.
    /// * DOES NOT validate that the cryptography used is appropriate.
    ///
    /// See the crate's documentation for more on the security implications.
    pub fn verify_signature_with_signed_data(
        &self,
        signed_data: &SignedData,
    ) -> Result<(), CmsError> {
        let verifier = self.signature_verifier(signed_data.certificates())?;
        let signed_content = self.signed_content_with_signed_data(signed_data);
        let signature = self.signature();

        verifier
            .verify(&signed_content, signature)
            .map_err(|_| CmsError::SignatureVerificationError)
    }

    /// Verifies the digest stored in signed attributes matches that of content in a `SignedData`.
    ///
    /// If signed attributes are present on this instance, they must contain
    /// a `message-digest` attribute defining the digest of data that was
    /// signed. The specification says this digested data should come from
    /// the encapsulated content within `SignedData` (`SignedData.signed_content()`).
    ///
    /// Note that some utilities of CMS will not store a computed digest
    /// in `message-digest` that came from `SignedData` or is using
    /// the digest algorithm indicated by this `SignerInfo`. This is strictly
    /// in violation of the specification but it does occur.
    ///
    /// # IMPORTANT SECURITY LIMITATIONS
    ///
    /// This method only performs message digest verification. It:
    ///
    /// * DOES NOT verify the signature over the signed data or anything about
    ///   the signer.
    /// * DOES NOT validate that the digest algorithm is strong/appropriate.
    /// * DOES NOT compare the digests in a manner that is immune to timing
    ///   side-channels.
    ///
    /// See the crate's documentation for more on the security implications.
    pub fn verify_message_digest_with_signed_data(
        &self,
        signed_data: &SignedData,
    ) -> Result<(), CmsError> {
        let signed_attributes = self
            .signed_attributes()
            .ok_or(CmsError::NoSignedAttributes)?;

        let wanted_digest: &[u8] = signed_attributes.message_digest.as_ref();
        let got_digest = self.compute_digest_with_signed_data(signed_data);

        // Susceptible to timing side-channel but we don't care per function
        // documentation.
        if wanted_digest == got_digest.as_ref() {
            Ok(())
        } else {
            Err(CmsError::DigestNotEqual)
        }
    }

    /// Obtain an entity for validating the signature described by this instance.
    ///
    /// See `signature_verifier_with_algorithm()` for documentation.
    ///
    /// This version calls into that with the signature algorithm used
    /// by this signer.
    pub fn signature_verifier<'a, C>(
        &self,
        certs: C,
    ) -> Result<UnparsedPublicKey<Vec<u8>>, CmsError>
    where
        C: Iterator<Item = &'a Certificate>,
    {
        self.signature_verifier_with_algorithm(
            certs,
            self.signature_algorithm.as_verification_algorithm(),
        )
    }

    /// Obtain an entity for validating the signature described by this instance.
    ///
    /// This will attempt to locate the certificate used by this signing info
    /// structure in the passed iterable of certificates and then construct
    /// a signature verifier that can be used to verify content integrity.
    ///
    /// The verification algorithm is controllable by the caller.
    ///
    /// If the certificate referenced by this signing info could not be found,
    /// an error occurs.
    pub fn signature_verifier_with_algorithm<'a, C>(
        &self,
        mut certs: C,
        algorithm: &'static dyn ring::signature::VerificationAlgorithm,
    ) -> Result<UnparsedPublicKey<Vec<u8>>, CmsError>
    where
        C: Iterator<Item = &'a Certificate>,
    {
        // The issuer of this signature is matched against the list of certificates.
        let signing_cert = certs
            .find(|cert| {
                // We're only verifying signatures here, not validating the certificate.
                // So even if the certificate comparison functionality is incorrect
                // (the called function does non-exact matching of the RdnSequence in
                // case the candidate certs have extra fields), that shouldn't have
                // security implications.
                certificate_is_subset_of(
                    &self.serial_number,
                    &self.issuer,
                    cert.serial_number(),
                    cert.issuer(),
                )
            })
            .ok_or(CmsError::CertificateNotFound)?;

        Ok(UnparsedPublicKey::new(
            algorithm,
            signing_cert.public_key.key.clone(),
        ))
    }

    /// Obtain the raw bytes of content that was signed given a `SignedData`.
    ///
    /// This joins the encapsulated content from `SignedData` with `SignedAttributes`
    /// on this instance to produce a new blob. This new blob is the message
    /// that is signed and whose signature is embedded in `SignerInfo` instances.
    pub fn signed_content_with_signed_data(&self, signed_data: &SignedData) -> Vec<u8> {
        self.signed_content(signed_data.signed_content())
    }

    /// Obtain the raw bytes of content that were digested and signed.
    ///
    /// The returned value is the message that was signed and whose signature
    /// of needs to be verified.
    ///
    /// The optional content argument is the `encapContentInfo eContent`
    /// field, typically the value of `SignedData.signed_content()`.
    pub fn signed_content(&self, content: Option<&[u8]>) -> Vec<u8> {
        let mut res = Vec::new();

        if let Some(content) = content {
            res.extend(content);
        }

        if let Some(signed_data) = &self.digested_signed_attributes_data {
            res.extend(signed_data.as_slice());
        }

        res
    }

    /// Compute a message digest using a `SignedData` instance.
    ///
    /// This will obtain the encapsulated content blob from a `SignedData`
    /// and digest it using the algorithm configured on this instance.
    ///
    /// The resulting digest is typically stored in the `message-digest`
    /// attribute of `SignedData`.
    pub fn compute_digest_with_signed_data(&self, signed_data: &SignedData) -> Digest {
        self.compute_digest(signed_data.signed_content())
    }

    /// Compute a message digest using the configured algorithm.
    ///
    /// This method calls into `compute_digest_with_algorithm()` using the
    /// digest algorithm stored in this instance.
    pub fn compute_digest(&self, content: Option<&[u8]>) -> Digest {
        self.compute_digest_with_algorithm(content, self.digest_algorithm)
    }

    /// Compute a message digest using an explicit digest algorithm.
    ///
    /// This will compute the hash/digest of the passed in content.
    pub fn compute_digest_with_algorithm(
        &self,
        content: Option<&[u8]>,
        alg: DigestAlgorithm,
    ) -> Digest {
        let mut hasher = alg.as_hasher();

        if let Some(content) = content {
            hasher.update(content);
        }

        hasher.finish()
    }
}

impl TryFrom<&crate::asn1::rfc5652::SignerInfo> for SignerInfo {
    type Error = CmsError;

    fn try_from(signer_info: &crate::asn1::rfc5652::SignerInfo) -> Result<Self, Self::Error> {
        let (issuer, serial_number) = match &signer_info.sid {
            SignerIdentifier::IssuerAndSerialNumber(issuer) => {
                (issuer.issuer.clone(), issuer.serial_number.clone())
            }
            SignerIdentifier::SubjectKeyIdentifier(_) => {
                return Err(CmsError::SubjectKeyIdentifierUnsupported);
            }
        };

        let digest_algorithm = DigestAlgorithm::try_from(&signer_info.digest_algorithm)?;
        let signature_algorithm = SignatureAlgorithm::try_from(&signer_info.signature_algorithm)?;
        let signature = signer_info.signature.to_bytes().to_vec();

        let signed_attributes = if let Some(attributes) = &signer_info.signed_attributes {
            // Content type attribute MUST be present.
            let content_type = attributes
                .iter()
                .find(|attr| attr.typ == OID_CONTENT_TYPE)
                .ok_or(CmsError::MissingSignedAttributeContentType)?;

            // Content type attribute MUST have exactly 1 value.
            if content_type.values.len() != 1 {
                return Err(CmsError::MalformedSignedAttributeContentType);
            }

            let content_type = content_type
                .values
                .get(0)
                .unwrap()
                .deref()
                .clone()
                .decode(|cons| Oid::take_from(cons))
                .map_err(|_| CmsError::MalformedSignedAttributeContentType)?;

            // Message digest attribute MUST be present.
            let message_digest = attributes
                .iter()
                .find(|attr| attr.typ == OID_MESSAGE_DIGEST)
                .ok_or(CmsError::MissingSignedAttributeMessageDigest)?;

            // Message digest attribute MUST have exactly 1 value.
            if message_digest.values.len() != 1 {
                return Err(CmsError::MalformedSignedAttributeMessageDigest);
            }

            let message_digest = message_digest
                .values
                .get(0)
                .unwrap()
                .deref()
                .clone()
                .decode(|cons| OctetString::take_from(cons))
                .map_err(|_| CmsError::MalformedSignedAttributeMessageDigest)?
                .to_bytes()
                .to_vec();

            // Signing time is optional, but common. So we pull it out for convenience.
            let signing_time = attributes
                .iter()
                .find(|attr| attr.typ == OID_SIGNING_TIME)
                .map(|attr| {
                    if attr.values.len() != 1 {
                        Err(CmsError::MalformedSignedAttributeSigningTime)
                    } else {
                        let time = attr
                            .values
                            .get(0)
                            .unwrap()
                            .deref()
                            .clone()
                            .decode(|cons| Time::take_from(cons))?;

                        let time = chrono::DateTime::from(time);

                        Ok(time)
                    }
                })
                .transpose()?;

            Some(SignedAttributes {
                content_type,
                message_digest,
                signing_time,
            })
        } else {
            None
        };

        let digested_signed_attributes_data = signer_info.signed_attributes_digested_content()?;

        let unsigned_attributes =
            if let Some(attributes) = &signer_info.unsigned_attributes {
                let time_stamp_token =
                    attributes
                        .iter()
                        .find(|attr| attr.typ == OID_TIME_STAMP_TOKEN)
                        .map(|attr| {
                            if attr.values.len() != 1 {
                                Err(CmsError::MalformedUnsignedAttributeTimeStampToken)
                            } else {
                                Ok(attr.values.get(0).unwrap().deref().clone().decode(|cons| {
                                    crate::asn1::rfc5652::SignedData::decode(cons)
                                })?)
                            }
                        })
                        .transpose()?;

                Some(UnsignedAttributes { time_stamp_token })
            } else {
                None
            };

        Ok(SignerInfo {
            issuer,
            serial_number,
            digest_algorithm,
            signature_algorithm,
            signature,
            signed_attributes,
            digested_signed_attributes_data,
            unsigned_attributes,
        })
    }
}

/// Represents the contents of a CMS SignedAttributes structure.
///
/// This is a high-level interface to the SignedAttributes ASN.1 type.
#[derive(Clone, Debug)]
pub struct SignedAttributes {
    /// The content type of the value being signed.
    ///
    /// This is often `OID_ID_DATA`.
    content_type: Oid,

    /// Holds the digest of the content that was signed.
    message_digest: Vec<u8>,

    /// The time the signature was created.
    signing_time: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Clone, Debug)]
pub struct UnsignedAttributes {
    /// Time-Stamp Token from a Time-Stamp Protocol server.
    time_stamp_token: Option<crate::asn1::rfc5652::SignedData>,
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        bcder::{encode::Values, Mode},
    };

    // This signature was extracted from the Firefox.app/Contents/MacOS/firefox
    // Mach-O executable on a aarch64 machine.
    const FIREFOX_SIGNATURE: &[u8] = include_bytes!("firefox.der");

    #[test]
    fn parse_firefox() {
        let raw = crate::asn1::rfc5652::SignedData::decode_ber(FIREFOX_SIGNATURE).unwrap();

        // Try to round trip it.
        let mut buffer = Vec::new();
        raw.encode_ref()
            .write_encoded(Mode::Ber, &mut buffer)
            .unwrap();

        // The bytes aren't identical because we use definite length encoding, so we can't
        // compare that. But we can compare the parsed objects for equivalence.

        let raw2 = crate::asn1::rfc5652::SignedData::decode_ber(&buffer).unwrap();
        assert_eq!(raw, raw2, "BER round tripping is identical");

        let signed_data = SignedData::parse_ber(FIREFOX_SIGNATURE).unwrap();

        for signer in signed_data.signers.iter() {
            signer
                .verify_signature_with_signed_data(&signed_data)
                .unwrap();

            // The message-digest does NOT match the encapsulated data in Apple code
            // signature's use of CMS. So digest verification will fail.
            signer
                .verify_message_digest_with_signed_data(&signed_data)
                .unwrap_err();
        }
    }
}
