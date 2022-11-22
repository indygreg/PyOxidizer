// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Cross-platform interface for code signing.
//!
//! This crate implements functionality for performing code signing in
//! a platform-agnostic manner. It attempts to abstract over platform
//! differences so users don't care what platform they are running on or
//! what type of entity they are signing. It achieves varying levels
//! of success, depending on limitations of underlying crates.
//!
//! # General Workflow
//!
//! [SigningCertificate] represents a code signing certificate (logically a
//! private key + a public X.509 certificate). Instances are constructed
//! with a reference to a code signing certificate in one of the enumerated
//! supported locations.
//!
//! [SigningCertificate] are converted into [Signer], which is a slightly
//! broader scoped entity. [Signer] holds additional attributes beyond the
//! [SigningCertificate], such as a list of issuing [CapturedX509Certificate]
//! constituting the certificate chain and a Time-Stamp Protocol server URL to
//! use.
//!
//! [SignableCandidate] represents the different potential data types that can
//! be signed. e.g. a filesystem path or slice of data.
//!
//! [Signer] exposes a concrete test for whether a [SignableCandidate] is
//! signable, via [Signer::resolve_signability]. The test relies on heuristics
//! in the supported signing *backends* (e.g. [apple_codesign] and
//! [tugger_windows_codesign]) as well as [Signer] specific state to determine
//! if an entity is signable. False positives and negatives are possible:
//! please report bugs! If an entity is signable, it will be converted to a
//! [Signable] instance.
//!
//! To sign a [Signable], you need to obtain a [SignableSigner].
//! You can do this by calling [Signer::resolve_signer]. This function performs
//! signability checks internally and returns `None` if unsignable entities are
//! seen. So most consumers with an intent to sign should call this without
//! calling [Signer::resolve_signability] to avoid a potentially expensive
//! double signability test.
//!
//! [SignableSigner] are associated with a [Signer] and [Signable] and are
//! used for signing just a single entity. [SignableSigner] instances are guaranteed
//! to be capable of signing a [Signable]. However, signing operations support
//! specifying writing the signed results to multiple types of destinations,
//! as specified via [SigningDestination], and not every destination is supported
//! by every input or signing backend. So before attempting signing, it is a good
//! idea to call [SignableSigner.destination_compatibility] and verify that
//! writing to a specific [SigningDestination] is supported!
//!
//! [SignableSigner] instances can further be customized to influence signing
//! settings. See its documentation for available settings. For power users,
//! callback functions can be registered on [Signer] instances to allow customization
//! of the low-level signing primitives used for signing individual [Signable]. See
//! [Signer::apple_settings_callback] and [Signer::windows_settings_callback].
//!
//! Finally, a signing operation can be performed via [SignableSigner::sign].
//! This hides away all the complexity of mapping different signable entities
//! to different signing *backends* and gives you a relatively clean interface
//! to attempt code signing. If signing was successful, you'll get a
//! [SignedOutput] describing where the signed content lives.

use {
    apple_codesign::{cryptography::InMemoryPrivateKey, AppleCodesignError, MachOSigner},
    cryptographic_message_syntax::CmsError,
    log::warn,
    reqwest::{IntoUrl, Url},
    simple_file_manifest::{File, FileData, FileEntry},
    std::{
        borrow::Cow,
        ops::Deref,
        path::{Path, PathBuf},
        sync::Arc,
    },
    thiserror::Error,
    tugger_windows_codesign::{
        CodeSigningCertificate, FileBasedCodeSigningCertificate, SystemStore,
    },
    x509_certificate::{CapturedX509Certificate, X509CertificateError},
    yasna::ASN1Error,
};

/// URL of Apple's time-stamp protocol server.
pub const APPLE_TIMESTAMP_URL: &str = "http://timestamp.apple.com/ts01";

/// Represents a signing error.
#[derive(Debug, Error)]
pub enum SigningError {
    #[error("could not determine if path is signable: {0}")]
    SignableTestError(String),

