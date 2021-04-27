// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Defines high-level interface to X.509 certificates.

use {
    crate::{
        algorithm::{KeyAlgorithm, SignatureAlgorithm},
        rfc3280::Name,
        rfc5280::Certificate,
        X509CertificateError as Error,
    },
    bcder::{decode::Constructed, encode::Values, int::Integer, Mode},
    bytes::Bytes,
    ring::signature,
    std::{
        cmp::Ordering,
        convert::TryFrom,
        io::Write,
        ops::{Deref, DerefMut},
    },
};

/// Provides an interface to the RFC 5280 [Certificate] ASN.1 type.
///
/// This type provides the main high-level API that this crate exposes
/// for reading and writing X.509 certificates.
///
/// Instances are backed by an actual ASN.1 [Certificate] instance.
/// Read operations are performed against the raw ASN.1 values. Mutations
/// result in mutations of the ASN.1 data structures.
///
/// Instances can be converted to/from [Certificate] using traits.
/// [AsRef]/[AsMut] are implemented to obtain a reference to the backing
/// [Certificate].
///
/// We have chosen not to implement [Deref]/[DerefMut] because we don't
/// want to pollute the type's API with lower-level ASN.1 primitives.
///
/// This type does not track the original data from which it came.
/// If you want a type that does that, consider [CapturedX509Certificate],
/// which implements [Deref] and therefore behaves like this type.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct X509Certificate(Certificate);

impl X509Certificate {
    /// Construct an instance by parsing DER encoded ASN.1 data.
    pub fn from_der(data: impl AsRef<[u8]>) -> Result<Self, Error> {
        let cert = Constructed::decode(data.as_ref(), Mode::Der, |cons| {
            Certificate::take_from(cons)
        })?;

        Ok(Self(cert))
    }

    /// Construct an instance by parsing BER encoded ASN.1 data.
    ///
    /// X.509 certificates are likely (and should be) using DER encoding.
    /// However, some specifications do mandate the use of BER, so this
    /// method is provided.
    pub fn from_ber(data: impl AsRef<[u8]>) -> Result<Self, Error> {
        let cert = Constructed::decode(data.as_ref(), Mode::Ber, |cons| {
            Certificate::take_from(cons)
        })?;

        Ok(Self(cert))
    }

    /// Construct an instance by parsing PEM encoded ASN.1 data.
    ///
    /// The data is a human readable string likely containing
    /// `--------- BEGIN CERTIFICATE ----------`.
    pub fn from_pem(data: impl AsRef<[u8]>) -> Result<Self, Error> {
        let data = pem::parse(data.as_ref()).map_err(Error::PemDecode)?;

        Self::from_der(&data.contents)
    }

    /// Construct instances by parsing PEM with potentially multiple records.
    ///
    /// By default, we only look for `--------- BEGIN CERTIFICATE --------`
    /// entries and silently ignore unknown ones. If you would like to specify
    /// an alternate set of tags (this is the value after the `BEGIN`) to search,
    /// call [Self::from_pem_multiple_tags].
    pub fn from_pem_multiple(data: impl AsRef<[u8]>) -> Result<Vec<Self>, Error> {
        Self::from_pem_multiple_tags(data, &["CERTIFICATE"])
    }

    /// Construct instances by parsing PEM armored DER encoded certificates with specific PEM tags.
    ///
    /// This is like [Self::from_pem_multiple] except you control the filter for
    /// which `BEGIN <tag>` values are filtered through to the DER parser.
    pub fn from_pem_multiple_tags(
        data: impl AsRef<[u8]>,
        tags: &[&str],
    ) -> Result<Vec<Self>, Error> {
        let pem = pem::parse_many(data.as_ref());

        pem.into_iter()
            .filter(|pem| tags.contains(&pem.tag.as_str()))
            .map(|pem| Self::from_der(&pem.contents))
            .collect::<Result<_, _>>()
    }

    /// Obtain the serial number as the ASN.1 [Integer] type.
    pub fn serial_number_asn1(&self) -> &Integer {
        &self.0.tbs_certificate.serial_number
    }

    /// Obtain the certificate's subject, as its ASN.1 [Name] type.
    pub fn subject_name(&self) -> &Name {
        &self.0.tbs_certificate.subject
    }

    /// Obtain the certificate's issuer, as its ASN.1 [Name] type.
    pub fn issuer_name(&self) -> &Name {
        &self.0.tbs_certificate.issuer
    }

