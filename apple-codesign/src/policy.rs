// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Apple trust policies.
//!
//! Apple operating systems have a number of pre-canned trust policies
//! that must be fulfilled in order to trust signed code. These are
//! often based off the presence of specific X.509 certificates in the
//! issuing chain and/or the presence of attributes in X.509 certificates.
//!
//! Trust policies are often engraved in code signatures as part of the
//! signed code requirements expression.
//!
//! This module defines a bunch of metadata for describing Apple trust
//! entities and also provides pre-canned policies that can be easily
//! constructed to match those employed by Apple's official signing tools.
//!
//! Apple's certificates can be found at
//! <https://www.apple.com/certificateauthority/>.

use {
    crate::{
        certificate::{
            AppleCertificate, CertificateAuthorityExtension, CertificateProfile,
            CodeSigningCertificateExtension,
        },
        code_requirement::{CodeRequirementExpression, CodeRequirementMatchExpression},
        error::AppleCodesignError,
    },
    once_cell::sync::Lazy,
    std::ops::Deref,
    x509_certificate::CapturedX509Certificate,
};

/// Code signing requirement for Mac Developer ID.
///
/// `anchor apple generic and certificate 1[field.1.2.840.113635.100.6.2.6] exists and
/// (certificate leaf[field.1.2.840.113635.100.6.1.14] or certificate leaf[field.1.2.840.113635.100.6.1.13])`
static POLICY_MAC_DEVELOPER_ID: Lazy<CodeRequirementExpression<'static>> = Lazy::new(|| {
    CodeRequirementExpression::And(
        Box::new(CodeRequirementExpression::And(
            Box::new(CodeRequirementExpression::AnchorAppleGeneric),
            Box::new(CodeRequirementExpression::CertificateGeneric(
                1,
                CertificateAuthorityExtension::DeveloperId.as_oid(),
                CodeRequirementMatchExpression::Exists,
            )),
        )),
        Box::new(CodeRequirementExpression::Or(
            Box::new(CodeRequirementExpression::CertificateGeneric(
                0,
                CodeSigningCertificateExtension::DeveloperIdInstaller.as_oid(),
                CodeRequirementMatchExpression::Exists,
            )),
            Box::new(CodeRequirementExpression::CertificateGeneric(
                0,
                CodeSigningCertificateExtension::DeveloperIdApplication.as_oid(),
                CodeRequirementMatchExpression::Exists,
            )),
        )),
    )
});

/// Notarized executable.
///
/// `anchor apple generic and certificate 1[field.1.2.840.113635.100.6.2.6] exists and
/// certificate leaf[field.1.2.840.113635.100.6.1.13] exists and notarized'`
///
static POLICY_NOTARIZED_EXECUTABLE: Lazy<CodeRequirementExpression<'static>> = Lazy::new(|| {
    CodeRequirementExpression::And(
        Box::new(CodeRequirementExpression::And(
            Box::new(CodeRequirementExpression::And(
                Box::new(CodeRequirementExpression::AnchorAppleGeneric),
                Box::new(CodeRequirementExpression::CertificateGeneric(
                    1,
                    CertificateAuthorityExtension::DeveloperId.as_oid(),
                    CodeRequirementMatchExpression::Exists,
                )),
            )),
            Box::new(CodeRequirementExpression::CertificateGeneric(
                0,
                CodeSigningCertificateExtension::DeveloperIdApplication.as_oid(),
                CodeRequirementMatchExpression::Exists,
            )),
        )),
        Box::new(CodeRequirementExpression::Notarized),
    )
});

/// Notarized installer.
///
/// `'anchor apple generic and certificate 1[field.1.2.840.113635.100.6.2.6] exists
/// and (certificate leaf[field.1.2.840.113635.100.6.1.14] or certificate
/// leaf[field.1.2.840.113635.100.6.1.13]) and notarized'`
static POLICY_NOTARIZED_INSTALLER: Lazy<CodeRequirementExpression<'static>> = Lazy::new(|| {
    CodeRequirementExpression::And(
        Box::new(CodeRequirementExpression::And(
            Box::new(CodeRequirementExpression::And(
                Box::new(CodeRequirementExpression::AnchorAppleGeneric),
                Box::new(CodeRequirementExpression::CertificateGeneric(
                    1,
                    CertificateAuthorityExtension::DeveloperId.as_oid(),
                    CodeRequirementMatchExpression::Exists,
                )),
            )),
            Box::new(CodeRequirementExpression::Or(
                Box::new(CodeRequirementExpression::CertificateGeneric(
                    0,
                    CodeSigningCertificateExtension::DeveloperIdInstaller.as_oid(),
                    CodeRequirementMatchExpression::Exists,
                )),
                Box::new(CodeRequirementExpression::CertificateGeneric(
                    0,
                    CodeSigningCertificateExtension::DeveloperIdApplication.as_oid(),
                    CodeRequirementMatchExpression::Exists,
                )),
            )),
        )),
        Box::new(CodeRequirementExpression::Notarized),
    )
});

/// Defines well-known execution policies for signed code.
///
/// Instances can be obtained from a human-readable string for convenience. Those
/// strings are:
///
/// * `developer-id-signed`
/// * `developer-id-notarized-executable`
/// * `developer-id-notarized-installer`
pub enum ExecutionPolicy {
    /// Code is signed by a certificate authorized for signing Mac applications or
    /// installers and that certificate was issued by
    /// [crate::apple_certificates::KnownCertificate::DeveloperId].
    ///
    /// This is the policy that applies when you get a `Developer ID Application` or
    /// `Developer ID Installer` certificate from Apple.
    DeveloperIdSigned,