    /// General I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("error reading ASN.1 data: {0}")]
    Asn1(#[from] ASN1Error),

    #[error("cryptography error: {0}")]
    Cms(#[from] CmsError),

    #[error("no certificate data was found")]
    NoCertificateData,

    #[error("incorrect decryption password")]
    BadDecryptionPassword,

    #[error("PFX reading error: {0}")]
    PfxRead(String),

    #[error("{0}")]
    BadWindowsCertificateStore(String),

    #[error("bad URL: {0}")]
    BadUrl(reqwest::Error),

    #[error("macOS keychain integration only supported on macOS")]
    MacOsKeychainNotSupported,

    #[error("failed to resolve signing certificate: {0}")]
    CertificateResolutionFailure(String),

    #[error("certificate not usable: {0}")]
    CertificateNotUsable(String),

    #[error("error resolving certificate chain: {0}")]
    MacOsCertificateChainResolveFailure(AppleCodesignError),

    #[error("path {0} is not signable")]
    PathNotSignable(PathBuf),

    #[error("error signing mach-o binary: {0}")]
    MachOSigningError(AppleCodesignError),

    #[error("error signing Apple bundle: {0}")]
    AppleBundleSigningError(AppleCodesignError),

    #[error("error running settings callback: {0}")]
    SettingsCallback(anyhow::Error),

    #[error("error running signtool: {0}")]
    SigntoolError(anyhow::Error),

    #[error("incompatible signing destination: {0}")]
    IncompatibleSigningDestination(&'static str),

    #[error("error when signing: {0}")]
    GeneralSigning(String),

    #[error("X.509 certificate handling error: {0}")]
    X509Certificate(#[from] X509CertificateError),
}

/// Represents a location where signed data should be written.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SigningDestination {
    /// Sign to a file at the given path.
    File(PathBuf),

    /// Sign to a directory at the given path.
    Directory(PathBuf),

    /// Sign to data in memory.
    Memory,
}

/// Describes capability of signing some entity to a [SigningDestination].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SigningDestinationCompatibility {
    /// Signing to a particular destination is supported.
    Compatible,

    /// Signing is not supported for a given reason.
    Incompatible(&'static str),
}

/// Describes the output of a successful signing operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SignedOutput {
    /// Signed data was written to a file at a given path.
    File(PathBuf),

    /// Signed data was written to a directory at a given path.
    Directory(PathBuf),

    /// Signed data was written to memory.
    Memory(Vec<u8>),
}

/// Represents how an entity can be signed.
///
/// This is used to describe what potential [SigningDestination] can be used
/// for a signing operation and what steps the signing operation should
/// perform (e.g. using temporary files to sign).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SigningMethod {
    /// Entity is backed by a file and can be signed in place.
    InPlaceFile,

    /// Entity is backed by a directory and can be signed in place.
    InPlaceDirectory,

    /// Entity can be signed into an arbitrary file.
    NewFile,

    /// Entity can be signed to an arbitrary directory.
    NewDirectory,

    /// Signed data can be written to memory.
    Memory,
}

/// Represents different methods of signing that are supported.
///
/// Just a collection of [SigningMethod] instances.
pub struct SigningMethods(Vec<SigningMethod>);

impl Deref for SigningMethods {
    type Target = Vec<SigningMethod>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Represents an entity that is a candidate for signing.
///
/// Most users will want to use [Self::Path] or [Self::Data], which will go
/// through signability checks and only turn into signable entities if we have
/// a high degree of confidence that they can be signed.
///
/// The [Self::Forced] variant can be used to forcefully skip signability
/// validation and supply your own [Signability]. Use this when our signability
/// heuristics fail (please consider reporting these scenarios as bugs!). This
/// variant is also useful for testing.
pub enum SignableCandidate<'a> {
    /// A filesystem path.
    ///
    /// Will be checked for signability.
    Path(Cow<'a, Path>),

    /// A slice of data in memory.
    ///
    /// Will be checked for signability.
    Data(Cow<'a, [u8]>),

    /// Entity whose [Signable] is already computed.
    Forced(Signable),
}

impl<'a> From<&'a Path> for SignableCandidate<'a> {
    fn from(p: &'a Path) -> Self {
        Self::Path(Cow::Borrowed(p))
    }
}

impl<'a> From<PathBuf> for SignableCandidate<'static> {
    fn from(p: PathBuf) -> Self {
        Self::Path(Cow::Owned(p))
    }
}

impl<'a> From<&'a [u8]> for SignableCandidate<'a> {
    fn from(b: &'a [u8]) -> Self {
        Self::Data(Cow::Borrowed(b))
    }
}

impl<'a> From<Vec<u8>> for SignableCandidate<'static> {
    fn from(data: Vec<u8>) -> Self {
        Self::Data(Cow::Owned(data))
    }
}

impl<'a> TryFrom<FileData> for SignableCandidate<'static> {
    type Error = anyhow::Error;

    fn try_from(file: FileData) -> Result<Self, Self::Error> {
        Ok(Self::Data(Cow::Owned(file.resolve_content()?)))
    }
}

impl<'a> TryFrom<&FileData> for SignableCandidate<'static> {
    type Error = anyhow::Error;

    fn try_from(file: &FileData) -> Result<Self, Self::Error> {
        Ok(Self::Data(Cow::Owned(file.resolve_content()?)))
    }
}

impl<'a> TryFrom<FileEntry> for SignableCandidate<'static> {
    type Error = anyhow::Error;

    fn try_from(entry: FileEntry) -> Result<Self, Self::Error> {
        SignableCandidate::try_from(entry.file_data())
    }
}

impl<'a> TryFrom<&FileEntry> for SignableCandidate<'static> {
    type Error = anyhow::Error;

    fn try_from(entry: &FileEntry) -> Result<Self, Self::Error> {
        SignableCandidate::try_from(entry.file_data())
    }
}

impl<'a> TryFrom<File> for SignableCandidate<'static> {
    type Error = anyhow::Error;

    fn try_from(file: File) -> Result<Self, Self::Error> {
        SignableCandidate::try_from(file.entry().file_data())
    }
}

impl<'a> TryFrom<&File> for SignableCandidate<'static> {
    type Error = anyhow::Error;

    fn try_from(file: &File) -> Result<Self, Self::Error> {
        SignableCandidate::try_from(file.entry().file_data())
    }
}

/// Represents a known, typed entity which is signable.
#[derive(Clone, Debug)]
pub enum Signable {
    /// A file that is signable on Windows.
    WindowsFile(PathBuf),

    /// Data that is signable on Windows.
    ///
    /// TODO store a Cow.
    WindowsData(Vec<u8>),

    /// A signable Mach-O file.
    ///
    /// We have to obtain the Mach-O data as part of evaluating whether it is
    /// signable. So we keep a reference to it to avoid a re-read later.
    MachOFile(PathBuf, Vec<u8>),

    /// Signable Mach-O data.
    MachOData(Vec<u8>),

    /// An Apple bundle, persisted on the filesystem as a directory.
    AppleBundle(PathBuf),
}

impl Signable {
    /// Obtain signing methods that are supported.
    pub fn signing_methods(&self) -> SigningMethods {
        SigningMethods(match self {
            Self::WindowsFile(_) => {
                vec![
                    // signtool.exe signs in place by default.
                    SigningMethod::InPlaceFile,
                    // We support copying to a new file and signing that.
                    SigningMethod::NewFile,
                    // We supporting copying to a temporary file and signing that.
                    SigningMethod::Memory,
                ]
            }
            Self::WindowsData(_) => {
                vec![
                    // We support writing data to a new file and signing that.
                    SigningMethod::NewFile,
                    // We supporting writing data to a temporary file, signing, and loading.
                    SigningMethod::Memory,
                ]
            }
            Self::MachOFile(_, _) => {
                // apple-codesign does all of these easily.
                vec![
                    SigningMethod::InPlaceFile,
                    SigningMethod::NewFile,
                    SigningMethod::Memory,
                ]
            }
            Self::MachOData(_) => {
                // apple-codesign does all of these easily.
                vec![SigningMethod::NewFile, SigningMethod::Memory]
            }
            Self::AppleBundle(_) => {
                // apple-codesign can sign in place or to a new directory.
                vec![SigningMethod::InPlaceDirectory, SigningMethod::NewDirectory]
            }
        })
    }

