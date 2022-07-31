// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Functionality for signing data. */

use {
    crate::{
        asn1::{
            rfc3161::OID_TIME_STAMP_TOKEN,
            rfc5652::{
                CertificateChoices, CertificateSet, CmsVersion, DigestAlgorithmIdentifier,
                DigestAlgorithmIdentifiers, EncapsulatedContentInfo, IssuerAndSerialNumber,
                SignatureValue, SignedAttributes, SignedData, SignerIdentifier, SignerInfo,
                SignerInfos, UnsignedAttributes, OID_CONTENT_TYPE, OID_ID_DATA, OID_ID_SIGNED_DATA,
                OID_MESSAGE_DIGEST, OID_SIGNING_TIME,
            },
        },
        time_stamp_protocol::{time_stamp_message_http, TimeStampError},
        CmsError,
    },
    bcder::{
        encode::{PrimitiveContent, Values},
        Captured, Mode, OctetString, Oid,
    },
    bytes::Bytes,
    reqwest::IntoUrl,
    std::collections::HashSet,
    x509_certificate::{
        asn1time::UtcTime,
        rfc5652::{Attribute, AttributeValue},
        CapturedX509Certificate, DigestAlgorithm, KeyInfoSigner, SignatureAlgorithm,
    },
};

/// Builder type to construct an entity that will sign some data.
///
/// Instances will be attached to `SignedDataBuilder` instances where they
/// will sign data using configured settings.
#[derive(Clone)]
pub struct SignerBuilder<'a> {
    /// The cryptographic key pair used for signing content.
    signing_key: &'a dyn KeyInfoSigner,

    /// X.509 certificate used for signing.
    signing_certificate: CapturedX509Certificate,

    /// Content digest algorithm to use.
    digest_algorithm: DigestAlgorithm,

    /// Explicit content to use for calculating the `message-id`
    /// attribute.
    message_id_content: Option<Vec<u8>>,

    /// The content type of the value being signed.
    ///
    /// This is a mandatory field for signed attributes. The default value
    /// is `id-data`.
    content_type: Oid,

    /// Extra attributes to include in the SignedAttributes set.
    extra_signed_attributes: Vec<Attribute>,

    /// Time-Stamp Protocol (TSP) server HTTP URL to use.
    time_stamp_url: Option<reqwest::Url>,
}

impl<'a> SignerBuilder<'a> {
    /// Construct a new entity that will sign content.
    ///
    /// An entity is constructed from a signing key, which is mandatory.
    pub fn new(
        signing_key: &'a dyn KeyInfoSigner,
        signing_certificate: CapturedX509Certificate,
    ) -> Self {
        Self {
            signing_key,
            signing_certificate,
            digest_algorithm: DigestAlgorithm::Sha256,
            message_id_content: None,
            content_type: Oid(Bytes::copy_from_slice(OID_ID_DATA.as_ref())),
            extra_signed_attributes: Vec::new(),
            time_stamp_url: None,
        }
    }

    /// Obtain the signature algorithm used by the signing key.
    pub fn signature_algorithm(&self) -> Result<SignatureAlgorithm, CmsError> {
        Ok(self.signing_key.signature_algorithm()?)
    }

    /// Define the content to use to calculate the `message-id` attribute.
    ///
    /// In most cases, this is never called and the encapsulated content
    /// embedded within the generated message is used. However, some users
    /// omit storing the data inline and instead use a `message-id` digest
    /// calculated from a different source. This defines that different source.
    #[must_use]
    pub fn message_id_content(mut self, data: Vec<u8>) -> Self {
        self.message_id_content = Some(data);
        self
    }

    /// Define the content type of the signed content.
    #[must_use]
    pub fn content_type(mut self, oid: Oid) -> Self {
        self.content_type = oid;
        self
    }

    /// Add an additional attribute to sign.
    #[must_use]
    pub fn signed_attribute(mut self, typ: Oid, values: Vec<AttributeValue>) -> Self {
        self.extra_signed_attributes.push(Attribute { typ, values });
        self
    }

    /// Add an additional OctetString signed attribute.
    ///
    /// This is a helper for converting a byte slice to an OctetString and AttributeValue
    /// without having to go through low-level ASN.1 code.
    #[must_use]
    pub fn signed_attribute_octet_string(self, typ: Oid, data: &[u8]) -> Self {
        self.signed_attribute(
            typ,
            vec![AttributeValue::new(Captured::from_values(
                Mode::Der,
                data.encode_ref(),
            ))],
        )
    }

    /// Obtain a time-stamp token from a server.
    ///
    /// If this is called, the URL must be a server implementing the Time-Stamp Protocol
    /// (TSP) as defined by RFC 3161. At signature generation time, the server will be
    /// contacted and the time stamp token response will be added as an unsigned attribute
    /// on the [SignedData] instance.
    pub fn time_stamp_url(mut self, url: impl IntoUrl) -> Result<Self, reqwest::Error> {
        self.time_stamp_url = Some(url.into_url()?);
        Ok(self)
    }
}

