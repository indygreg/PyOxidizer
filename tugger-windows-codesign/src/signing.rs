// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Functionality for signing binaries on Windows. */

use {
    anyhow::{anyhow, Result},
    chrono::SubsecRound,
    std::{
        io::Read,
        path::{Path, PathBuf},
    },
};

/// Represents an x509 signing certificate backed by a file.
#[derive(Clone, Debug)]
pub struct FileBasedX509SigningCertificate {
    /// Path to the certificate file.
    path: PathBuf,
    /// Password used to unlock the certificate.
    password: Option<String>,
}

impl FileBasedX509SigningCertificate {
    /// Construct an instance from a path.
    ///
    /// No validation is done that the path exists.
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
            password: None,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn password(&self) -> &Option<String> {
        &self.password
    }

    pub fn set_password(&mut self, password: impl ToString) {
        self.password = Some(password.to_string());
    }
}

/// Represents an x509 certificate used to sign binaries on Windows.
#[derive(Clone, Debug)]
pub enum X509SigningCertificate {
    /// Select the best available signing certificate.
    Auto,

    /// An x509 certificate backed by a filesystem file.
    File(FileBasedX509SigningCertificate),

    /// An x509 certificate specified by its subject name or substring thereof.
    SubjectName(String),
}

impl From<FileBasedX509SigningCertificate> for X509SigningCertificate {
    fn from(v: FileBasedX509SigningCertificate) -> Self {
        Self::File(v)
    }
}

/// Create parameters for a self-signed x509 certificate suitable for code signing on Windows.
///
/// The self-signed certificate mimics what the powershell
/// `New-SelfSignedCertificate -DnsName <subject_name> -Type CodeSigning -KeyAlgorithm ECDSA_nistP256`
/// would do.
pub fn create_self_signed_code_signing_certificate_params(
    subject_name: &str,
) -> rcgen::CertificateParams {
    let mut params = rcgen::CertificateParams::new(vec![]);
    params.alg = &rcgen::PKCS_ECDSA_P256_SHA256;
    params.key_identifier_method = rcgen::KeyIdMethod::Sha256;
    params.distinguished_name = rcgen::DistinguishedName::new();
    params
        .subject_alt_names
        .push(rcgen::SanType::DnsName(subject_name.to_string()));
    params
        .distinguished_name
        .push(rcgen::DnType::CommonName, subject_name);
    params
        .extended_key_usages
        .push(rcgen::ExtendedKeyUsagePurpose::CodeSigning);
    params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
    // The default is thousands of years in the future. Let's use something more reasonable.
    params.not_after = chrono::Utc::now()
        .checked_add_signed(chrono::Duration::days(365))
        .unwrap()
        .trunc_subsecs(0);

    // KeyUsage(KeyUsage { flags: 1 })
    let mut key_usage =
        rcgen::CustomExtension::from_oid_content(&[2, 5, 29, 15], vec![3, 2, 7, 128]);
    key_usage.set_criticality(true);
    params.custom_extensions.push(key_usage);

    params
}

pub fn create_self_signed_code_signing_certificate(
    subject_name: &str,
) -> std::result::Result<rcgen::Certificate, rcgen::RcgenError> {
    let params = create_self_signed_code_signing_certificate_params(subject_name);

    rcgen::Certificate::from_params(params)
}

/// Serialize a certificate to a PKCS #12 `.pfx` file.
///
/// This file format is what is used by `signtool` and other Microsoft tools.
pub fn certificate_to_pfx(
    cert: &rcgen::Certificate,
    password: &str,
    name: &str,
) -> Result<Vec<u8>> {
    let cert_der = cert.serialize_der()?;
    let key_der = cert.serialize_private_key_der();

    let pfx = p12::PFX::new(&cert_der, &key_der, None, password, name)
        .ok_or_else(|| anyhow!("unable to convert to pfx"))?;

    let buffer = yasna::construct_der(|writer| {
        pfx.write(writer);
    });

    Ok(buffer)
}

/// MSI file magic.
const CFB_MAGIC_NUMBER: [u8; 8] = [0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1];

/// Whether the bytes passed in look like a file header for a format that is signable.
///
/// The passed buffer must be at least 16 bytes long.
///
/// This could yield false positives.
#[allow(clippy::if_same_then_else)]
pub fn is_signable_binary_header(data: &[u8]) -> bool {
    if data.len() < 16 {
        false
    // DOS header.
    } else if data[0] == 0x4d && data[1] == 0x5a {
        true
    } else {
        data[0..CFB_MAGIC_NUMBER.len()] == CFB_MAGIC_NUMBER
    }
}

