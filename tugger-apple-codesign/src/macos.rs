// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Functionality that only works on macOS.

use {
    crate::error::AppleCodesignError,
    bcder::{ConstOid, Oid},
    bytes::Bytes,
    cryptographic_message_syntax::Certificate,
    security_framework::{
        item::{ItemClass, ItemSearchOptions, Reference, SearchResult},
        os::macos::{
            item::ItemSearchOptionsExt,
            keychain::{SecKeychain, SecPreferencesDomain},
        },
    },
};

/// UserID.
///
/// 0.9.2342.19200300.100.1.1
pub const OID_UID: ConstOid = Oid(&[9, 146, 38, 137, 147, 242, 44, 100, 1, 1]);

const SYSTEM_ROOTS_KEYCHAIN: &str = "/System/Library/Keychains/SystemRootCertificates.keychain";

fn find_certificates(keychains: &[SecKeychain]) -> Result<Vec<Certificate>, AppleCodesignError> {
    let mut search = ItemSearchOptions::default();
    search.keychains(keychains);
    search.class(ItemClass::certificate());
    search.limit(i32::MAX as _);

    let mut certs = vec![];

    for item in search.search()? {
        match item {
            SearchResult::Ref(reference) => match reference {
                Reference::Certificate(cert) => {
                    if let Ok(c) = Certificate::from_der(&cert.to_der()) {
                        certs.push(c);
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

/// Find the x509 certificate chain for a certificate given search parameters.
///
/// `domain` and `password` specify which keychain to operate on and whether
/// to attempt to unlock it via a password.
///
/// `user_id` specifies the UID value in the certificate subject to search for.
/// You can find this in `Keychain Access` by clicking on the certificate in
/// question and looking for `User ID` under the `Subject Name` section.
pub fn macos_keychain_find_certificate_chain(
    domain: SecPreferencesDomain,
    password: Option<&str>,
    user_id: &str,
) -> Result<Vec<Certificate>, AppleCodesignError> {
    let mut keychain = SecKeychain::default_for_domain(domain)?;
    if password.is_some() {
        keychain.unlock(password)?;
    }

    // Find all certificates for the given keychain plus the system roots, which
    // has the root CAs.
    let keychains = vec![SecKeychain::open(SYSTEM_ROOTS_KEYCHAIN)?, keychain];

    let certs = find_certificates(&keychains)?;

    // Now search for the requested start certificate and pull the thread until
    // we get to a self-signed certificate.
    let start_cert: &Certificate = certs
        .iter()
        .find(|cert| {
            if let Ok(subject) = cert.subject_dn() {
                if let Ok(Some(value)) =
                    subject.find_attribute_string(Oid(Bytes::from(OID_UID.as_ref())))
                {
                    value == user_id
                } else {
                    false
                }
            } else {
                false
            }
        })
        .ok_or_else(|| AppleCodesignError::CertificateNotFound(format!("UID={}", user_id)))?;

    let mut chain = vec![];
    chain.push(start_cert.clone());

    let mut last_issuer_name = start_cert.issuer();

    loop {
        let issuer = certs.iter().find(|cert| cert.subject() == last_issuer_name);

        if let Some(issuer) = issuer {
            chain.push(issuer.clone());

            // Self signed. Stop the chain so we don't infinite loop.
            if issuer.subject() == issuer.issuer() {
                break;
            } else {
                last_issuer_name = issuer.issuer();
            }
        } else {
            // Couldn't find issuer. Stop the search.
            break;
        }
    }

    Ok(chain)
}