    /// Encode the certificate data structure using DER encoding.
    ///
    /// (This is the common ASN.1 encoding format for X.509 certificates.)
    ///
    /// This always serializes the internal ASN.1 data structure. If you
    /// call this on a wrapper type that has retained a copy of the original
    /// data, this may emit different data than that copy.
    pub fn encode_der_to(&self, fh: &mut impl Write) -> Result<(), std::io::Error> {
        self.0.encode_ref().write_encoded(Mode::Der, fh)
    }

    /// Encode the certificate data structure use BER encoding.
    pub fn encode_ber_to(&self, fh: &mut impl Write) -> Result<(), std::io::Error> {
        self.0.encode_ref().write_encoded(Mode::Ber, fh)
    }

    /// Encode the internal ASN.1 data structures to DER.
    pub fn encode_der(&self) -> Result<Vec<u8>, std::io::Error> {
        let mut buffer = Vec::<u8>::new();
        self.encode_der_to(&mut buffer)?;

        Ok(buffer)
    }

    /// Obtain the BER encoded representation of this certificate.
    pub fn encode_ber(&self) -> Result<Vec<u8>, std::io::Error> {
        let mut buffer = Vec::<u8>::new();
        self.encode_ber_to(&mut buffer)?;

        Ok(buffer)
    }

    /// Encode the certificate to PEM.
    ///
    /// This will write a human-readable string with `------ BEGIN CERTIFICATE -------`
    /// armoring. This is a very common method for encoding certificates.
    ///
    /// The underlying binary data is DER encoded.
    pub fn write_pem(&self, fh: &mut impl Write) -> Result<(), std::io::Error> {
        let encoded = pem::encode(&pem::Pem {
            tag: "CERTIFICATE".to_string(),
            contents: self.encode_der()?,
        });

        fh.write_all(encoded.as_bytes())
    }

    /// Encode the certificate to a PEM string.
    pub fn encode_pem(&self) -> Result<String, std::io::Error> {
        Ok(pem::encode(&pem::Pem {
            tag: "CERTIFICATE".to_string(),
            contents: self.encode_der()?,
        }))
    }

    /// Attempt to resolve a known [KeyAlgorithm] used by the private key associated with this certificate.
    ///
    /// If this crate isn't aware of the OID associated with the key algorithm,
    /// `None` is returned.
    pub fn key_algorithm(&self) -> Option<KeyAlgorithm> {
        KeyAlgorithm::try_from(&self.0.tbs_certificate.subject_public_key_info.algorithm).ok()
    }

    /// Obtain the raw data constituting this certificate's public key.
    ///
    /// A copy of the data is returned.
    pub fn public_key_data(&self) -> Bytes {
        self.0
            .tbs_certificate
            .subject_public_key_info
            .subject_public_key
            .octet_bytes()
    }

    /// Compare 2 instances, sorting them so the issuer comes before the issued.
    ///
    /// This function examines the [Self::issuer_name] and [Self::subject_name]
    /// fields of 2 certificates, attempting to sort them so the issuing
    /// certificate comes before the issued certificate.
    ///
    /// This function performs a strict compare of the ASN.1 [Name] data.
    /// The assumption here is that the issuing certificate's subject [Name]
    /// is identical to the issued's issuer [Name]. This assumption is often
    /// true. But it likely isn't always true, so this function may not produce
    /// reliable results.
    pub fn compare_issuer(&self, other: &Self) -> Ordering {
        // Self signed certificate has no ordering.
        if self.0.tbs_certificate.subject == self.0.tbs_certificate.issuer {
            Ordering::Equal
            // We were issued by the other certificate. The issuer comes first.
        } else if self.0.tbs_certificate.issuer == other.0.tbs_certificate.subject {
            Ordering::Greater
        } else if self.0.tbs_certificate.subject == other.0.tbs_certificate.issuer {
            // We issued the other certificate. We come first.
            Ordering::Less
        } else {
            Ordering::Equal
        }
    }

    /// Whether the subject [Name] is also the issuer's [Name].
    ///
    /// This might be a way of determining if a certificate is self-signed.
    /// But there can likely be false negatives due to differences in ASN.1
    /// encoding of the underlying data. So we don't claim this is a test for
    /// being self-signed.
    pub fn subject_is_issuer(&self) -> bool {
        self.0.tbs_certificate.subject == self.0.tbs_certificate.issuer
    }
}