/// Determine whether a given filesystem path is signable.
///
/// This effectively answers whether the given path is a PE or MSI.
pub fn is_file_signable(path: impl AsRef<Path>) -> Result<bool> {
    let path = path.as_ref();

    if path.metadata()?.len() < 16 {
        return Ok(false);
    }

    let mut fh = std::fs::File::open(&path)?;
    let mut buffer: [u8; 16] = [0; 16];
    fh.read_exact(&mut buffer)?;

    Ok(is_signable_binary_header(&buffer))
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        anyhow::Result,
        der_parser::oid,
        std::{collections::BTreeMap, iter::FromIterator},
    };

    // PEM encoded key pair generated via Powershell.
    const POWERSHELL_CERTIFICATE_PUBLIC_PEM: &str = "-----BEGIN CERTIFICATE-----\n\
        MIIBnzCCAUagAwIBAgIQSE/jLE4ZZYtHZ1e/Uh5IKTAKBggqhkjOPQQDAjAeMRww\n\
        GgYDVQQDDBN0ZXN0aW5nQGV4YW1wbGUuY29tMB4XDTIwMTEyNjIxMjIyOFoXDTIx\n\
        MTEyNjIxNDIyOFowHjEcMBoGA1UEAwwTdGVzdGluZ0BleGFtcGxlLmNvbTBZMBMG\n\
        ByqGSM49AgEGCCqGSM49AwEHA0IABG50cCwrBbSYIHjakucfkFQwBxyELaqq36a5\n\
        l33+zC5ugnh/zDNp/txhOEHoWb7KxgeeLsDU5fnE5o7LWMweHF6jZjBkMA4GA1Ud\n\
        DwEB/wQEAwIHgDATBgNVHSUEDDAKBggrBgEFBQcDAzAeBgNVHREEFzAVghN0ZXN0\n\
        aW5nQGV4YW1wbGUuY29tMB0GA1UdDgQWBBQTIsJVQaqqlRroqvxjrQxdaPWF2zAK\n\
        BggqhkjOPQQDAgNHADBEAiBW6XrjErz6HAyJk/lhyhAfpYiQBKc+74dBaBFRccbd\n\
        HgIgWCs4HPGhR1KmUEvjOLZLxsph/SZ1omQt8QQQYsUn1m4=\n\
        -----END CERTIFICATE-----\n";

    const POWERSHELL_CERTIFICATE_PRIVATE_PEM: &str = "-----BEGIN PRIVATE KEY-----\n\
        MIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQg9mPzM4rZBqtjLuWZ\n\
        rWiPM5PgwTcsYMm6ojX9OAz1AIehRANCAARudHAsKwW0mCB42pLnH5BUMAcchC2q\n\
        qt+muZd9/swuboJ4f8wzaf7cYThB6Fm+ysYHni7A1OX5xOaOy1jMHhxe\n\
        -----END PRIVATE KEY-----\n";

    #[test]
    fn test_create_self_signed_certificate() -> Result<()> {
        let powershell_pem = x509_parser::pem::Pem::read(std::io::Cursor::new(
            POWERSHELL_CERTIFICATE_PUBLIC_PEM.as_bytes(),
        ))?
        .0;
        let powershell = powershell_pem.parse_x509()?;

        // Just in case we need to use this in the future.
        rcgen::KeyPair::from_pem(POWERSHELL_CERTIFICATE_PRIVATE_PEM)?;

        let cert = create_self_signed_code_signing_certificate("testing@example.com")?;
        let cert_der = cert.serialize_der()?;

        let generated = x509_parser::parse_x509_certificate(&cert_der)?.1;

        assert_eq!(generated.subject(), powershell.subject(), "subject matches");
        assert_eq!(
            generated.signature_algorithm, powershell.signature_algorithm,
            "signature algorithm matches"
        );

        let subject_key_identifier_oid = oid!(2.5.29 .14);
        let basic_constraints_oid = oid!(2.5.29 .19);
        let subject_alternative_name_oid = oid!(2.5.29 .17);
        let extended_usage_oid = oid!(2.5.29 .37);
        let key_usage_oid = oid!(2.5.29 .15);

        assert!(generated
            .extensions()
            .contains_key(&subject_key_identifier_oid));
        assert_ne!(
            generated.extensions().get(&subject_key_identifier_oid),
            powershell.extensions().get(&subject_key_identifier_oid),
            "subject key identifier extension differs"
        );
        assert!(generated.extensions().contains_key(&basic_constraints_oid));
        assert!(!powershell.extensions().contains_key(&basic_constraints_oid));

        assert!(generated
            .extensions()
            .contains_key(&subject_alternative_name_oid));
        assert_eq!(
            generated.extensions().get(&subject_alternative_name_oid),
            powershell.extensions().get(&subject_alternative_name_oid),
            "subject alternative name extension identical"
        );

        assert!(generated.extensions().contains_key(&extended_usage_oid));
        assert_eq!(
            generated.extensions().get(&extended_usage_oid),
            powershell.extensions().get(&extended_usage_oid),
            "extended key usage extension identical"
        );

        assert!(generated.extensions().contains_key(&key_usage_oid));
        assert_eq!(
            generated.extensions().get(&key_usage_oid),
            powershell.extensions().get(&key_usage_oid),
            "key usage extension identical"
        );

        // Subject Key Identifier differs due to different key pairs in use.
        // Ours also emits a basic constraints extension.
        let generated_filtered =
            BTreeMap::from_iter(generated.extensions().iter().filter_map(|(k, ext)| {
                if k != &subject_key_identifier_oid && k != &basic_constraints_oid {
                    Some((k.to_id_string(), ext.clone()))
                } else {
                    None
                }
            }));
        let powershell_filtered =
            BTreeMap::from_iter(powershell.extensions().iter().filter_map(|(k, ext)| {
                if k != &subject_key_identifier_oid {
                    Some((k.to_id_string(), ext.clone()))
                } else {
                    None
                }
            }));

        assert_eq!(generated_filtered, powershell_filtered, "extensions match");

        Ok(())
    }

    #[test]
    fn test_serialize_pfx() -> Result<()> {
        let cert = create_self_signed_code_signing_certificate("someone@example.com")?;
        certificate_to_pfx(&cert, "password", "name")?;

        Ok(())
    }

    #[test]
    fn test_is_signable() -> Result<()> {
        let exe = std::env::current_exe()?;

        let is_signable = is_file_signable(&exe)?;

        if cfg!(target_family = "windows") {
            assert!(is_signable);
        } else {
            assert!(!is_signable);
        }

        Ok(())
    }
}
