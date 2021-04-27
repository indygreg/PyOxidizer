// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Functionality related to certificates.

use {
    crate::error::AppleCodesignError,
    bcder::{
        encode::{PrimitiveContent, Values},
        BitString, ConstOid, Mode, OctetString, Oid,
    },
    bytes::Bytes,
    ring::signature::{EcdsaKeyPair, Ed25519KeyPair, ECDSA_P256_SHA256_ASN1_SIGNING},
    x509_certificate::{
        asn1time::Time,
        rfc3280::Name,
        rfc5280::{
            AlgorithmIdentifier, Certificate, Extension, Extensions, SubjectPublicKeyInfo,
            TbsCertificate, Validity, Version,
        },
        rfc5958::OneAsymmetricKey,
        InMemorySigningKeyPair, KeyAlgorithm,
    },
};

/// Key Usage extension.
///
/// 2.5.29.15
const OID_EXTENSION_KEY_USAGE: ConstOid = Oid(&[85, 29, 15]);

/// Extended Key Usage extension.
///
/// 2.5.29.37
const OID_EXTENSION_EXTENDED_KEY_USAGE: ConstOid = Oid(&[85, 29, 37]);

/// Extended Key Usage purpose for code signing.
///
/// 1.3.6.1.5.5.7.3.3
const OID_PURPOSE_CODE_SIGNING: ConstOid = Oid(&[43, 6, 1, 5, 5, 7, 3, 3]);

/// OID used for email address in RDN in Apple generated code signing certificates.
const OID_EMAIL_ADDRESS: ConstOid = Oid(&[42, 134, 72, 134, 247, 13, 1, 9, 1]);

fn bmp_string(s: &str) -> Vec<u8> {
    let utf16: Vec<u16> = s.encode_utf16().collect();

    let mut bytes = Vec::with_capacity(utf16.len() * 2 + 2);
    for c in utf16 {
        bytes.push((c / 256) as u8);
        bytes.push((c % 256) as u8);
    }
    bytes.push(0x00);
    bytes.push(0x00);

    bytes
}

/// Parse PFX data into a key pair.
///
/// PFX data is commonly encountered in `.p12` files, such as those created
/// when exporting certificates from Apple's `Keychain Access` application.
///
/// The contents of the PFX file require a password to decrypt. However, if
/// no password was provided to create the PFX data, this password may be the
/// empty string.
pub fn parse_pfx_data(
    data: &[u8],
    password: &str,
) -> Result<
    (
        cryptographic_message_syntax::Certificate,
        InMemorySigningKeyPair,
    ),
    AppleCodesignError,
> {
    let pfx = p12::PFX::parse(data).map_err(|e| {
        AppleCodesignError::PfxParseError(format!("data does not appear to be PFX: {:?}", e))
    })?;

    if !pfx.verify_mac(password) {
        return Err(AppleCodesignError::PfxBadPassword);
    }

    // Apple's certificate export format consists of regular data content info
    // with inner ContentInfo components holding the key and certificate.
    let data = match pfx.auth_safe {
        p12::ContentInfo::Data(data) => data,
        _ => {
            return Err(AppleCodesignError::PfxParseError(
                "unexpected PFX content info".to_string(),
            ));
        }
    };

    let content_infos = yasna::parse_der(&data, |reader| {
        reader.collect_sequence_of(p12::ContentInfo::parse)
    })
    .map_err(|e| {
        AppleCodesignError::PfxParseError(format!("failed parsing inner ContentInfo: {:?}", e))
    })?;

    let bmp_password = bmp_string(password);

    let mut certificate = None;
    let mut signing_key = None;

    for content in content_infos {
        let bags_data = match content {
            p12::ContentInfo::Data(inner) => inner,
            p12::ContentInfo::EncryptedData(encrypted) => {
                encrypted.data(&bmp_password).ok_or_else(|| {
                    AppleCodesignError::PfxParseError(
                        "failed decrypting inner EncryptedData".to_string(),
                    )
                })?
            }
            p12::ContentInfo::OtherContext(_) => {
                return Err(AppleCodesignError::PfxParseError(
                    "unexpected OtherContent content in inner PFX data".to_string(),
                ));
            }
        };

        let bags = yasna::parse_ber(&bags_data, |reader| {
            reader.collect_sequence_of(p12::SafeBag::parse)
        })
        .map_err(|e| {
            AppleCodesignError::PfxParseError(format!(
                "failed parsing SafeBag within inner Data: {:?}",
                e
            ))
        })?;

        for bag in bags {
            match bag.bag {
                p12::SafeBagKind::CertBag(cert_bag) => match cert_bag {
                    p12::CertBag::X509(cert_data) => {
                        certificate = Some(cryptographic_message_syntax::Certificate::from_der(
                            &cert_data,
                        )?);
                    }
                    p12::CertBag::SDSI(_) => {
                        return Err(AppleCodesignError::PfxParseError(
                            "unexpected SDSI certificate data".to_string(),
                        ));
                    }
                },
                p12::SafeBagKind::Pkcs8ShroudedKeyBag(key_bag) => {
                    let decrypted = key_bag.decrypt(&bmp_password).ok_or_else(|| {
                        AppleCodesignError::PfxParseError(
                            "error decrypting PKCS8 shrouded key bag; is the password correct?"
                                .to_string(),
                        )
                    })?;

                    signing_key = Some(InMemorySigningKeyPair::from_pkcs8_der(&decrypted, None)?);
                }
                p12::SafeBagKind::OtherBagKind(_) => {
                    return Err(AppleCodesignError::PfxParseError(
                        "unexpected bag type in inner PFX content".to_string(),
                    ));
                }
            }
        }
    }

    match (certificate, signing_key) {
        (Some(certificate), Some(signing_key)) => Ok((certificate, signing_key)),
        (None, Some(_)) => Err(AppleCodesignError::PfxParseError(
            "failed to find x509 certificate in PFX data".to_string(),
        )),
        (_, None) => Err(AppleCodesignError::PfxParseError(
            "failed to find signing key in PFX data".to_string(),
        )),
    }
}

