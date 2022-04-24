// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Functionality that only works on macOS.

use {
    crate::{
        certificate::{AppleCertificate, OID_USER_ID},
        error::AppleCodesignError,
    },
    bcder::Oid,
    security_framework::{
        certificate::SecCertificate,
        item::{ItemClass, ItemSearchOptions, Reference, SearchResult},
        os::macos::{
            item::ItemSearchOptionsExt,
            keychain::{SecKeychain, SecPreferencesDomain},
        },
    },
    std::ops::Deref,
    x509_certificate::CapturedX509Certificate,
};

const SYSTEM_ROOTS_KEYCHAIN: &str = "/System/Library/Keychains/SystemRootCertificates.keychain";

/// A wrapper around [SecPreferencesDomain] so we can use crate local types.
#[derive(Clone, Copy, Debug)]
pub enum KeychainDomain {
    User,
    System,
    Common,
    Dynamic,
}

impl From<KeychainDomain> for SecPreferencesDomain {
    fn from(v: KeychainDomain) -> Self {
        match v {
            KeychainDomain::User => Self::User,
            KeychainDomain::System => Self::System,
            KeychainDomain::Common => Self::Common,
            KeychainDomain::Dynamic => Self::Dynamic,
        }
    }
}

impl TryFrom<&str> for KeychainDomain {
    type Error = String;

    fn try_from(v: &str) -> Result<Self, Self::Error> {
        match v {
            "user" => Ok(Self::User),
            "system" => Ok(Self::System),
            "common" => Ok(Self::Common),
            "dynamic" => Ok(Self::Dynamic),
            _ => Err(format!(
                "{} is not a valid keychain domain; use user, system, common, or dynamic",
                v
            )),
        }
    }
}

/// A certificate in a keychain.
pub struct KeychainCertificate {
    sec_cert: SecCertificate,
    captured: CapturedX509Certificate,
}

impl Deref for KeychainCertificate {
    type Target = CapturedX509Certificate;

    fn deref(&self) -> &Self::Target {
        &self.captured
    }
}

fn find_certificates(
    keychains: &[SecKeychain],
) -> Result<Vec<KeychainCertificate>, AppleCodesignError> {
    let mut search = ItemSearchOptions::default();
    search.keychains(keychains);
    search.class(ItemClass::certificate());
    search.limit(i32::MAX as i64);

    let mut certs = vec![];

    for item in search.search()? {
        match item {
            SearchResult::Ref(reference) => match reference {
                Reference::Certificate(cert) => {
                    if let Ok(captured) = CapturedX509Certificate::from_der(cert.to_der()) {
                        certs.push(KeychainCertificate {
                            sec_cert: cert,
                            captured,
                        });
                    }
                }

                _ => {
                    return Err(AppleCodesignError::KeychainError(
                        "non-certificate reference from keychain search (this should not happen)"
                            .to_string(),
                    ));
                }
            },
            _ => {
                return Err(AppleCodesignError::KeychainError(
                    "non-reference result from keychain search (this should not happen)"
                        .to_string(),
                ));
            }
        }
    }

    Ok(certs)
}

/// Locate code signing certificates in the macOS keychain.
pub fn keychain_find_code_signing_certificates(
    domain: KeychainDomain,
    password: Option<&str>,
) -> Result<Vec<KeychainCertificate>, AppleCodesignError> {
    let mut keychain = SecKeychain::default_for_domain(domain.into())?;
    if password.is_some() {
        keychain.unlock(password)?;
    }

    let certs = find_certificates(&[keychain])?;

    Ok(certs
        .into_iter()
        .filter(|cert| !cert.captured.apple_code_signing_extensions().is_empty())
        .collect::<Vec<_>>())
}

/// Find the x509 certificate chain for a certificate given search parameters.
///
/// `domain` and `password` specify which keychain to operate on and whether
/// to attempt to unlock it via a password.
///
/// `user_id` specifies the UID value in the certificate subject to search for.
/// You can find this in `Keychain Access` by clicking on the certificate in
/// question and looking for `User ID` under the `Subject Name` section.
pub fn macos_keychain_find_certificate_chain(
    domain: KeychainDomain,
    password: Option<&str>,
    user_id: &str,
) -> Result<Vec<CapturedX509Certificate>, AppleCodesignError> {
    let mut keychain = SecKeychain::default_for_domain(domain.into())?;
    if password.is_some() {
        keychain.unlock(password)?;
    }

    // Find all certificates for the given keychain plus the system roots, which
    // has the root CAs.
    let keychains = vec![SecKeychain::open(SYSTEM_ROOTS_KEYCHAIN)?, keychain];

    let certs = find_certificates(&keychains)?;

    // Now search for the requested start certificate and pull the thread until
    // we get to a self-signed certificate.
    let start_cert: &CapturedX509Certificate = certs
        .iter()
        .find_map(|cert| {
            if let Ok(Some(value)) = cert
                .captured
                .subject_name()
                .find_first_attribute_string(Oid(OID_USER_ID.as_ref().into()))
            {
                if value == user_id {
                    Some(&cert.captured)
                } else {
                    None
                }
            } else {
                None
            }
        })
        .ok_or_else(|| AppleCodesignError::CertificateNotFound(format!("UID={}", user_id)))?;

    let mut chain = vec![start_cert.clone()];
    let mut last_issuer_name = start_cert.issuer_name();

    loop {
        let issuer = certs.iter().find_map(|cert| {
            if cert.captured.subject_name() == last_issuer_name {
                Some(&cert.captured)
            } else {
                None
            }
        });

        if let Some(issuer) = issuer {
            chain.push(issuer.clone());

            // Self signed. Stop the chain so we don't infinite loop.
            if issuer.subject_name() == issuer.issuer_name() {
                break;
            } else {
                last_issuer_name = issuer.issuer_name();
            }
        } else {
            // Couldn't find issuer. Stop the search.
            break;
        }
    }

    Ok(chain)
}
