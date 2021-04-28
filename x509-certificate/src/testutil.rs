// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    crate::{asn1time::Time, rfc3280::Name, rfc5280, rfc5958::OneAsymmetricKey, *},
    bcder::encode::Values,
    bytes::Bytes,
    ring::signature::KeyPair,
    std::convert::TryInto,
};

const RSA_PRIVATE_KEY: &str = "-----BEGIN PRIVATE KEY-----\n\
        MIIEvwIBADANBgkqhkiG9w0BAQEFAASCBKkwggSlAgEAAoIBAQC2rF88ecfP3lsn\n\
        i21jnGm7IqMG4RyG5nuXlyqmjZdvOW5tjonRyjxFJucp8GyppKwssEVuG4ohmDYi\n\
        pNdHcMjVx1rMplE6FZTvRC7RuFgmFY0PLddDFtFqUi2Z1RCkW/+Q8ebRRlhr4Pj/\n\
        qGsKDzHIgcmADOXzIqzlO+lA9xodxCfT6ay0cjG1WL1+Agf7ngy7OvVr/CDf4pbv\n\
        ooHZ9e+SZmTs1/gXVQDvEZcCk7hH12HBb7I/NHDucOEE7kJklXVGuwb5+Mhw/gKo\n\
        LEcZ644K6Jac8AH9NVM6MdNMxyZt6pR0q08oqeozP+YoIhDrtlRLkRMzw3VS2/v1\n\
        0xh+7SDzAgMBAAECggEBAI8IKs3cgPKnJXKyPmW3jCYl+caiLscF4xIQIConRcKm\n\
        EmwgJpOoqUZwLqJtCXhPYyzenI6Za6/gUcsQjSv4CJkzLkp9k65KRcKO/aXilMrF\n\
        Jx0ShLGYRULds6z24r/+9P4WGugUD5nwnqb3xVAsE4vu68qizs5wgTZAkeP3V3Cj\n\
        2usyWKuLjbXoeR/wuRluq2Q07QXHTjrVziw2JwISn5w6ynHw4ogGDxmIMoAcThiq\n\
        rTNufGA3pmBxq0Sk8umXVRjUBeoKKo/qGpfoxSDzrTxn3wt5gVRpit+oKnxTy2B7\n\
        vwC4+ASo9HEeQX0L6HJBTIxUSsgzeWnf25T+fquhyAkCgYEA2sWEsktyRQMHygjZ\n\
        S6Lb/V4ZsbJwfix6hm7//wbMFDzgtDKSRMp+C265kRf/hdYnyGQDTtan6w9GFsvO\n\
        V12CugxdC07gt2mmikWf9um716X9u5nrEgJvNotwmW1mk28rP55nr/SsKniNkx6y\n\
        JgLjGzVa2Yf9jP0A3+ASYKqFisUCgYEA1cJIuOhnBZGBBdqxG/YPljYmoaAXSrUu\n\
        raZA8a9KeZ/QODWsZwCCGA+OQZIfoLn9WueZf3oRxpIqNSqXW2XE7Xv78Ih01xLN\n\
        d7nzMSTz3GiNv1UNYmm4ZsKf/XDapYCM23oqiNcVw7XBEr1hit1IRB5slm4gESWf\n\
        dNdjMybumFcCgYEA0SeFdfArj08WY1GSbX2GVPViG0E9y2M6wMveczNMaQzKx3yR\n\
        2rK9TrDNOKp44LudzTfQ8c7HOzOfDqxK2bvM/5JSYj1HGhMn5YorJSTRMZrAulqt\n\
        IsqxCLTHMegl6U6fSnNnLhH9h505vS3bo/uepKSd9trMzb4U1/ShnUlp4wECgYEA\n\
        lwwQo0jl85Nb3q0oVZ/MZ9Kf/bnIe6wH7gD7B01cjREW64FR7/717tafKUp+Ou7y\n\
        Tpg1aVTy1qRWWvdbuOPzAfWIk/F4zrmkoyOs6183Sto+v6L0MESQX1zL/SUP+78Y\n\
        ycZL5CJIaOE4K2vTT3MKK8hr5uiulC9HvCKvIGg0VUUCgYBNrn4+tINn6iN0c45/\n\
        0qmmNuM/lLmI5UMgGsbpR0E7zHueiNjZSkPkra8uvV7km8YWoxaCyNpQMi2r/aRp\n\
        VzRAm2HqWPLEtc+BzoVT9ySc8RuOibUH6hJ7b8/secpFQwJUBhxjnxuyKXnIdxsK\n\
        wCqqgSEHwBtdDKP/nox4H+CcMw==\n\
        -----END PRIVATE KEY-----";