/// Create a new self-signed X.509 certificate suitable for signing code.
///
/// The created certificate contains all the extensions needed to convey
/// that it is used for code signing and should resemble certificates.
///
/// However, because the certificate isn't signed by Apple or another
/// trusted certificate authority, binaries signed with the certificate
/// may not pass Apple's verification requirements and the OS may refuse
/// to proceed. Needless to say, only use certificates generated with this
/// function for testing purposes only.
pub fn create_self_signed_code_signing_certificate(
    algorithm: KeyAlgorithm,
    common_name: &str,
    country_name: &str,
    email_address: &str,
    validity_duration: chrono::Duration,
) -> Result<
    (
        cryptographic_message_syntax::Certificate,
        InMemorySigningKeyPair,
        Vec<u8>,
    ),
    AppleCodesignError,
> {
    let system_random = ring::rand::SystemRandom::new();

    let key_pair_document = match algorithm {
        KeyAlgorithm::Ed25519 => Ed25519KeyPair::generate_pkcs8(&system_random)
            .map_err(AppleCodesignError::CertificateRing)?,
        KeyAlgorithm::Ecdsa => {
            let signing_algorithm = &ECDSA_P256_SHA256_ASN1_SIGNING;
            EcdsaKeyPair::generate_pkcs8(signing_algorithm, &system_random)
                .map_err(AppleCodesignError::CertificateRing)?
        }
        KeyAlgorithm::Rsa => {
            return Err(AppleCodesignError::CertificateUnsupportedKeyAlgorithm(
                algorithm,
            ));
        }
    };

    let key_pair_asn1 =
        bcder::decode::Constructed::decode(key_pair_document.as_ref(), Mode::Der, |cons| {
            OneAsymmetricKey::take_from(cons)
        })
        .map_err(AppleCodesignError::CertificateDecode)?;

    let signing_key = InMemorySigningKeyPair::from_pkcs8_der(key_pair_document.as_ref(), None)?;

    let mut name = Name::default();
    name.append_common_name_utf8_string(common_name)
        .map_err(AppleCodesignError::CertificateCharset)?;
    name.append_country_utf8_string(country_name)
        .map_err(AppleCodesignError::CertificateCharset)?;
    name.append_utf8_string(Oid(OID_EMAIL_ADDRESS.as_ref().into()), email_address)
        .map_err(AppleCodesignError::CertificateCharset)?;

    let now = chrono::Utc::now();
    let expires = now + validity_duration;

    let mut extensions = Extensions::default();

    // Digital Signature key usage extension.
    extensions.push(Extension {
        id: Oid(Bytes::from(OID_EXTENSION_KEY_USAGE.as_ref())),
        critical: Some(true),
        value: OctetString::new(Bytes::copy_from_slice(&[3, 2, 7, 128])),
    });

    let captured =
        bcder::encode::sequence(Oid(Bytes::from(OID_PURPOSE_CODE_SIGNING.as_ref())).encode())
            .to_captured(Mode::Ber);

    extensions.push(Extension {
        id: Oid(Bytes::from(OID_EXTENSION_EXTENDED_KEY_USAGE.as_ref())),
        critical: Some(true),
        value: OctetString::new(Bytes::copy_from_slice(captured.as_ref())),
    });

    let tbs_certificate = TbsCertificate {
        version: Version::V3,
        serial_number: 42.into(),
        signature: algorithm.default_signature_algorithm().into(),
        issuer: name.clone(),
        validity: Validity {
            not_before: Time::from(now),
            not_after: Time::from(expires),
        },
        subject: name,
        subject_public_key_info: SubjectPublicKeyInfo {
            algorithm: AlgorithmIdentifier {
                algorithm: key_pair_asn1.private_key_algorithm.algorithm.clone(),
                parameters: key_pair_asn1.private_key_algorithm.parameters,
            },
            subject_public_key: BitString::new(
                0,
                Bytes::copy_from_slice(signing_key.public_key_data()),
            ),
        },
        issuer_unique_id: None,
        subject_unique_id: None,
        extensions: Some(extensions),
        raw_data: None,
    };

    // We need to serialize the TBS certificate so we can sign it with the private
    // key and include its signature.
    let mut cert_ber = Vec::<u8>::new();
    tbs_certificate
        .encode_ref()
        .write_encoded(Mode::Ber, &mut cert_ber)?;

    let signature = signing_key.sign(&cert_ber)?;

    let cert = Certificate {
        tbs_certificate,
        signature_algorithm: algorithm.default_signature_algorithm().into(),
        signature: BitString::new(0, Bytes::copy_from_slice(signature.as_ref())),
    };

    let cert = cryptographic_message_syntax::Certificate::from_parsed_asn1(cert)?;

    Ok((cert, signing_key, key_pair_document.as_ref().to_vec()))
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        cryptographic_message_syntax::{SignedData, SignedDataBuilder, SignerBuilder},
    };

    #[test]
    fn parse_keychain_p12_export() {
        let data = include_bytes!("apple-codesign-testuser.p12");

        let err = parse_pfx_data(data, "bad-password").unwrap_err();
        assert!(matches!(err, AppleCodesignError::PfxBadPassword));

        parse_pfx_data(data, "password123").unwrap();
    }

    #[test]
    fn generate_self_signed_certificate_ecdsa() {
        create_self_signed_code_signing_certificate(
            KeyAlgorithm::Ecdsa,
            "test",
            "US",
            "nobody@example.com",
            chrono::Duration::hours(1),
        )
        .unwrap();
    }

    #[test]
    fn generate_self_signed_certificate_ed25519() {
        create_self_signed_code_signing_certificate(
            KeyAlgorithm::Ed25519,
            "test",
            "US",
            "nobody@example.com",
            chrono::Duration::hours(1),
        )
        .unwrap();
    }

    #[test]
    fn cms_self_signed_certificate_signing_ecdsa() {
        let (cert, signing_key, _) = create_self_signed_code_signing_certificate(
            KeyAlgorithm::Ecdsa,
            "test",
            "US",
            "nobody@example.com",
            chrono::Duration::hours(1),
        )
        .unwrap();

        let plaintext = "hello, world";

        let cms = SignedDataBuilder::default()
            .certificate(cert.clone())
            .unwrap()
            .signed_content(plaintext.as_bytes().to_vec())
            .signer(SignerBuilder::new(&signing_key, cert))
            .build_ber()
            .unwrap();

        let signed_data = SignedData::parse_ber(&cms).unwrap();

        for signer in signed_data.signers() {
            signer
                .verify_signature_with_signed_data(&signed_data)
                .unwrap();
        }
    }

    #[test]
    fn cms_self_signed_certificate_signing_ed25519() {
        let (cert, signing_key, _) = create_self_signed_code_signing_certificate(
            KeyAlgorithm::Ed25519,
            "test",
            "US",
            "nobody@example.com",
            chrono::Duration::hours(1),
        )
        .unwrap();

        let plaintext = "hello, world";

        let cms = SignedDataBuilder::default()
            .certificate(cert.clone())
            .unwrap()
            .signed_content(plaintext.as_bytes().to_vec())
            .signer(SignerBuilder::new(&signing_key, cert))
            .build_ber()
            .unwrap();

        let signed_data = SignedData::parse_ber(&cms).unwrap();

        for signer in signed_data.signers() {
            signer
                .verify_signature_with_signed_data(&signed_data)
                .unwrap();
        }
    }
}