    /// Whether we are capable of signing.
    pub fn is_signable(&self) -> bool {
        !self.signing_methods().is_empty()
    }

    /// Obtain the filesystem path of the signable entity, if it is backed by a file.
    pub fn source_file(&self) -> Option<&Path> {
        match self {
            Self::WindowsFile(p) => Some(p.as_path()),
            Self::MachOFile(p, _) => Some(p.as_path()),
            Self::WindowsData(_) | Self::MachOData(_) | Self::AppleBundle(_) => None,
        }
    }

    /// Obtain the filesystem path of the signable directory, if it is backed by a directory.
    pub fn source_directory(&self) -> Option<&Path> {
        match self {
            Self::AppleBundle(p) => Some(p.as_path()),
            Self::WindowsFile(_)
            | Self::WindowsData(_)
            | Self::MachOFile(_, _)
            | Self::MachOData(_) => None,
        }
    }

    /// Resolves the compatibility for signing this entity to a given [SigningDestination].
    pub fn destination_compatibility(
        &self,
        destination: &SigningDestination,
    ) -> SigningDestinationCompatibility {
        let methods = self.signing_methods();

        match destination {
            SigningDestination::Memory => {
                if methods.iter().any(|x| *x == SigningMethod::Memory) {
                    SigningDestinationCompatibility::Compatible
                } else {
                    SigningDestinationCompatibility::Incompatible("signing to memory not supported")
                }
            }
            SigningDestination::File(dest_path) => {
                let same_file = if let Some(source_path) = self.source_file() {
                    source_path == dest_path
                } else {
                    false
                };

                let compatible = methods.iter().any(|x| match x {
                    SigningMethod::InPlaceFile => same_file,
                    SigningMethod::NewFile => true,
                    SigningMethod::InPlaceDirectory
                    | SigningMethod::NewDirectory
                    | SigningMethod::Memory => false,
                });

                if compatible {
                    SigningDestinationCompatibility::Compatible
                } else if same_file {
                    SigningDestinationCompatibility::Incompatible(
                        "signing file in place not supported",
                    )
                } else {
                    SigningDestinationCompatibility::Incompatible(
                        "signing to a new file not supported",
                    )
                }
            }
            SigningDestination::Directory(dest_dir) => {
                let same_dir = if let Some(source_dir) = self.source_directory() {
                    source_dir == dest_dir
                } else {
                    false
                };

                let compatible = methods.iter().any(|x| match x {
                    SigningMethod::InPlaceDirectory => same_dir,
                    SigningMethod::NewDirectory => true,
                    SigningMethod::InPlaceFile | SigningMethod::NewFile | SigningMethod::Memory => {
                        false
                    }
                });

                if compatible {
                    SigningDestinationCompatibility::Compatible
                } else if same_dir {
                    SigningDestinationCompatibility::Incompatible(
                        "signing directory in place not supported",
                    )
                } else {
                    SigningDestinationCompatibility::Incompatible(
                        "signing to a new directory not supported",
                    )
                }
            }
        }
    }
}

/// Represents the results of a signability test.
#[derive(Debug)]
pub enum Signability {
    /// A known entity which is signable.
    Signable(Signable),

    /// The entity is not signable for undetermined reason.
    Unsignable,

    /// The entity is a Mach-O binary that cannot be signed.
    UnsignableMachoError(AppleCodesignError),

    /// The entity is signable, but not from this platform. Details of the
    /// limitation are stored in a string.
    PlatformUnsupported(&'static str),
}

/// Resolve signability information given an input path.
///
/// The path can be to a file or directory.
///
/// Returns `Err` if we could not fully test the path. This includes
/// I/O failures.
pub fn path_signable(path: impl AsRef<Path>) -> Result<Signability, SigningError> {
    let path = path.as_ref();

    if path.is_file() {
        match tugger_windows_codesign::is_file_signable(path) {
            Ok(true) => {
                // But we can only sign Windows binaries on Windows since we call out to
                // signtool.exe.
                return if cfg!(target_family = "windows") {
                    Ok(Signability::Signable(Signable::WindowsFile(
                        path.to_path_buf(),
                    )))
                } else {
                    Ok(Signability::PlatformUnsupported(
                        "Windows signing requires running on Windows",
                    ))
                };
            }
            Ok(false) => {}
            Err(e) => return Err(SigningError::SignableTestError(format!("{:?}", e))),
        }

        let data = std::fs::read(path)?;

        if goblin::mach::Mach::parse(&data).is_ok() {
            // Try to construct a signer to see if the binary is compatible.
            return Ok(match MachOSigner::new(&data) {
                Ok(_) => Signability::Signable(Signable::MachOFile(path.to_path_buf(), data)),
                Err(e) => Signability::UnsignableMachoError(e),
            });
        }
    } else if path.is_dir() && apple_bundles::DirectoryBundle::new_from_path(path).is_ok() {
        return Ok(Signability::Signable(Signable::AppleBundle(
            path.to_path_buf(),
        )));
    }

    Ok(Signability::Unsignable)
}

/// Resolve signability information given a data slice.
pub fn data_signable(data: &[u8]) -> Result<Signability, SigningError> {
    if tugger_windows_codesign::is_signable_binary_header(data) {
        // But we can only sign Windows binaries on Windows since we call out to
        // signtool.exe.
        return if cfg!(target_family = "windows") {
            Ok(Signability::Signable(Signable::WindowsData(
                data.as_ref().to_vec(),
            )))
        } else {
            Ok(Signability::PlatformUnsupported(
                "Windows signing requires running on Windows",
            ))
        };
    }

    if goblin::mach::Mach::parse(data).is_ok() {
        // Try to construct a signer to see if the binary is compatible.
        return Ok(match MachOSigner::new(data) {
            Ok(_) => Signability::Signable(Signable::MachOData(data.to_vec())),
            Err(e) => Signability::UnsignableMachoError(e),
        });
    }

    Ok(Signability::Unsignable)
}

/// Represents a signing key and public certificate to sign something.
#[derive(Debug)]
pub enum SigningCertificate {
    /// A parsed certificate and signing key stored in memory.
    ///
    /// The private key is managed by the `ring` crate.
    Memory(CapturedX509Certificate, InMemoryPrivateKey),