const X509_CERTIFICATE: &str = "-----BEGIN CERTIFICATE-----\n\
        MIIDkzCCAnugAwIBAgIUDNhjvv6ol8EZG5YhNniO4pAiUQEwDQYJKoZIhvcNAQEL\n\
        BQAwWTELMAkGA1UEBhMCVVMxEzARBgNVBAgMCkNhbGlmb3JuaWExEDAOBgNVBAoM\n\
        B3Rlc3RpbmcxDTALBgNVBAsMBHVuaXQxFDASBgNVBAMMC1VuaXQgVGVzdGVyMB4X\n\
        DTIxMDMxNjE2MDkyOFoXDTI2MDkwNjE2MDkyOFowWTELMAkGA1UEBhMCVVMxEzAR\n\
        BgNVBAgMCkNhbGlmb3JuaWExEDAOBgNVBAoMB3Rlc3RpbmcxDTALBgNVBAsMBHVu\n\
        aXQxFDASBgNVBAMMC1VuaXQgVGVzdGVyMIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8A\n\
        MIIBCgKCAQEAtqxfPHnHz95bJ4ttY5xpuyKjBuEchuZ7l5cqpo2XbzlubY6J0co8\n\
        RSbnKfBsqaSsLLBFbhuKIZg2IqTXR3DI1cdazKZROhWU70Qu0bhYJhWNDy3XQxbR\n\
        alItmdUQpFv/kPHm0UZYa+D4/6hrCg8xyIHJgAzl8yKs5TvpQPcaHcQn0+mstHIx\n\
        tVi9fgIH+54Muzr1a/wg3+KW76KB2fXvkmZk7Nf4F1UA7xGXApO4R9dhwW+yPzRw\n\
        7nDhBO5CZJV1RrsG+fjIcP4CqCxHGeuOCuiWnPAB/TVTOjHTTMcmbeqUdKtPKKnq\n\
        Mz/mKCIQ67ZUS5ETM8N1Utv79dMYfu0g8wIDAQABo1MwUTAdBgNVHQ4EFgQUkiWC\n\
        PwIRoykbi6mtOjWNR0X1eFEwHwYDVR0jBBgwFoAUkiWCPwIRoykbi6mtOjWNR0X1\n\
        eFEwDwYDVR0TAQH/BAUwAwEB/zANBgkqhkiG9w0BAQsFAAOCAQEAAN4plkAcXZIx\n\
        4KqM5AueYqYtR1y8HAaVz+5BKAWyiQJxhktAJJr7o8Yafde7SrUMfEVGDvPa2xuG\n\
        xhx5d2L3G/FDUhHbsmM3Yp3XTGkS5VwH2nHi6x4HBEpLJZfTbbTDQgS1AdtrQg0V\n\
        VY4ph7n/F0sjJL9pmpTdRx1Z2OrwYpJfWOEIA3NDflYvby9Ubb29uVRsFWrgBijl\n\
        3NIzXHvoJ2Fd+Crkc43+wWZ55hcbwSgkC1/T1mFNzd4klwncH4Rqw2KDkEFdWKmM\n\
        CiRnpyZ52+8FW64s952/SGtMs4P3fFNnWpL3njNDnfxa+r+aWDtz12PJc5FyzlkC\n\
        P4ysBX3CuA==\n\
        -----END CERTIFICATE-----";

pub fn rsa_private_key() -> InMemorySigningKeyPair {
    let key_der = pem::parse(RSA_PRIVATE_KEY.as_bytes()).unwrap();

    InMemorySigningKeyPair::from(
        ring::signature::RsaKeyPair::from_pkcs8(&key_der.contents).unwrap(),
    )
}

pub fn rsa_cert() -> CapturedX509Certificate {
    CapturedX509Certificate::from_pem(X509_CERTIFICATE.as_bytes()).unwrap()
}