/// Encapsulated content to sign.
enum SignedContent {
    /// No content is being signed.
    None,

    /// Signed content to be embedded in the signature.
    Inline(Vec<u8>),

    /// Signed content whose digest is to be captured but won't be included in the signature.
    ///
    /// Internal value is the raw content, not the digest.
    External(Vec<u8>),
}

/// Entity for incrementally deriving a SignedData primitive.
///
/// Use this type for generating an RFC 5652 payload for signed data.
///
/// By default, the encapsulated content to sign is empty. Call [Self::content_inline()]
/// or [Self::content_external()] to define encapsulated content.
pub struct SignedDataBuilder<'a> {
    /// Encapsulated content to sign.
    signed_content: SignedContent,

    /// Entities who will generated signatures.
    signers: Vec<SignerBuilder<'a>>,

    /// X.509 certificates to add to the payload.
    certificates: Vec<CapturedX509Certificate>,

    /// The OID to use for `ContentInfo.contentType`.
    ///
    /// This is supposed to be `signed-data` when there are signatures
    /// present. But not all data producers use the same OID and this
    /// can cause problems. So we allow overriding the default.
    content_type: Oid,
}

impl<'a> Default for SignedDataBuilder<'a> {
    fn default() -> Self {
        Self {
            signed_content: SignedContent::None,
            signers: vec![],
            certificates: vec![],
            content_type: Oid(OID_ID_SIGNED_DATA.as_ref().into()),
        }
    }
}

impl<'a> SignedDataBuilder<'a> {
    /// Define encapsulated content that will be stored inline in the produced signature.
    #[must_use]
    pub fn content_inline(mut self, content: Vec<u8>) -> Self {
        self.signed_content = SignedContent::Inline(content);
        self
    }

    /// Define encapsulated content that won't be present in the produced signature.
    ///
    /// The content will be digested and that digest conveyed in the built signature.
    /// But the content itself won't be present in the signature. RFC 5652 refers to
    /// this as an _external signature_.
    #[must_use]
    pub fn content_external(mut self, content: Vec<u8>) -> Self {
        self.signed_content = SignedContent::External(content);
        self
    }

    /// Add a signer.
    ///
    /// The signer is the thing generating the cryptographic signature over
    /// data to be signed.
    #[must_use]
    pub fn signer(mut self, signer: SignerBuilder<'a>) -> Self {
        self.signers.push(signer);
        self
    }

    /// Add a certificate defined by our crate's Certificate type.
    #[must_use]
    pub fn certificate(mut self, cert: CapturedX509Certificate) -> Self {
        if !self.certificates.iter().any(|x| x == &cert) {
            self.certificates.push(cert);
        }

        self
    }

    /// Add multiple certificates to the certificates chain.
    #[must_use]
    pub fn certificates(mut self, certs: impl Iterator<Item = CapturedX509Certificate>) -> Self {
        for cert in certs {
            if !self.certificates.iter().any(|x| x == &cert) {
                self.certificates.push(cert);
            }
        }

        self
    }

    /// Force the OID for the `ContentInfo.contentType` field.
    #[must_use]
    pub fn content_type(mut self, oid: Oid) -> Self {
        self.content_type = oid;
        self
    }