    /// A PFX file containing validated certificate data.
    ///
    /// The password to open the file is also tracked.
    PfxFile(PathBuf, String, CapturedX509Certificate, InMemoryPrivateKey),

    /// Use an automatically chosen certificate in the Windows certificate store.
    WindowsStoreAuto,

    /// A certificate stored in a Windows certificate store with a subject name string.
    ///
    /// See [SystemStore] for the possible system stores. [SystemStore::My] (the
    /// current user's store) is typically where code signing certificates are
    /// located.
    ///
    /// The string defines a value to match against in the certificate's `subject`
    /// field to locate the certificate.
    WindowsStoreSubject(SystemStore, String),

    /// A certificate stored in a Windows certificate with a specified SHA-1 thumbprint.
    ///
    /// See [SystemStore] for the possible system stores. [SystemStore::My] (the
    /// current user's store) is typically where code signing certificates re located.
    ///
    /// The string defines the SHA-1 thumbprint of the certificate. You can find this
    /// in the `Details` tab of the certificate when viewed in `certmgr.msc`.
    WindowsStoreSha1Thumbprint(SystemStore, String),
}

impl SigningCertificate {
    /// Obtain an instance referencing a file containing PFX / PKCS #12 data.
    ///
    /// This is like [Self::from_pfx_data] except the certificate is referenced by path
    /// instead of persisted into memory. However, we do read the certificate data
    /// as part of constructing the instance to verify the certificate is well-formed.
    pub fn from_pfx_file(path: impl AsRef<Path>, password: &str) -> Result<Self, SigningError> {
        let data = std::fs::read(path.as_ref())?;

        // Validate the certificate is valid.
        let (cert, key) = apple_codesign::cryptography::parse_pfx_data(&data, password)
            .map_err(|e| SigningError::PfxRead(format!("{:?}", e)))?;

        Ok(Self::PfxFile(
            path.as_ref().to_path_buf(),
            password.to_string(),
            cert,
            key,
        ))
    }

    /// Obtain an instance by parsing PFX / PKCS #12 data.
    ///
    /// PFX data is commonly encountered in `.pfx` or `.p12` files, such as
    /// those created when exporting certificates from Apple's `Keychain Access`
    /// or Windows' `certmgr`.
    ///
    /// The contents of the file require a password to decrypt. However, if no
    /// password was provided to create the data, this password may be the
    /// empty string.
    pub fn from_pfx_data(data: &[u8], password: &str) -> Result<Self, SigningError> {
        let (cert, key) = apple_codesign::cryptography::parse_pfx_data(data, password)
            .map_err(|e| SigningError::PfxRead(format!("{:?}", e)))?;

        Ok(Self::Memory(cert, key))
    }

    /// Construct an instance referring to a named certificate in a Windows certificate store.
    ///
    /// `store` is the name of a Windows certificate store to open. See
    /// [SystemStore] for possible values. The `My` store (the store for the current
    /// user) is likely where code signing certificates live.
    ///
    /// `subject` is a string to match against the certificate's `subject` field
    /// to locate the certificate.
    pub fn windows_store_with_subject(
        store: &str,
        subject: impl ToString,
    ) -> Result<Self, SigningError> {
        let store =
            SystemStore::try_from(store).map_err(SigningError::BadWindowsCertificateStore)?;

        Ok(Self::WindowsStoreSubject(store, subject.to_string()))
    }

    /// Construct an instance referring to a certificate with a SHA-1 thumbprint in a Windows certificate store.
    ///
    /// `store` is the name of a Windows certificate store to open. See
    /// [SystemStore] for possible values. The `My` store (the store for the current
    /// user) is likely where code signing certificates live.
    ///
    /// `thumbprint` is the SHA-1 thumbprint of the certificate. It should uniquely identify
    /// any X.509 certificate.
    pub fn windows_store_with_sha1_thumbprint(
        store: &str,
        thumbprint: impl ToString,
    ) -> Result<Self, SigningError> {
        let store =
            SystemStore::try_from(store).map_err(SigningError::BadWindowsCertificateStore)?;

        Ok(Self::WindowsStoreSha1Thumbprint(
            store,
            thumbprint.to_string(),
        ))
    }

    /// Attempt to convert this instance to a [CodeSigningCertificate] for use signing on Windows.
    pub fn to_windows_code_signing_certificate(
        &self,
    ) -> Result<CodeSigningCertificate, SigningError> {
        match self {
            Self::WindowsStoreAuto => Ok(CodeSigningCertificate::Auto),
            Self::WindowsStoreSha1Thumbprint(store, thumbprint) => Ok(
                CodeSigningCertificate::Sha1Thumbprint(*store, thumbprint.clone()),
            ),
            Self::WindowsStoreSubject(store, subject) => {
                Ok(CodeSigningCertificate::SubjectName(*store, subject.clone()))
            }
            Self::PfxFile(path, password, _, _) => {
                let mut f = FileBasedCodeSigningCertificate::new(path);
                f.set_password(password);

                Ok(CodeSigningCertificate::File(f))
            }
            Self::Memory(_, _) => {
                // This requires support for materializing the certificate to a
                // temporary file or something.
                unimplemented!();
            }
        }
    }
}

/// A callback for influencing the creation of [apple_codesign::SigningSettings]
/// instances for a given [Signable].
pub type AppleSigningSettingsFn =
    fn(&Signable, &mut apple_codesign::SigningSettings) -> Result<(), anyhow::Error>;

/// A callback for influencing the creation of [tugger_windows_codesign::SigntoolSign]
/// instances for a given [Signable].
pub type WindowsSignerFn =
    fn(&Signable, &mut tugger_windows_codesign::SigntoolSign) -> Result<(), anyhow::Error>;

/// An entity for performing code signing.
///
/// This contains the [SigningCertificate] as well as other global signing
/// settings.
pub struct Signer {
    /// The signing certificate to use.
    signing_certificate: SigningCertificate,