pub fn self_signed_ecdsa_key_pair() -> (CapturedX509Certificate, InMemorySigningKeyPair) {
    let document = ring::signature::EcdsaKeyPair::generate_pkcs8(
        &ring::signature::ECDSA_P256_SHA256_ASN1_SIGNING,
        &ring::rand::SystemRandom::new(),
    )
    .unwrap();

    let key_pair_asn1 =
        bcder::decode::Constructed::decode(document.as_ref(), bcder::Mode::Der, |cons| {
            OneAsymmetricKey::take_from(cons)
        })
        .unwrap();
    let key_pair = ring::signature::EcdsaKeyPair::from_pkcs8(
        &ring::signature::ECDSA_P256_SHA256_ASN1_SIGNING,
        document.as_ref(),
    )
    .unwrap();

    let signing_key = InMemorySigningKeyPair::from_pkcs8_der(document.as_ref(), None).unwrap();

    let mut name = Name::default();
    name.append_common_name_utf8_string("test").unwrap();
    name.append_country_utf8_string("US").unwrap();

    let now = chrono::Utc::now();
    let expires = now + chrono::Duration::hours(1);

    let signature_algorithm = SignatureAlgorithm::EcdsaSha256;

    let tbs_certificate = rfc5280::TbsCertificate {
        version: rfc5280::Version::V3,
        serial_number: 42.into(),
        signature: signature_algorithm.into(),
        issuer: name.clone(),
        validity: rfc5280::Validity {
            not_before: Time::from(now),
            not_after: Time::from(expires),
        },
        subject: name,
        subject_public_key_info: rfc5280::SubjectPublicKeyInfo {
            algorithm: rfc5280::AlgorithmIdentifier {
                algorithm: key_pair_asn1.private_key_algorithm.algorithm.clone(),
                parameters: key_pair_asn1.private_key_algorithm.parameters,
            },
            subject_public_key: bcder::BitString::new(
                0,
                Bytes::copy_from_slice(key_pair.public_key().as_ref()),
            ),
        },
        issuer_unique_id: None,
        subject_unique_id: None,
        extensions: None,
        raw_data: None,
    };

    let mut cert_ber = Vec::<u8>::new();
    tbs_certificate
        .encode_ref()
        .write_encoded(bcder::Mode::Ber, &mut cert_ber)
        .unwrap();

    let signature = signing_key.sign(&cert_ber).unwrap();

    let cert = rfc5280::Certificate {
        tbs_certificate,
        signature_algorithm: signature_algorithm.into(),
        signature: bcder::BitString::new(0, Bytes::copy_from_slice(&signature)),
    };

    let cert = X509Certificate::from(cert).try_into().unwrap();

    (cert, signing_key)
}

pub fn self_signed_ed25519_key_pair() -> (CapturedX509Certificate, InMemorySigningKeyPair) {
    let document =
        ring::signature::Ed25519KeyPair::generate_pkcs8(&ring::rand::SystemRandom::new()).unwrap();

    let key_pair_asn1 =
        bcder::decode::Constructed::decode(document.as_ref(), bcder::Mode::Der, |cons| {
            OneAsymmetricKey::take_from(cons)
        })
        .unwrap();
    let key_pair = ring::signature::Ed25519KeyPair::from_pkcs8(document.as_ref()).unwrap();

    let signing_key = InMemorySigningKeyPair::from_pkcs8_der(document.as_ref(), None).unwrap();

    let mut name = Name::default();
    name.append_common_name_utf8_string("test").unwrap();
    name.append_country_utf8_string("US").unwrap();

    let now = chrono::Utc::now();
    let expires = now + chrono::Duration::hours(1);

    let signature_algorithm = SignatureAlgorithm::Ed25519;

    let tbs_certificate = rfc5280::TbsCertificate {
        version: rfc5280::Version::V3,
        serial_number: 42.into(),
        signature: signature_algorithm.into(),
        issuer: name.clone(),
        validity: rfc5280::Validity {
            not_before: Time::from(now),
            not_after: Time::from(expires),
        },
        subject: name,
        subject_public_key_info: rfc5280::SubjectPublicKeyInfo {
            algorithm: rfc5280::AlgorithmIdentifier {
                algorithm: key_pair_asn1.private_key_algorithm.algorithm.clone(),
                parameters: key_pair_asn1.private_key_algorithm.parameters,
            },
            subject_public_key: bcder::BitString::new(
                0,
                Bytes::copy_from_slice(key_pair.public_key().as_ref()),
            ),
        },
        issuer_unique_id: None,
        subject_unique_id: None,
        extensions: None,
        raw_data: None,
    };

    let mut cert_ber = Vec::<u8>::new();
    tbs_certificate
        .encode_ref()
        .write_encoded(bcder::Mode::Ber, &mut cert_ber)
        .unwrap();

    let signature = signing_key.sign(&cert_ber).unwrap();

    let cert = rfc5280::Certificate {
        tbs_certificate,
        signature_algorithm: signature_algorithm.into(),
        signature: bcder::BitString::new(0, Bytes::copy_from_slice(&signature)),
    };

    let cert = X509Certificate::from(cert).try_into().unwrap();

    (cert, signing_key)
}