    /// Like [Self::DeveloperIdSigned] but only applies to executables (not installers)
    /// and the executable must be notarized.
    ///
    /// If you notarize an individual executable, you effectively convert the
    /// [Self::DeveloperIdSigned] policy into this variant.
    DeveloperIdNotarizedExecutable,

    /// Like [Self::DeveloperIdSigned] but only applies to installers (not executables)
    /// and the installer must be notarized.
    ///
    /// If you notarize an individual installer, you effectively convert the
    /// [Self::DeveloperIdSigned] policy into this variant.
    DeveloperIdNotarizedInstaller,
}

impl Deref for ExecutionPolicy {
    type Target = CodeRequirementExpression<'static>;

    fn deref(&self) -> &Self::Target {
        match self {
            Self::DeveloperIdSigned => POLICY_MAC_DEVELOPER_ID.deref(),
            Self::DeveloperIdNotarizedExecutable => POLICY_NOTARIZED_EXECUTABLE.deref(),
            Self::DeveloperIdNotarizedInstaller => POLICY_NOTARIZED_INSTALLER.deref(),
        }
    }
}

impl TryFrom<&str> for ExecutionPolicy {
    type Error = AppleCodesignError;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        match s {
            "developer-id-signed" => Ok(Self::DeveloperIdSigned),
            "developer-id-notarized-executable" => Ok(Self::DeveloperIdNotarizedExecutable),
            "developer-id-notarized-installer" => Ok(Self::DeveloperIdNotarizedInstaller),
            _ => Err(AppleCodesignError::UnknownPolicy(s.to_string())),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn get_policies() {
        ExecutionPolicy::DeveloperIdSigned.to_bytes().unwrap();
        ExecutionPolicy::DeveloperIdNotarizedExecutable
            .to_bytes()
            .unwrap();
        ExecutionPolicy::DeveloperIdNotarizedInstaller
            .to_bytes()
            .unwrap();
    }
}

/// Derive a designated requirements expression given a code signing certificate.
///
/// This function figures out what the run-time requirements of a signed binary
/// should be given its code signing certificate.
///
/// We determine the flavor of Apple code signing certificate in use and apply an
/// appropriate requirements policy. We strive for behavior equivalence with
/// Apple's `codesign` tool.
pub fn derive_designated_requirements(
    cert: &CapturedX509Certificate,
    identifier: Option<String>,
) -> Result<Option<CodeRequirementExpression<'static>>, AppleCodesignError> {
    let profile = if let Some(profile) = cert.apple_guess_profile() {
        profile
    } else {
        return Ok(None);
    };

    match profile {
        // These appear to be the same policy.
        CertificateProfile::AppleDevelopment | CertificateProfile::AppleDistribution => {
            let cn = cert.subject_common_name().ok_or_else(|| {
                AppleCodesignError::PolicyFormulationError(format!(
                    "(deriving for {:?}) certificate common name not available",
                    profile
                ))
            })?;

            let expr = CodeRequirementExpression::And(
                // It chains to Apple root CA.
                Box::new(CodeRequirementExpression::AnchorAppleGeneric),
                Box::new(CodeRequirementExpression::And(
                    // It was signed by this cert.
                    Box::new(CodeRequirementExpression::CertificateField(
                        0,
                        "subject.CN".to_string().into(),
                        CodeRequirementMatchExpression::Equal(cn.into()),
                    )),
                    // That cert was signed by a CA with WWDR extension.
                    Box::new(CodeRequirementExpression::CertificateGeneric(
                        1,
                        CertificateAuthorityExtension::AppleWorldwideDeveloperRelations.as_oid(),
                        CodeRequirementMatchExpression::Exists,
                    )),
                )),
            );

            Ok(Some(if let Some(identifier) = identifier {
                CodeRequirementExpression::And(
                    Box::new(CodeRequirementExpression::Identifier(identifier.into())),
                    Box::new(expr),
                )
            } else {
                expr
            }))
        }
        CertificateProfile::DeveloperIdApplication => {
            let team_id = cert.apple_team_id().ok_or_else(|| {
                AppleCodesignError::PolicyFormulationError(format!(
                    "(deriving for {:?}) could not find team identifier in signing certificate",
                    profile
                ))
            })?;

            let expr = CodeRequirementExpression::And(
                // Chains to Apple root CA.
                Box::new(CodeRequirementExpression::AnchorAppleGeneric),
                Box::new(CodeRequirementExpression::And(
                    // Certificate issued by CA with Developer ID extension.
                    Box::new(CodeRequirementExpression::CertificateGeneric(
                        1,
                        CertificateAuthorityExtension::DeveloperId.as_oid(),
                        CodeRequirementMatchExpression::Exists,
                    )),
                    Box::new(CodeRequirementExpression::And(
                        // A certificate entrusted with Developer ID Application signing rights.
                        Box::new(CodeRequirementExpression::CertificateGeneric(
                            0,
                            CodeSigningCertificateExtension::DeveloperIdApplication.as_oid(),
                            CodeRequirementMatchExpression::Exists,
                        )),
                        // Signed by this team ID.
                        Box::new(CodeRequirementExpression::CertificateField(
                            0,
                            "subject.OU".to_string().into(),
                            CodeRequirementMatchExpression::Equal(team_id.into()),
                        )),
                    )),
                )),
            );

            Ok(Some(if let Some(identifier) = identifier {
                CodeRequirementExpression::And(
                    Box::new(CodeRequirementExpression::Identifier(identifier.into())),
                    Box::new(expr),
                )
            } else {
                expr
            }))
        }
        CertificateProfile::MacInstallerDistribution | CertificateProfile::DeveloperIdInstaller => {
            Err(AppleCodesignError::PolicyFormulationError(format!(
                "(deriving for {:?}) we do not know how to handle this policy",
                profile
            )))
        }
    }
}