    /// The certificates that signed the signing certificate.
    ///
    /// Ideally this contains the full certificate chain, leading to the
    /// root CA.
    certificate_chain: Vec<CapturedX509Certificate>,

    /// URL of Time-Stamp Protocol server to use.
    time_stamp_url: Option<Url>,

    /// Optional function to influence creation of [apple_codesign::SigningSettings]
    /// used for signing Apple signables.
    apple_signing_settings_fn: Option<Arc<AppleSigningSettingsFn>>,

    /// Optional function to influence creation of [tugger_windows_codesign::SigntoolSign]
    /// used for signing Windows signables.
    windows_signer_fn: Option<Arc<WindowsSignerFn>>,
}

impl From<SigningCertificate> for Signer {
    fn from(cert: SigningCertificate) -> Self {
        Self::new(cert)
    }
}

impl Signer {
    /// Construct a new instance given a [SigningCertificate].
    pub fn new(signing_certificate: SigningCertificate) -> Self {
        Self {
            signing_certificate,
            certificate_chain: vec![],
            time_stamp_url: None,
            apple_signing_settings_fn: None,
            windows_signer_fn: None,
        }
    }

    /// Add an X.509 certificate to the certificate chain.
    ///
    /// When signing, it is common to include the chain of certificates
    /// that signed the signing certificate in the signature. This can
    /// facilitate with validation of the signature.
    ///
    /// This function can be called to register addition certificates
    /// into the signing chain.
    pub fn chain_certificate(&mut self, certificate: CapturedX509Certificate) {
        self.certificate_chain.push(certificate);
    }

    /// Add PEM encoded X.509 certificates to the certificate chain.
    ///
    /// This is like [Self::chain_certificate] except the certificate is specified as
    /// PEM encoded data. This is a human readable string like
    /// `-----BEGIN CERTIFICATE-----` and is a common method for encoding
    /// certificate data. The specified data can contain multiple certificates.
    pub fn chain_certificates_pem(&mut self, data: impl AsRef<[u8]>) -> Result<(), SigningError> {
        let certs = CapturedX509Certificate::from_pem_multiple(data)?;

        if certs.is_empty() {
            Err(SigningError::NoCertificateData)
        } else {
            self.certificate_chain.extend(certs);
            Ok(())
        }
    }

    /// Add multiple X.509 certificates to the certificate chain.
    ///
    /// See [Self::chain_certificate] for details.
    pub fn chain_certificates(
        &mut self,
        certificates: impl Iterator<Item = CapturedX509Certificate>,
    ) {
        self.certificate_chain.extend(certificates);
    }

    /// Chain X.509 certificates by searching for them in the macOS keychain.
    ///
    /// This function will access the macOS keychain and attempt to locate
    /// the certificates composing the signing chain of the currently configured
    /// signing certificate.
    ///
    /// This function only works when run on macOS.
    ///
    /// This function will error if the signing certificate wasn't self-signed
    /// and its issuer chain could not be resolved.
    #[cfg(target_os = "macos")]
    pub fn chain_certificates_macos_keychain(&mut self) -> Result<(), SigningError> {
        let cert: &CapturedX509Certificate = match &self.signing_certificate {
            SigningCertificate::Memory(cert, _) => Ok(cert),
            _ => Err(SigningError::CertificateResolutionFailure(
                "can only operate on signing certificates loaded into memory".to_string(),
            )),
        }?;

        if cert.subject_is_issuer() {
            return Ok(());
        }

        let user_id = cert
            .subject_name()
            .find_first_attribute_string(bcder::Oid(apple_codesign::OID_USER_ID.as_ref().into()))
            .map_err(|e| {
                SigningError::CertificateResolutionFailure(format!(
                    "failed to decode UID field in signing certificate: {:?}",
                    e
                ))
            })?
            .ok_or_else(|| {
                SigningError::CertificateResolutionFailure(
                    "could not find UID in signing certificate".to_string(),
                )
            })?;

        let domain = apple_codesign::KeychainDomain::User;

        let certs = apple_codesign::macos_keychain_find_certificate_chain(domain, None, &user_id)
            .map_err(SigningError::MacOsCertificateChainResolveFailure)?;

        if certs.is_empty() {
            return Err(SigningError::CertificateResolutionFailure(
                "issuing certificates not found in macOS keychain".to_string(),
            ));
        }

        if !certs[certs.len() - 1].subject_is_issuer() {
            return Err(SigningError::CertificateResolutionFailure(
                "unable to resolve entire signing certificate chain; root certificate not found"
                    .to_string(),
            ));
        }

        self.certificate_chain.extend(certs);
        Ok(())
    }

    /// Chain X.509 certificates by searching for them in the macOS keychain.
    ///
    /// This function will access the macOS keychain and attempt to locate
    /// the certificates composing the signing chain of the currently configured
    /// signing certificate.
    ///
    /// This function only works when run on macOS.
    ///
    /// This function will error if the signing certificate wasn't self-signed
    /// and its issuer chain could not be resolved.
    #[cfg(not(target_os = "macos"))]
    #[allow(unused_mut)]
    pub fn chain_certificates_macos_keychain(&mut self) -> Result<(), SigningError> {
        Err(SigningError::MacOsKeychainNotSupported)
    }

    /// Set the URL of a Time-Stamp Protocol server to use.
    ///
    /// If specified, the server will always be used. In some cases, a
    /// Time-Stamp Protocol server will be used automatically if one is
    /// not specified.
    pub fn time_stamp_url(&mut self, url: impl IntoUrl) -> Result<(), SigningError> {
        let url = url.into_url().map_err(SigningError::BadUrl)?;
        self.time_stamp_url = Some(url);
        Ok(())
    }