    /// Construct a DER-encoded ASN.1 document containing a `SignedData` object.
    ///
    /// RFC 5652 says `SignedData` is BER encoded. However, DER is a stricter subset
    /// of BER. DER encodings are valid BER. So producing DER encoded data is perfectly
    /// valid. We choose to go with the more well-defined encoding format.
    pub fn build_der(&self) -> Result<Vec<u8>, CmsError> {
        let mut signer_infos = SignerInfos::default();
        let mut seen_digest_algorithms = HashSet::new();
        let mut seen_certificates = self.certificates.clone();

        for signer in &self.signers {
            seen_digest_algorithms.insert(signer.digest_algorithm);

            if !seen_certificates
                .iter()
                .any(|x| x == &signer.signing_certificate)
            {
                seen_certificates.push(signer.signing_certificate.clone());
            }

            let version = CmsVersion::V1;
            let digest_algorithm = DigestAlgorithmIdentifier {
                algorithm: signer.digest_algorithm.into(),
                parameters: None,
            };

            let sid = SignerIdentifier::IssuerAndSerialNumber(IssuerAndSerialNumber {
                issuer: signer.signing_certificate.issuer_name().clone(),
                serial_number: signer.signing_certificate.serial_number_asn1().clone(),
            });

            // The message digest attribute is mandatory.
            //
            // Message digest is computed from override content on the signer
            // or the encapsulated content if present. The "empty" hash is a
            // valid value if no content (only signed attributes) are being signed.
            let mut hasher = signer.digest_algorithm.digester();
            if let Some(content) = &signer.message_id_content {
                hasher.update(content);
            } else {
                match &self.signed_content {
                    SignedContent::None => {}
                    SignedContent::Inline(content) | SignedContent::External(content) => {
                        hasher.update(content)
                    }
                }
            }
            let digest = hasher.finish();

            let mut signed_attributes = SignedAttributes::default();

            // The content-type field is mandatory.
            signed_attributes.push(Attribute {
                typ: Oid(Bytes::copy_from_slice(OID_CONTENT_TYPE.as_ref())),
                values: vec![AttributeValue::new(Captured::from_values(
                    Mode::Der,
                    signer.content_type.encode_ref(),
                ))],
            });

            // Set `messageDigest` field
            signed_attributes.push(Attribute {
                typ: Oid(Bytes::copy_from_slice(OID_MESSAGE_DIGEST.as_ref())),
                values: vec![AttributeValue::new(Captured::from_values(
                    Mode::Der,
                    digest.as_ref().encode(),
                ))],
            });

            // Add signing time because it is common to include.
            signed_attributes.push(Attribute {
                typ: Oid(Bytes::copy_from_slice(OID_SIGNING_TIME.as_ref())),
                values: vec![AttributeValue::new(Captured::from_values(
                    Mode::Der,
                    UtcTime::now().encode(),
                ))],
            });

            signed_attributes.extend(signer.extra_signed_attributes.iter().cloned());

            // According to RFC 5652, signed attributes are DER encoded. This means a SET
            // (which SignedAttributes is) should be sorted. But bcder doesn't appear to do
            // this. So we manually sort here.
            let signed_attributes = signed_attributes.as_sorted()?;

            let signed_attributes = Some(signed_attributes);

            let signature_algorithm = signer.signature_algorithm()?.into();

            // The function for computing the signed attributes digested content
            // is on SignerInfo. So construct an instance so we can compute the
            // signature.
            let mut signer_info = SignerInfo {
                version,
                sid,
                digest_algorithm,
                signed_attributes,
                signature_algorithm,
                signature: SignatureValue::new(Bytes::copy_from_slice(&[])),
                unsigned_attributes: None,
                signed_attributes_data: None,
            };

            // The content being signed is the DER encoded signed attributes, if present, or the
            // encapsulated content. Since we always create signed attributes above, it *must* be
            // the DER encoded signed attributes.
            let signed_content = signer_info
                .signed_attributes_digested_content()?
                .expect("presence of signed attributes should ensure this is Some(T)");

            let signature = signer.signing_key.try_sign(&signed_content)?;
            let signature_algorithm = signer.signing_key.signature_algorithm()?;

            signer_info.signature = SignatureValue::new(Bytes::from(signature.clone()));
            signer_info.signature_algorithm = signature_algorithm.into();

            if let Some(url) = &signer.time_stamp_url {
                // The message sent to the TSA (via a digest) is the signature of the signed data.
                let res = time_stamp_message_http(
                    url.clone(),
                    signature.as_ref(),
                    signer.digest_algorithm,
                )?;

                if !res.is_success() {
                    return Err(TimeStampError::Unsuccessful(res.clone()).into());
                }

                let signed_data = res
                    .signed_data()?
                    .ok_or(CmsError::TimeStampProtocol(TimeStampError::BadResponse))?;

                let mut unsigned_attributes = UnsignedAttributes::default();
                unsigned_attributes.push(Attribute {
                    typ: Oid(Bytes::copy_from_slice(OID_TIME_STAMP_TOKEN.as_ref())),
                    values: vec![AttributeValue::new(Captured::from_values(
                        Mode::Der,
                        signed_data.encode_ref(),
                    ))],
                });

                signer_info.unsigned_attributes = Some(unsigned_attributes);
            }

            signer_infos.push(signer_info);
        }

        let mut digest_algorithms = DigestAlgorithmIdentifiers::default();
        digest_algorithms.extend(seen_digest_algorithms.into_iter().map(|alg| {
            DigestAlgorithmIdentifier {
                algorithm: alg.into(),
                parameters: None,
            }
        }));

        // Many consumers prefer the issuing certificate to come before the issued
        // certificate. So we explicitly sort all the seen certificates in this order,
        // attempting for all issuing certificates to come before the issued.
        seen_certificates.sort_by(|a, b| a.compare_issuer(b));

        let mut certificates = CertificateSet::default();
        certificates.extend(
            seen_certificates
                .into_iter()
                .map(|cert| CertificateChoices::Certificate(Box::new(cert.into()))),
        );

        // The certificates could have been encountered in any order. For best results,
        // we want issuer certificates before their "children." So we apply sorting here.

        let signed_data = SignedData {
            version: CmsVersion::V1,
            digest_algorithms,
            content_info: EncapsulatedContentInfo {
                content_type: self.content_type.clone(),
                content: match &self.signed_content {
                    SignedContent::None | SignedContent::External(_) => None,
                    SignedContent::Inline(content) => {
                        Some(OctetString::new(Bytes::copy_from_slice(content)))
                    }
                },
            },
            certificates: if certificates.is_empty() {
                None
            } else {
                Some(certificates)
            },
            crls: None,
            signer_infos,
        };

        let mut ber = Vec::new();
        signed_data
            .encode_ref()
            .write_encoded(Mode::Der, &mut ber)?;

        Ok(ber)
    }
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        crate::SignedData,
        x509_certificate::{testutil::*, EcdsaCurve},
    };

    const DIGICERT_TIMESTAMP_URL: &str = "http://timestamp.digicert.com";

    #[test]
    fn simple_rsa_signature_inline() {
        let key = rsa_private_key();
        let cert = rsa_cert();

        let signer = SignerBuilder::new(&key, cert);

        let ber = SignedDataBuilder::default()
            .content_inline(vec![42])
            .signer(signer)
            .build_der()
            .unwrap();

        let signed_data = crate::SignedData::parse_ber(&ber).unwrap();
        assert_eq!(signed_data.signed_content(), Some(vec![42].as_ref()));

        for signer in signed_data.signers() {
            signer
                .verify_message_digest_with_signed_data(&signed_data)
                .unwrap();
            signer
                .verify_signature_with_signed_data(&signed_data)
                .unwrap();
            assert!(signer.unsigned_attributes.is_none());
        }
    }

    #[test]
    fn simple_rsa_signature_external() {
        let key = rsa_private_key();
        let cert = rsa_cert();

        let signer = SignerBuilder::new(&key, cert);

        let ber = SignedDataBuilder::default()
            .content_external(vec![42])
            .signer(signer)
            .build_der()
            .unwrap();

        let signed_data = crate::SignedData::parse_ber(&ber).unwrap();
        assert!(signed_data.signed_content().is_none());

        for signer in signed_data.signers() {
            signer.verify_message_digest_with_content(&[42]).unwrap();
            signer
                .verify_signature_with_signed_data(&signed_data)
                .unwrap();
            assert!(signer.unsigned_attributes.is_none());
        }
    }

    #[test]
    fn time_stamp_url() {
        let key = rsa_private_key();
        let cert = rsa_cert();

        let signer = SignerBuilder::new(&key, cert)
            .time_stamp_url(DIGICERT_TIMESTAMP_URL)
            .unwrap();

        let ber = SignedDataBuilder::default()
            .content_inline(vec![42])
            .signer(signer)
            .build_der()
            .unwrap();

        let signed_data = crate::SignedData::parse_ber(&ber).unwrap();

        for signer in signed_data.signers() {
            let unsigned = signer.unsigned_attributes().unwrap();
            let tst = unsigned.time_stamp_token.as_ref().unwrap();
            assert!(tst.certificates.is_some());

            let tst_signed_data = signer.time_stamp_token_signed_data().unwrap().unwrap();
            for signer in tst_signed_data.signers() {
                signer
                    .verify_message_digest_with_signed_data(&tst_signed_data)
                    .unwrap();
                signer
                    .verify_signature_with_signed_data(&tst_signed_data)
                    .unwrap();
            }

            assert!(signer.verify_time_stamp_token().unwrap().is_some());
        }
    }

    #[test]
    fn simple_ecdsa_signature() {
        for curve in EcdsaCurve::all() {
            let (cert, key) = self_signed_ecdsa_key_pair(Some(*curve));

            let cms = SignedDataBuilder::default()
                .content_inline("hello world".as_bytes().to_vec())
                .certificate(cert.clone())
                .signer(SignerBuilder::new(&key, cert))
                .build_der()
                .unwrap();

            let signed_data = SignedData::parse_ber(&cms).unwrap();

            for signer in signed_data.signers() {
                signer
                    .verify_signature_with_signed_data(&signed_data)
                    .unwrap();
            }
        }
    }

    #[test]
    fn simple_ed25519_signature() {
        let (cert, key) = self_signed_ed25519_key_pair();

        let cms = SignedDataBuilder::default()
            .content_inline("hello world".as_bytes().to_vec())
            .certificate(cert.clone())
            .signer(SignerBuilder::new(&key, cert))
            .build_der()
            .unwrap();

        let signed_data = SignedData::parse_ber(&cms).unwrap();

        for signer in signed_data.signers() {
            signer
                .verify_signature_with_signed_data(&signed_data)
                .unwrap();
        }
    }
}