impl From<Certificate> for X509Certificate {
    fn from(v: Certificate) -> Self {
        Self(v)
    }
}

impl From<X509Certificate> for Certificate {
    fn from(v: X509Certificate) -> Self {
        v.0
    }
}

impl AsRef<Certificate> for X509Certificate {
    fn as_ref(&self) -> &Certificate {
        &self.0
    }
}

impl AsMut<Certificate> for X509Certificate {
    fn as_mut(&mut self) -> &mut Certificate {
        &mut self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum OriginalData {
    Ber(Vec<u8>),
    Der(Vec<u8>),
}

/// Represents an immutable (read-only) X.509 certificate that was parsed from data.
///
/// This type implements [Deref] but not [DerefMut], so only functions
/// taking a non-mutable instance are usable.
///
/// A copy of the certificate's raw backing data is stored, facilitating
/// subsequent access.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CapturedX509Certificate {
    original: OriginalData,
    inner: X509Certificate,
}

impl CapturedX509Certificate {
    /// Construct an instance from DER encoded data.
    ///
    /// A copy of this data will be stored in the instance and is guaranteed
    /// to be immutable for the lifetime of the instance. The original constructing
    /// data can be retrieved later.
    pub fn from_der(data: impl Into<Vec<u8>>) -> Result<Self, Error> {
        let der_data = data.into();

        let inner = X509Certificate::from_der(&der_data)?;

        Ok(Self {
            original: OriginalData::Der(der_data),
            inner,
        })
    }

    /// Construct an instance from BER encoded data.
    ///
    /// A copy of this data will be stored in the instance and is guaranteed
    /// to be immutable for the lifetime of the instance, allowing it to
    /// be retrieved later.
    pub fn from_ber(data: impl Into<Vec<u8>>) -> Result<Self, Error> {
        let data = data.into();

        let inner = X509Certificate::from_ber(&data)?;

        Ok(Self {
            original: OriginalData::Ber(data),
            inner,
        })
    }

    /// Construct an instance by parsing PEM encoded ASN.1 data.
    ///
    /// The data is a human readable string likely containing
    /// `--------- BEGIN CERTIFICATE ----------`.
    pub fn from_pem(data: impl AsRef<[u8]>) -> Result<Self, Error> {
        let data = pem::parse(data.as_ref()).map_err(Error::PemDecode)?;

        Self::from_der(data.contents)
    }

    /// Construct instances by parsing PEM with potentially multiple records.
    ///
    /// By default, we only look for `--------- BEGIN CERTIFICATE --------`
    /// entries and silently ignore unknown ones. If you would like to specify
    /// an alternate set of tags (this is the value after the `BEGIN`) to search,
    /// call [Self::from_pem_multiple_tags].
    pub fn from_pem_multiple(data: impl AsRef<[u8]>) -> Result<Vec<Self>, Error> {
        Self::from_pem_multiple_tags(data, &["CERTIFICATE"])
    }

    /// Construct instances by parsing PEM armored DER encoded certificates with specific PEM tags.
    ///
    /// This is like [Self::from_pem_multiple] except you control the filter for
    /// which `BEGIN <tag>` values are filtered through to the DER parser.
    pub fn from_pem_multiple_tags(
        data: impl AsRef<[u8]>,
        tags: &[&str],
    ) -> Result<Vec<Self>, Error> {
        let pem = pem::parse_many(data.as_ref());

        pem.into_iter()
            .filter(|pem| tags.contains(&pem.tag.as_str()))
            .map(|pem| Self::from_der(pem.contents))
            .collect::<Result<_, _>>()
    }

    /// Obtain the DER data that was used to construct this instance.
    ///
    /// The data is guaranteed to not have been modified since the instance
    /// was constructed.
    pub fn constructed_data(&self) -> &[u8] {
        match &self.original {
            OriginalData::Ber(data) => data,
            OriginalData::Der(data) => data,
        }
    }

    /// Encode the original contents of this certificate to PEM.
    pub fn encode_pem(&self) -> String {
        pem::encode(&pem::Pem {
            tag: "CERTIFICATE".to_string(),
            contents: self.constructed_data().to_vec(),
        })
    }