    /// Set a callback function to be called to influence settings for signing individual Apple signables.
    pub fn apple_settings_callback(&mut self, cb: AppleSigningSettingsFn) {
        self.apple_signing_settings_fn = Some(Arc::new(cb));
    }

    /// Set a callback function to be called to influence settings for signing individual Windows signables.
    pub fn windows_settings_callback(&mut self, cb: WindowsSignerFn) {
        self.windows_signer_fn = Some(Arc::new(cb));
    }

    /// Determine the *signability* of a potentially signable entity.
    pub fn resolve_signability(
        &self,
        candidate: &SignableCandidate,
    ) -> Result<Signability, SigningError> {
        let signability = match candidate {
            SignableCandidate::Path(path) => path_signable(path),
            SignableCandidate::Data(data) => data_signable(data.as_ref()),
            SignableCandidate::Forced(signable) => Ok(Signability::Signable(signable.clone())),
        }?;

        // We don't yet support exporting the key back to PFX for Windows signing.
        if matches!(
            signability,
            Signability::Signable(Signable::WindowsFile(_))
                | Signability::Signable(Signable::WindowsData(_))
        ) && matches!(self.signing_certificate, SigningCertificate::Memory(_, _))
        {
            Ok(Signability::PlatformUnsupported(
                "do not support PFX key re-export on Windows",
            ))
        } else {
            Ok(signability)
        }
    }

    /// Attempt to resolve a [SignableSigner] for the [SignableCandidate].
    ///
    /// This will determine if a given entity can be signed by us. If so, we will
    /// return a `Some(T)` that can be used to sign it. If the entity is not signable,
    /// returns a `None`.
    ///
    /// If an error occurs computing signability, `Err` occurs.
    pub fn resolve_signer(
        &self,
        candidate: &SignableCandidate,
    ) -> Result<Option<SignableSigner<'_>>, SigningError> {
        let signability = self.resolve_signability(candidate)?;

        if let Signability::Signable(entity) = signability {
            Ok(Some(SignableSigner::new(self, entity)))
        } else {
            Ok(None)
        }
    }
}

/// A single invocation of a signing operation.
///
/// Instances are constructed from a [Signer] and [Signability] result and
/// are used to sign a single item. Instances can be customized to tailor
/// signing just the entity in question.
pub struct SignableSigner<'a> {
    /// The signing certificate to use.
    signing_certificate: &'a SigningCertificate,

    /// The thing we are signing.
    signable: Signable,

    /// The certificates that signed the signing certificate.
    ///
    /// Ideally this contains the full certificate chain, leading to the
    /// root CA.
    certificate_chain: Vec<CapturedX509Certificate>,

    /// URL of Time-Stamp Protocol server to use.
    time_stamp_url: Option<Url>,

    /// Optional function to influence creation of [apple_codesign::SigningSettings]
    /// used for signing Apple signables.
    apple_signing_settings_fn: Option<Arc<AppleSigningSettingsFn>>,

    /// Optional function to influence creation of [tugger_windows_codesign::SigntoolSign]
    /// used for signing Windows signables.
    windows_signer_fn: Option<Arc<WindowsSignerFn>>,
}

impl<'a> SignableSigner<'a> {
    fn new(signer: &'a Signer, signable: Signable) -> Self {
        let signing_certificate = &signer.signing_certificate;
        let certificate_chain = signer.certificate_chain.clone();
        let time_stamp_url = signer.time_stamp_url.clone();

        Self {
            signing_certificate,
            signable,
            certificate_chain,
            time_stamp_url,
            apple_signing_settings_fn: signer.apple_signing_settings_fn.clone(),
            windows_signer_fn: signer.windows_signer_fn.clone(),
        }
    }

    /// Obtain a reference to the underlying [Signable].
    pub fn signable(&self) -> &Signable {
        &self.signable
    }

    /// Obtain a [SigningDestination] that is the same as the input.
    pub fn in_place_destination(&self) -> SigningDestination {
        match &self.signable {
            Signable::WindowsFile(path) => SigningDestination::File(path.clone()),
            Signable::MachOFile(path, _) => SigningDestination::File(path.clone()),
            Signable::AppleBundle(path) => SigningDestination::Directory(path.clone()),
            Signable::WindowsData(_) | Signable::MachOData(_) => SigningDestination::Memory,
        }
    }

    /// Obtain a [apple_codesign::SigningSettings] from this instance.
    pub fn as_apple_signing_settings(
        &self,
    ) -> Result<apple_codesign::SigningSettings<'_>, SigningError> {
        let mut settings = apple_codesign::SigningSettings::default();

        match &self.signing_certificate {
            SigningCertificate::Memory(cert, key) => {
                settings.set_signing_key(key, cert.clone());
            }
            SigningCertificate::PfxFile(_, _, cert, key) => {
                settings.set_signing_key(key, cert.clone());
            }
            SigningCertificate::WindowsStoreSubject(_, _)
            | SigningCertificate::WindowsStoreSha1Thumbprint(_, _)
            | SigningCertificate::WindowsStoreAuto => {
                return Err(SigningError::CertificateNotUsable("certificates in the Windows store are not supported for signing Apple primitives; try using a PFX file-based certificate instead".to_string()));
            }
        };

        // Automatically register Apple CA certificates for convenience.
        settings.chain_apple_certificates();

        for cert in &self.certificate_chain {
            settings.chain_certificate(cert.clone());
        }

        if let Some(url) = &self.time_stamp_url {
            settings
                .set_time_stamp_url(url.clone())
                .expect("shouldn't have failed for already parsed URL");
        } else {
            settings
                .set_time_stamp_url(APPLE_TIMESTAMP_URL)
                .expect("shouldn't have failed for constant URL");
        }

        if let Some(cb) = &self.apple_signing_settings_fn {
            cb(&self.signable, &mut settings).map_err(SigningError::SettingsCallback)?;
        }

        Ok(settings)
    }

    /// Obtain a [tugger_windows_codesign::SigntoolSign] from this instance.
    pub fn as_windows_signer(&self) -> Result<tugger_windows_codesign::SigntoolSign, SigningError> {
        let cert = self
            .signing_certificate
            .to_windows_code_signing_certificate()?;

        let mut signer = tugger_windows_codesign::SigntoolSign::new(cert);

        if let Some(url) = &self.time_stamp_url {
            signer.timestamp_server(tugger_windows_codesign::TimestampServer::Rfc3161(
                url.to_string(),
                "SHA256".to_string(),
            ));
        }

        signer.file_digest_algorithm("SHA256");

        if let Some(cb) = &self.windows_signer_fn {
            cb(&self.signable, &mut signer).map_err(SigningError::SettingsCallback)?;
        }

        Ok(signer)
    }

    /// Compute [SigningDestinationCompatibility] with a given [SigningDestination].
    ///
    /// This takes the current to-be-signed entity into account.
    pub fn destination_compatibility(
        &self,
        destination: &SigningDestination,
    ) -> SigningDestinationCompatibility {
        self.signable.destination_compatibility(destination)
    }

    /// Signs this signable entity to the given destination.
    ///
    /// Callers should probably verify destination compatibility by calling
    /// [Self.destination_compatibility] first. But we will turn it into an
    /// `Err` if the destination isn't compatibile.
    ///
    /// `temp_dir` denotes the path of a writable directory where temporary
    /// files can be created, as needed. If not provided, a new temporary
    /// directory will be managed. In all cases, we attempt to remove temporary
    /// files as part of execution.
    pub fn sign(
        &self,

        temp_dir: Option<&Path>,
        destination: &SigningDestination,
    ) -> Result<SignedOutput, SigningError> {
        if let SigningDestinationCompatibility::Incompatible(reason) =
            self.destination_compatibility(destination)
        {
            return Err(SigningError::IncompatibleSigningDestination(reason));
        }

        let temp_dir = if self.requires_temporary_files(destination) {
            let mut builder = tempfile::Builder::new();
            builder.prefix("tugger-code-sign-");

            Some(if let Some(temp_dir) = temp_dir {
                builder.tempdir_in(temp_dir)
            } else {
                builder.tempdir()
            }?)
        } else {
            None
        };

        match &self.signable {
            Signable::WindowsData(data) => {
                let mut signer = self.as_windows_signer()?;

                // Regardless of what we're writing to, we materialize the file
                // data so signtool can sign it. We always go through a temp
                // file so we don't write to the destination except in cases
                // of success.
                let td = temp_dir.as_ref().unwrap().path();

                let sign_path = td.join("sign_temp");
                warn!(
                    "writing signable Windows data to temporary file to sign: {}",
                    sign_path.display()
                );
                std::fs::write(&sign_path, data)?;

                signer.sign_file(&sign_path);
                signer.run().map_err(SigningError::SigntoolError)?;

                match destination {
                    SigningDestination::Memory => {
                        warn!("signing success; reading signed file to memory");
                        Ok(SignedOutput::Memory(std::fs::read(&sign_path)?))
                    }
                    SigningDestination::File(dest_path) => {
                        if copy_file_needed(dest_path, &sign_path)? {
                            warn!(
                                "signing success; copying signed file to {}",
                                dest_path.display()
                            );
                            std::fs::copy(&sign_path, dest_path)?;
                        } else {
                            warn!("signing success");
                        }

                        Ok(SignedOutput::File(dest_path.clone()))
                    }
                    SigningDestination::Directory(_) => {
                        panic!("illegal signing combination: SignableWindowsData -> :Directory");
                    }
                }
            }
            Signable::WindowsFile(source_file) => {
                let mut signer = self.as_windows_signer()?;

                // We may or may not be going through a temporary file. If we are,
                // copy the file. Otherwise sign in place.
                let sign_path = if let Some(temp_dir) = temp_dir.as_ref().map(|x| x.path()) {
                    let filename = source_file.file_name().ok_or_else(|| {
                        SigningError::GeneralSigning(format!(
                            "unable to resolve filename of {}",
                            source_file.display()
                        ))
                    })?;

                    let sign_path = temp_dir.join(filename);
                    warn!(
                        "copying {} to {} to perform signing",
                        source_file.display(),
                        sign_path.display()
                    );
                    std::fs::copy(source_file, &sign_path)?;

                    sign_path
                } else {
                    warn!("signing {}", source_file.display());
                    source_file.clone()
                };

                signer.sign_file(&sign_path);
                signer.run().map_err(SigningError::SigntoolError)?;

                match destination {
                    SigningDestination::Memory => {
                        warn!("signing success; reading signed file to memory");
                        Ok(SignedOutput::Memory(std::fs::read(&sign_path)?))
                    }
                    SigningDestination::File(dest_path) => {
                        if copy_file_needed(&sign_path, dest_path)? {
                            warn!(
                                "signing success; copying signed file to {}",
                                dest_path.display()
                            );
                            std::fs::copy(&sign_path, dest_path)?;
                        } else {
                            warn!("signing success");
                        }

                        Ok(SignedOutput::File(dest_path.clone()))
                    }
                    SigningDestination::Directory(_) => {
                        panic!("illegal signing combination: SignableWindowsFile -> Directory");
                    }
                }
            }
            Signable::MachOData(macho_data) => {
                warn!(
                    "signing Mach-O binary from in-memory data of size {} bytes",
                    macho_data.len()
                );
                let settings = self.as_apple_signing_settings()?;

                let signer = apple_codesign::MachOSigner::new(macho_data)
                    .map_err(SigningError::MachOSigningError)?;

                let mut dest = Vec::<u8>::with_capacity(macho_data.len() + 2_usize.pow(17));
                signer
                    .write_signed_binary(&settings, &mut dest)
                    .map_err(SigningError::MachOSigningError)?;

                match destination {
                    SigningDestination::Memory => {
                        warn!("Mach-O signing success; new size {}", dest.len());
                        Ok(SignedOutput::Memory(dest))
                    }
                    SigningDestination::File(dest_file) => {
                        warn!("Mach-O signing success; writing to {}", dest_file.display());
                        std::fs::write(dest_file, &dest)?;
                        Ok(SignedOutput::File(dest_file.clone()))
                    }
                    SigningDestination::Directory(_) => {
                        panic!("illegal signing combination: SignableMachOData -> Directory");
                    }
                }
            }
            Signable::MachOFile(source_file, macho_data) => {
                let settings = self.as_apple_signing_settings()?;

                warn!("signing {}", source_file.display());

                let signer = apple_codesign::MachOSigner::new(macho_data)
                    .map_err(SigningError::MachOSigningError)?;

                let mut dest = Vec::<u8>::with_capacity(macho_data.len() + 2_usize.pow(17));
                signer
                    .write_signed_binary(&settings, &mut dest)
                    .map_err(SigningError::MachOSigningError)?;

                match destination {
                    SigningDestination::Memory => {
                        warn!("Mach-O signing success; new size {}", dest.len());
                        Ok(SignedOutput::Memory(dest))
                    }
                    SigningDestination::File(dest_file) => {
                        warn!("Mach-O signing success; writing to {}", dest_file.display());
                        std::fs::write(dest_file, &dest)?;
                        Ok(SignedOutput::File(dest_file.clone()))
                    }
                    SigningDestination::Directory(_) => {
                        panic!("illegal signing combination: SignableMachOPath -> Directory");
                    }
                }
            }
            Signable::AppleBundle(source_dir) => {
                let settings = self.as_apple_signing_settings()?;

                // TODO go through temporary directory when not doing in-place signing.
                let dest_dir = match destination {
                    SigningDestination::Directory(d) => d,
                    _ => panic!("illegal signing combination: SignableAppleBundle -> !Directory"),
                };

                warn!(
                    "signing Apple bundle at {} to {}",
                    source_dir.display(),
                    dest_dir.display()
                );

                let signer = apple_codesign::BundleSigner::new_from_path(source_dir)
                    .map_err(SigningError::AppleBundleSigningError)?;

                signer
                    .write_signed_bundle(dest_dir, &settings)
                    .map_err(SigningError::AppleBundleSigningError)?;

                Ok(SignedOutput::Directory(dest_dir.clone()))
            }
        }
    }

    /// Whether signing to the specified [SigningDestination] will require temporary files.
    ///
    /// Temporary files are used when:
    ///
    /// * Signed content lives in memory and signer only supports signing files.
    ///   (e.g. signtool.exe)
    /// * We are sending output to the filesystem and the destination path isn't the
    ///   source path. We could write directly to the destination. However, we choose
    ///   to play it safe and only write to the destination after signing success.
    ///   By going through a temporary directory, we prevent polluting the destination
    ///   with corrupted results.
    pub fn requires_temporary_files(&self, destination: &SigningDestination) -> bool {
        match &self.signable {
            // signtool only supports signing files. We'll have to persist data to a file.
            Signable::WindowsData(_) => true,
            Signable::WindowsFile(source_file) => match destination {
                // We don't want to touch the original file, so we'll have to create a new one.
                SigningDestination::Memory => true,
                // When signing to a file, we allow in-place signing but always go
                // through a temporary file when writing a new file.
                SigningDestination::File(dest_file) => source_file != dest_file,
                // Signing to a directory isn't supported.
                SigningDestination::Directory(_) => false,
            },
            // apple-codesign does everything in memory and doesn't need files.
            Signable::MachOData(_) | Signable::MachOFile(_, _) => false,
            // But, when we are sending output to the filesystem and the output isn't
            // the input, we go through a temporary directory to prevent writing
            // bad results to the output directory.
            Signable::AppleBundle(source_dir) => match destination {
                SigningDestination::Directory(dest_dir) => source_dir != dest_dir,
                SigningDestination::Memory | SigningDestination::File(_) => false,
            },
        }
    }
}