    /// Verify that another certificate, `other`, signed this certificate.
    ///
    /// If this is a self-signed certificate, you can pass `self` as the 2nd
    /// argument.
    ///
    /// This function isn't exposed on [X509Certificate] because the exact
    /// bytes constituting the certificate's internals need to be consulted
    /// to verify signatures. And since this type tracks the underlying
    /// bytes, we are guaranteed to have a pristine copy.
    pub fn verify_signed_by_certificate(
        &self,
        other: impl AsRef<X509Certificate>,
    ) -> Result<(), Error> {
        let public_key = other
            .as_ref()
            .0
            .tbs_certificate
            .subject_public_key_info
            .subject_public_key
            .octet_bytes();

        self.verify_signed_by_public_key(public_key)
    }

    /// Verifies that this certificate was cryptographically signed using raw public key data from a signing key.
    ///
    /// This function does the low-level work of extracting the signature and
    /// verification details from the current certificate and figuring out
    /// the correct combination of cryptography settings to apply to perform
    /// signature verification.
    ///
    /// In many cases, an X.509 certificate is signed by another certificate. And
    /// since the public key is embedded in the X.509 certificate, it is easier
    /// to go through [Self::verify_signed_by_certificate] instead.
    pub fn verify_signed_by_public_key(
        &self,
        public_key_data: impl AsRef<[u8]>,
    ) -> Result<(), Error> {
        // Always verify against the original content, as the inner
        // certificate could be mutated via the mutable wrapper of this
        // type.
        let this_cert = match &self.original {
            OriginalData::Ber(data) => X509Certificate::from_ber(data),
            OriginalData::Der(data) => X509Certificate::from_der(data),
        }
        .expect("certificate re-parse should never fail");

        let signed_data = this_cert
            .0
            .tbs_certificate
            .raw_data
            .as_ref()
            .expect("original certificate data should have persisted as part of re-parse");
        let signature = this_cert.0.signature.octet_bytes();

        let signature_algorithm = SignatureAlgorithm::try_from(&this_cert.0.signature_algorithm)?;
        let verify_algorithm = signature_algorithm.into();

        let public_key = signature::UnparsedPublicKey::new(verify_algorithm, public_key_data);

        public_key
            .verify(&signed_data, &signature)
            .map_err(|_| Error::CertificateSignatureVerificationFailed)
    }
}

impl Deref for CapturedX509Certificate {
    type Target = X509Certificate;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl AsRef<X509Certificate> for CapturedX509Certificate {
    fn as_ref(&self) -> &X509Certificate {
        &self.inner
    }
}

impl TryFrom<&X509Certificate> for CapturedX509Certificate {
    type Error = Error;

    fn try_from(cert: &X509Certificate) -> Result<Self, Self::Error> {
        let mut buffer = Vec::<u8>::new();
        cert.encode_der_to(&mut buffer)?;

        Self::from_der(buffer)
    }
}

impl TryFrom<X509Certificate> for CapturedX509Certificate {
    type Error = Error;

    fn try_from(cert: X509Certificate) -> Result<Self, Self::Error> {
        let mut buffer = Vec::<u8>::new();
        cert.encode_der_to(&mut buffer)?;

        Self::from_der(buffer)
    }
}

impl From<CapturedX509Certificate> for Certificate {
    fn from(cert: CapturedX509Certificate) -> Self {
        cert.inner.0
    }
}

/// Provides a mutable wrapper to an X.509 certificate that was parsed from data.
///
/// This is like [CapturedX509Certificate] except it implements [DerefMut],
/// enabling you to modify the certificate while still being able to access
/// the raw data the certificate is backed by. However, mutations are
/// only performed against the parsed ASN.1 data structure, not the original
/// data it was constructed with.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MutableX509Certificate(CapturedX509Certificate);

impl Deref for MutableX509Certificate {
    type Target = X509Certificate;

    fn deref(&self) -> &Self::Target {
        &self.0.inner
    }
}

impl DerefMut for MutableX509Certificate {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0.inner
    }
}

impl From<CapturedX509Certificate> for MutableX509Certificate {
    fn from(cert: CapturedX509Certificate) -> Self {
        Self(cert)
    }
}

/// Whether one certificate is a subset of another certificate.
///
/// This returns true iff the two certificates have the same serial number
/// and every `Name` attribute in the first certificate is present in the other.
pub fn certificate_is_subset_of(
    a_serial: &Integer,
    a_name: &Name,
    b_serial: &Integer,
    b_name: &Name,
) -> bool {
    if a_serial != b_serial {
        return false;
    }

    let Name::RdnSequence(a_sequence) = &a_name;
    let Name::RdnSequence(b_sequence) = &b_name;

    a_sequence.iter().all(|rdn| b_sequence.contains(rdn))
}