/// Whether a request to copy between 2 paths needs to be fulfilled.
///
/// We can run into cases where we are writing to the input file but we don't
/// think we are because of path normalization issues. This function adds
/// a test for that.
fn copy_file_needed(source: &Path, dest: &Path) -> Result<bool, std::io::Error> {
    if dest.exists() {
        Ok(source.canonicalize()? != dest.canonicalize()?)
    } else {
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const APPLE_P12_DATA: &[u8] = include_bytes!("apple-codesign-testuser.p12");

    const WINDOWS_PFX_DEFAULT_DATA: &[u8] = include_bytes!("windows-testuser-default.pfx");
    const WINDOWS_PFX_NO_EXTRAS_DATA: &[u8] = include_bytes!("windows-testuser-no-extras.pfx");

    #[test]
    fn parse_apple_p12() {
        SigningCertificate::from_pfx_data(APPLE_P12_DATA, "password123").unwrap();
    }

    #[test]
    fn parse_windows_pfx() {
        SigningCertificate::from_pfx_data(WINDOWS_PFX_DEFAULT_DATA, "password123").unwrap();
        SigningCertificate::from_pfx_data(WINDOWS_PFX_NO_EXTRAS_DATA, "password123").unwrap();
    }

    #[test]
    fn parse_windows_pfx_dynamic() {
        let cert =
            tugger_windows_codesign::create_self_signed_code_signing_certificate("test user")
                .unwrap();
        let pfx_data =
            tugger_windows_codesign::certificate_to_pfx(&cert, "password", "name").unwrap();

        SigningCertificate::from_pfx_data(&pfx_data, "password").unwrap();
    }

    #[test]
    fn windows_store_with_subject() {
        let cert = SigningCertificate::windows_store_with_subject("my", "test user").unwrap();
        assert!(matches!(
            cert,
            SigningCertificate::WindowsStoreSubject(_, _)
        ));
    }
}
