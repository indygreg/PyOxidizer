// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    crate::starlark::{get_context_value, TuggerContextValue},
    anyhow::{anyhow, Context, Result},
    linked_hash_map::LinkedHashMap,
    log::{debug, info, warn},
    simple_file_manifest::{FileEntry, FileManifest},
    starlark::{
        environment::TypeValues,
        eval::call_stack::CallStack,
        values::{
            error::{RuntimeError, UnsupportedOperation, ValueError},
            none::NoneType,
            {Mutable, TypedValue, Value, ValueResult},
        },
        {
            starlark_fun, starlark_module, starlark_parse_param_type, starlark_signature,
            starlark_signature_extraction, starlark_signatures,
        },
    },
    starlark_dialect_build_targets::required_type_arg,
    std::{
        fmt::{Display, Formatter},
        path::{Path, PathBuf},
        sync::{Arc, Mutex},
    },
    tugger_code_signing::{
        SignableCandidate, SignedOutput, Signer, SigningCertificate, SigningDestination,
        SigningError,
    },
};

/// Holds additional code signing settings to influence code signing.
///
/// Likely populated from callbacks in Starlark.
#[derive(Clone, Default)]
pub struct SigningSettings {
    /// Whether to defer to another [Signer].
    pub defer: bool,

    /// Whether to prevent signing of this entity.
    pub prevent_signing: bool,
}

/// Represents a request for a [Signer] to sign something.
#[derive(Clone)]
pub struct SigningRequest {
    /// The action triggering this request.
    pub action: &'static str,

    /// Filename of entity that might be signed.
    pub filename: String,

    /// Filesystem or virtual path of entity that might be signed.
    pub path: Option<String>,
}

impl Display for SigningRequest {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!(
            "SigningRequest<action={}, filename={}, path={:?}>",
            self.action, self.filename, self.path
        ))
    }
}

fn from_code_signing_error(err: SigningError, label: impl ToString) -> ValueError {
    ValueError::Runtime(RuntimeError {
        code: "TUGGER_CODE_SIGNING",
        message: format!("{:?}", err),
        label: label.to_string(),
    })
}

fn error_context<F, T>(label: &str, f: F) -> Result<T, ValueError>
where
    F: FnOnce() -> anyhow::Result<T>,
{
    f().map_err(|e| {
        ValueError::Runtime(RuntimeError {
            code: "TUGGER_CODE_SIGNING",
            message: format!("{:?}", e),
            label: label.to_string(),
        })
    })
}

#[derive(Clone)]
pub struct CodeSignerValue {
    pub inner: Arc<Mutex<Signer>>,

    /// Starlark functions to influence signing operations.
    signing_callback: Option<Value>,
}

impl TypedValue for CodeSignerValue {
    type Holder = Mutable<CodeSignerValue>;
    const TYPE: &'static str = "CodeSigner";

    fn values_for_descendant_check_and_freeze(&self) -> Box<dyn Iterator<Item = Value>> {
        Box::new(std::iter::empty())
    }
}

impl From<SigningCertificate> for CodeSignerValue {
    fn from(cert: SigningCertificate) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Signer::new(cert))),
            signing_callback: None,
        }
    }
}

impl CodeSignerValue {
    fn signer(&self, label: &str) -> Result<std::sync::MutexGuard<Signer>, ValueError> {
        self.inner.try_lock().map_err(|e| {
            ValueError::Runtime(RuntimeError {
                code: "TUGGER_CODE_SIGNING",
                message: format!("error obtaining signer lock: {:?}", e),
                label: label.to_string(),
            })
        })
    }
}

// Starlark methods.
impl CodeSignerValue {
    fn from_pfx_file(path: String, password: String) -> ValueResult {
        let pfx_data = std::fs::read(&path).map_err(|e| {
            ValueError::Runtime(RuntimeError {
                code: "TUGGER_CODE_SIGNING",
                message: format!("error reading file: {:?}", e),
                label: "code_signer_from_pfx_file()".to_string(),
            })
        })?;

        let cert = SigningCertificate::from_pfx_data(&pfx_data, &password)
            .map_err(|e| from_code_signing_error(e, "code_signer_from_pfx_file"))?;

        Ok(Value::new::<CodeSignerValue>(cert.into()))
    }

    fn from_windows_store_sha1_thumbprint(thumbprint: String, store: String) -> ValueResult {
        let cert = SigningCertificate::windows_store_with_sha1_thumbprint(&store, thumbprint)
            .map_err(|e| from_code_signing_error(e, "from_windows_store_sha1_thumbprint"))?;

        Ok(Value::new::<CodeSignerValue>(cert.into()))
    }

    fn from_windows_store_subject(subject: String, store: String) -> ValueResult {
        let cert = SigningCertificate::windows_store_with_subject(&store, &subject)
            .map_err(|e| from_code_signing_error(e, "code_signer_from_windows_store_subject"))?;

        Ok(Value::new::<CodeSignerValue>(cert.into()))
    }

    #[allow(clippy::unnecessary_wraps)]
    fn from_windows_store_auto() -> ValueResult {
        Ok(Value::new::<CodeSignerValue>(
            SigningCertificate::WindowsStoreAuto.into(),
        ))
    }

    fn activate(&self, type_values: &TypeValues) -> ValueResult {
        let context_value = get_context_value(type_values)?;
        let mut context = context_value
            .downcast_mut::<TuggerContextValue>()?
            .ok_or(ValueError::IncorrectParameterType)?;

        context.code_signers.push(Value::new(self.clone()));

        Ok(Value::new(NoneType::None))
    }

    fn chain_issuer_certificates_pem_file(&self, path: String) -> ValueResult {
        let label = "chain_issuer_certificates_pem_file()";

        let mut signer = self.signer(label)?;

        error_context(label, || {
            let pem_data = std::fs::read(&path)?;
            signer.chain_certificates_pem(&pem_data)?;

            Ok(Value::new(NoneType::None))
        })
    }

    fn chain_issuer_certificates_macos_keychain(&self) -> ValueResult {
        let label = "chain_issuer_certificates_macos_keychain()";

        let mut signer = self.signer(label)?;

        error_context(label, || {
            signer.chain_certificates_macos_keychain()?;

            Ok(Value::new(NoneType::None))
        })
    }

    fn set_time_stamp_server(&self, url: String) -> ValueResult {
        let mut signer = self.signer("set_time_stamp_server()")?;

        error_context("set_time_stamp_server()", || {
            signer.time_stamp_url(url)?;

            Ok(Value::new(NoneType::None))
        })
    }

    fn set_signing_callback(&mut self, func: Value) -> ValueResult {
        required_type_arg("func", "function", &func)?;

        self.signing_callback = Some(func);

        Ok(Value::from(NoneType::None))
    }
}

pub struct CodeSigningRequestValue {
    inner: SigningRequest,

    settings: SigningSettings,
}

impl TypedValue for CodeSigningRequestValue {
    type Holder = Mutable<CodeSigningRequestValue>;
    const TYPE: &'static str = "CodeSigningRequest";

    fn values_for_descendant_check_and_freeze(&self) -> Box<dyn Iterator<Item = Value>> {
        Box::new(std::iter::empty())
    }

    fn get_attr(&self, attribute: &str) -> ValueResult {
        Ok(match attribute {
            "action" => Value::from(self.inner.action),
            "filename" => Value::from(self.inner.filename.as_str()),
            "path" => {
                if let Some(path) = &self.inner.path {
                    Value::from(path.as_str())
                } else {
                    Value::from(NoneType::None)
                }
            }
            _ => {
                return Err(ValueError::OperationNotSupported {
                    op: UnsupportedOperation::GetAttr(attribute.to_string()),
                    left: "CodeSigningRequest".to_string(),
                    right: None,
                })
            }
        })
    }

    fn has_attr(&self, attribute: &str) -> Result<bool, ValueError> {
        Ok(matches!(attribute, "action" | "filename" | "path"))
    }

    fn set_attr(&mut self, attribute: &str, value: Value) -> Result<(), ValueError> {
        match attribute {
            "defer" => self.settings.defer = value.to_bool(),
            "prevent_signing" => self.settings.prevent_signing = value.to_bool(),
            _ => {
                return Err(ValueError::OperationNotSupported {
                    op: UnsupportedOperation::SetAttr(attribute.to_string()),
                    left: Self::TYPE.to_string(),
                    right: None,
                })
            }
        }

        Ok(())
    }
}

/// Describes an action triggering a signability check.
///
/// Strongly typed to make it easier for docs to stay in sync with
/// reality. But also extensible by other Starlark dialects to provide
/// their own value via the [Self::Other] variant.
#[derive(Clone, Copy, Debug)]
pub enum SigningAction {
    FileManifestInstall,
    MacOsApplicationBunderCreation,
    WindowsInstallerCreation,
    WindowsInstallerFileAdded,
    Other(&'static str),
}

impl SigningAction {
    fn as_str(&self) -> &'static str {
        match self {
            Self::FileManifestInstall => "file-manifest-install",
            Self::MacOsApplicationBunderCreation => "macos-application-bundle-creation",
            Self::WindowsInstallerCreation => "windows-installer-creation",
            Self::WindowsInstallerFileAdded => "windows-installer-file-added",
            Self::Other(s) => s,
        }
    }
}

/// Provides context for an event that is triggering potential code signing.
///
/// This type exists so we don't have to pass a ton of arguments to
/// [handle_signable_event].
pub struct SigningContext<'a> {
    label: &'static str,
    action: SigningAction,
    filename: PathBuf,
    candidate: &'a SignableCandidate<'a>,
    path: Option<PathBuf>,
    destination: Option<SigningDestination>,
    pretend_output: Option<SignedOutput>,
}

impl<'a> SigningContext<'a> {
    pub fn new(
        label: &'static str,
        action: SigningAction,
        filename: impl AsRef<Path>,
        candidate: &'a SignableCandidate<'a>,
    ) -> Self {
        Self {
            label,
            action,
            filename: filename.as_ref().to_path_buf(),
            candidate,
            path: None,
            destination: None,
            pretend_output: None,
        }
    }

    /// Set the path for this context.
    pub fn set_path(&mut self, path: impl AsRef<Path>) {
        self.path = Some(path.as_ref().to_path_buf());
    }

    /// Set the signing destination for this operation.
    ///
    /// Constructors are strongly advised to set this so they have explicit control
    /// over where the signed entity is written to!
    pub fn set_signing_destination(&mut self, destination: SigningDestination) {
        self.destination = Some(destination);
    }

    /// Set the pretend output for this operation.
    ///
    /// If set, the [SignedOutput] will be used instead of actually performing
    /// code signing.
    ///
    /// This is intended to facilitate testing, stubbing out code signing
    /// at the last possible instance.
    pub fn set_pretend_output(&mut self, output: SignedOutput) {
        self.pretend_output = Some(output);
    }
}

/// Represents the execution results of a signing event.
#[derive(Default)]
pub struct SigningResponse {
    /// Total number of [Signer] that exist.
    pub signers_count: usize,

    /// Total number of [Signer] that were consulted for this event.
    ///
    /// *Consultation* stops once a [Signer] handles the event or instructs
    /// processing to stop.
    pub signers_consulted: usize,

    /// Index of [Signer] that prevented signing.
    pub prevented_index: Option<usize>,

    /// Number of signers that deferred to process this request.
    pub defer_count: usize,

    /// Index of the [Signer] that signed this response.
    pub signed_index: Option<usize>,

    /// The output of a successful code signing operation.
    pub output: Option<SignedOutput>,
}

/// Starlark handler for code signing events.
///
/// This is what Starlark code should call when it wants to trigger a
/// signability check for content.
///
/// This function will obtain a reference to all the signers and
/// perform all the iterations, checks, and callbacks to influence
/// code signing.
///
/// If extending our Starlark dialect, you can use the [SigningAction::Other]
/// variant to define custom *action* names.
pub fn handle_signable_event(
    type_values: &TypeValues,
    call_stack: &mut CallStack,
    request_context: SigningContext,
) -> Result<SigningResponse, ValueError> {
    if request_context.destination.is_none() {
        panic!("SigningContext.destination must be set; logic error in caller");
    }

    let context_value = get_context_value(type_values)?;
    let context = context_value
        .downcast_ref::<TuggerContextValue>()
        .ok_or(ValueError::IncorrectParameterType)?;

    // We can't hold the reference to the context due to re-entrancy. So get the
    // values we need from it and release.
    let signers = context.code_signers.clone();
    drop(context);

    let request = SigningRequest {
        action: request_context.action.as_str(),
        filename: format!("{}", request_context.filename.display()),
        path: request_context
            .path
            .as_ref()
            .map(|x| format!("{}", x.display())),
    };

    let mut response = SigningResponse {
        signers_count: signers.len(),
        ..Default::default()
    };

    info!("processing signing request {}", request);

    for (i, signer_raw) in signers.into_iter().enumerate() {
        response.signers_consulted += 1;
        debug!("consulting CodeSigner #{}", i);

        let signer_value = signer_raw
            .downcast_ref::<CodeSignerValue>()
            .ok_or(ValueError::IncorrectParameterType)?;

        let signer = signer_value.signer(request_context.label)?;

        if let Some(signable_signer) = error_context(request_context.label, || {
            Ok(signer.resolve_signer(request_context.candidate)?)
        })? {
            info!("CodeSigner #{} is capable of signing {}", i, request);

            // Call registered callback to give an opportunity to influence.
            // There is a potential for deadlock here, as we have the Signer lock held.
            // However, nothing we call into should acquire the lock, as there is no access
            // to the code signer, so we should be OK.
            let settings = if let Some(callback) = &signer_value.signing_callback {
                process_callback(type_values, call_stack, &request, callback)?
            } else {
                SigningSettings::default()
            };

            if settings.prevent_signing {
                response.prevented_index = Some(i);
                warn!(
                    "callback for CodeSigner #{} prevented signing of {}",
                    i, request
                );
                break;
            }

            if settings.defer {
                response.defer_count += 1;
                info!(
                    "callback for CodeSigner #{0} deferred signing of {}",
                    request
                );
                continue;
            }

            let destination = request_context
                .destination
                .unwrap_or_else(|| signable_signer.in_place_destination());

            warn!(
                "CodeSigner #{} attempting to sign {} to {}",
                i,
                request,
                match &destination {
                    SigningDestination::Memory => "memory".to_string(),
                    SigningDestination::File(p) => format!("{:?}", p),
                    SigningDestination::Directory(p) => format!("{:?}", p),
                }
            );

            // Skip actual code signing if we're in pretend mode. (This is meant for testing.)
            let output = if let Some(output) = request_context.pretend_output {
                Ok(output)
            } else {
                error_context(request_context.label, || {
                    // TODO specify temp dir as build directory.
                    Ok(signable_signer.sign(None, &destination)?)
                })
            }?;

            response.signed_index = Some(i);
            response.output = Some(output);
            break;
        } else {
            info!("CodeSigner isn't compatible with {}", request);
        }
    }

    Ok(response)
}

fn process_callback(
    type_values: &TypeValues,
    call_stack: &mut CallStack,
    original_request: &SigningRequest,
    func: &Value,
) -> Result<SigningSettings, ValueError> {
    let request_value = Value::new(CodeSigningRequestValue {
        inner: original_request.clone(),
        settings: SigningSettings::default(),
    });

    func.call(
        call_stack,
        type_values,
        vec![request_value.clone()],
        LinkedHashMap::new(),
        None,
        None,
    )?;

    // .unwrap() is safe because type couldn't have changed.
    let request = request_value
        .downcast_ref::<CodeSigningRequestValue>()
        .unwrap();

    Ok(request.settings.clone())
}

/// Process signability events on a [FileManifest].
///
/// This will iterate entries of a [FileManifest] and attempt to sign them.
///
/// Returns a new [FileManifest] holding possibly signed files.
pub fn handle_file_manifest_signable_events(
    type_values: &TypeValues,
    call_stack: &mut CallStack,
    manifest: &FileManifest,
    label: &'static str,
    action: SigningAction,
) -> Result<FileManifest> {
    let mut new_manifest = FileManifest::default();

    for (path, entry) in manifest.iter_entries() {
        let filename = path
            .file_name()
            .ok_or_else(|| anyhow!("could not resolve file name from FileManifest entry"))?;

        let candidate = entry
            .try_into()
            .context("converting FileManifest entry into signing candidate")?;
        let mut signing_context = SigningContext::new(label, action, filename, &candidate);
        signing_context.set_path(path);
        signing_context.set_signing_destination(SigningDestination::Memory);

        let response = handle_signable_event(type_values, call_stack, signing_context)
            .map_err(|e| anyhow!("{:?}", e))
            .context("handling Starlark signable event")?;

        let entry = if let Some(output) = response.output {
            if let SignedOutput::Memory(data) = output {
                FileEntry::new_from_data(data, entry.is_executable())
            } else {
                return Err(anyhow!("SignedOutput::Memory should have been forced"));
            }
        } else {
            entry.clone()
        };

        new_manifest
            .add_file_entry(path, entry)
            .context("adding entry to FileManifest")?;
    }

    Ok(new_manifest)
}

starlark_module! { code_signing_module =>
    code_signer_from_pfx_file(path: String, password: String) {
        CodeSignerValue::from_pfx_file(path, password)
    }

    code_signer_from_windows_store_sha1_thumbprint(thumbprint: String, store: String = "my".to_string()) {
        CodeSignerValue::from_windows_store_sha1_thumbprint(thumbprint, store)
    }

    code_signer_from_windows_store_subject(subject: String, store: String = "my".to_string()) {
        CodeSignerValue::from_windows_store_subject(subject, store)
    }

    code_signer_from_windows_store_auto() {
        CodeSignerValue::from_windows_store_auto()
    }

    CodeSigner.activate(env env, this) {
        let this = this.downcast_ref::<CodeSignerValue>().unwrap();
        this.activate(env)
    }

    CodeSigner.chain_issuer_certificates_pem_file(this, path: String) {
        let this = this.downcast_ref::<CodeSignerValue>().unwrap();
        this.chain_issuer_certificates_pem_file(path)
    }

    CodeSigner.chain_issuer_certificates_macos_keychain(this) {
        let this = this.downcast_ref::<CodeSignerValue>().unwrap();
        this.chain_issuer_certificates_macos_keychain()
    }

    CodeSigner.set_time_stamp_server(this, url: String) {
        let this = this.downcast_ref::<CodeSignerValue>().unwrap();
        this.set_time_stamp_server(url)
    }

    CodeSigner.set_signing_callback(this, func) {
        let mut this = this.downcast_mut::<CodeSignerValue>().unwrap().unwrap();
        this.set_signing_callback(func)
    }
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        crate::starlark::testutil::*,
        anyhow::Result,
        apple_codesign::CertificateProfile,
        tugger_code_signing::Signable,
        tugger_common::testutil::*,
        tugger_windows_codesign::{
            certificate_to_pfx, create_self_signed_code_signing_certificate,
        },
        x509_certificate::{EcdsaCurve, KeyAlgorithm},
    };

    struct TestSigningEventValue {
        label: &'static str,
        action: SigningAction,
        filename: PathBuf,
        path: Option<PathBuf>,
        candidate: SignableCandidate<'static>,
        response: Option<SigningResponse>,
    }

    impl Default for TestSigningEventValue {
        fn default() -> Self {
            Self {
                label: "default_label",
                action: SigningAction::Other("test"),
                filename: PathBuf::from("test_filename"),
                path: None,
                candidate: SignableCandidate::Forced(Signable::MachOData(vec![])),
                response: None,
            }
        }
    }

    impl TestSigningEventValue {
        fn run(&mut self, type_values: &TypeValues, call_stack: &mut CallStack) -> ValueResult {
            let mut context =
                SigningContext::new(self.label, self.action, &self.filename, &self.candidate);
            context.path = self.path.clone();
            context.set_pretend_output(SignedOutput::Memory(vec![42]));
            context.set_signing_destination(SigningDestination::Memory);

            let response = handle_signable_event(type_values, call_stack, context)?;

            self.response = Some(response);

            Ok(Value::new(NoneType::None))
        }
    }

    impl TypedValue for TestSigningEventValue {
        type Holder = Mutable<TestSigningEventValue>;
        const TYPE: &'static str = "TestSigningEvent";

        fn values_for_descendant_check_and_freeze(&self) -> Box<dyn Iterator<Item = Value>> {
            Box::new(std::iter::empty())
        }
    }

    starlark_module! { test_module =>
        TestSigningEvent.run(env env, call_stack cs, this) {
            let mut this = this.downcast_mut::<TestSigningEventValue>().unwrap().unwrap();
            this.run(env, cs)
        }
    }

    fn env_with_pfx_signer() -> Result<StarlarkEnvironment> {
        const PASSWORD: &str = "password123";

        let mut builder = tempfile::Builder::new();
        builder.prefix("certificate-");
        builder.suffix(".pfx");

        let pfx_file = builder.tempfile_in(DEFAULT_TEMP_DIR.path())?;
        // Normalize paths to work around string escaping.
        let pfx_path_str = format!("{}", pfx_file.path().display()).replace('\\', "/");

        let cert = create_self_signed_code_signing_certificate("test user")?;
        let pfx_data = certificate_to_pfx(&cert, PASSWORD, "name")?;
        std::fs::write(pfx_file.path(), &pfx_data)?;

        let mut env = StarlarkEnvironment::new()?;

        // Inject ability to invoke a test signing event.
        test_module(&mut env.env, &mut env.type_values);
        env.env
            .set(
                "SIGNING_EVENT",
                Value::new(TestSigningEventValue::default()),
            )
            .unwrap();

        env.eval(&format!(
            "signer = code_signer_from_pfx_file('{}', '{}')",
            pfx_path_str, PASSWORD
        ))?;

        Ok(env)
    }

    #[test]
    fn code_signer_from_pfx_file() -> Result<()> {
        let mut env = env_with_pfx_signer()?;

        let signer = env.eval("signer")?;
        assert_eq!(signer.get_type(), CodeSignerValue::TYPE);

        Ok(())
    }

    #[test]
    fn code_signer_from_windows_store_sha1_thumbprint() -> Result<()> {
        let mut env = StarlarkEnvironment::new()?;

        let signer = env.eval("code_signer_from_windows_store_sha1_thumbprint('1737477f1f3678b1da2695ab887c9af95cc95ebf')")?;
        assert_eq!(signer.get_type(), CodeSignerValue::TYPE);

        env.eval("code_signer_from_windows_store_sha1_thumbprint('1737477f1f3678b1da2695ab887c9af95cc95ebf', store = 'my')")?;
        env.eval("code_signer_from_windows_store_sha1_thumbprint('1737477f1f3678b1da2695ab887c9af95cc95ebf', store = 'root')")?;

        Ok(())
    }

    #[test]
    fn code_signer_from_windows_store_subject() -> Result<()> {
        let mut env = StarlarkEnvironment::new()?;

        let signer = env.eval("code_signer_from_windows_store_subject('test user')")?;
        assert_eq!(signer.get_type(), CodeSignerValue::TYPE);
        env.eval("code_signer_from_windows_store_subject('test user', store = 'my')")?;

        Ok(())
    }

    #[test]
    fn code_signer_from_windows_store_auto() -> Result<()> {
        let mut env = StarlarkEnvironment::new()?;

        let signer = env.eval("code_signer_from_windows_store_auto()")?;

        assert_eq!(signer.get_type(), CodeSignerValue::TYPE);

        Ok(())
    }

    #[test]
    fn chain_issuer_certificates_pem_file() -> Result<()> {
        let mut env = StarlarkEnvironment::new()?;

        // We obtain an X509 certificate by generating a key pair.
        let (cert, _, _) = apple_codesign::create_self_signed_code_signing_certificate(
            KeyAlgorithm::Ecdsa(EcdsaCurve::Secp256r1),
            CertificateProfile::AppleDevelopment,
            "teamid",
            "Joe Developer",
            "Wakanda",
            chrono::Duration::hours(1),
        )?;

        let pem_path = DEFAULT_TEMP_DIR
            .path()
            .join("chain_issuer_certificates_pem_file.pem");
        let pem_path_str = format!("{}", pem_path.display()).replace('\\', "/");

        let pem_data = cert.encode_pem();
        std::fs::write(&pem_path, &pem_data)?;

        env.eval("signer = code_signer_from_windows_store_auto()")?;
        env.eval(&format!(
            "signer.chain_issuer_certificates_pem_file('{}')",
            pem_path_str
        ))?;

        Ok(())
    }

    #[test]
    fn activate() -> Result<()> {
        let mut env = StarlarkEnvironment::new()?;

        {
            let context_value = get_context_value(&env.type_values).unwrap();
            let context = context_value.downcast_ref::<TuggerContextValue>().unwrap();
            assert_eq!(context.code_signers.len(), 0);
        }

        env.eval("signer = code_signer_from_windows_store_auto()")?;
        env.eval("signer.activate()")?;

        let context_value = get_context_value(&env.type_values).unwrap();
        let context = context_value.downcast_ref::<TuggerContextValue>().unwrap();
        assert_eq!(context.code_signers.len(), 1);

        Ok(())
    }

    #[test]
    fn set_signing_callback() -> Result<()> {
        let mut env = env_with_pfx_signer()?;

        env.eval("def callback(request):\n    return None\n")?;
        env.eval("signer.set_signing_callback(callback)")?;

        let signer_value = env.eval("signer")?;
        let signer = signer_value.downcast_ref::<CodeSignerValue>().unwrap();
        assert!(signer.signing_callback.is_some());

        Ok(())
    }

    #[test]
    fn multiple_signers() -> Result<()> {
        let mut env = env_with_pfx_signer()?;

        env.eval("def callback1(request):\n    pass\n")?;
        env.eval("def callback2(request):\n    pass\n")?;
        env.eval("signer.set_signing_callback(callback1)")?;
        env.eval("signer.activate()")?;
        env.eval("signer.set_signing_callback(callback2)")?;
        env.eval("signer.activate()")?;
        env.eval("SIGNING_EVENT.run()")?;

        let event_value = env.eval("SIGNING_EVENT")?;
        let event = event_value.downcast_ref::<TestSigningEventValue>().unwrap();

        assert!(event.response.is_some());
        let response = event.response.as_ref().unwrap();

        assert_eq!(response.signers_count, 2);

        Ok(())
    }

    #[test]
    fn callback_defer() -> Result<()> {
        let mut env = env_with_pfx_signer()?;

        env.eval("def callback1(request):\n    request.defer = True\n")?;
        env.eval("def callback2(request):\n    pass\n")?;
        env.eval("signer.set_signing_callback(callback1)")?;
        env.eval("signer.activate()")?;
        env.eval("signer.set_signing_callback(callback2)")?;
        env.eval("signer.activate()")?;
        env.eval("SIGNING_EVENT.run()")?;

        let event_value = env.eval("SIGNING_EVENT")?;
        let event = event_value.downcast_ref::<TestSigningEventValue>().unwrap();

        assert!(event.response.is_some());
        let response = event.response.as_ref().unwrap();

        assert_eq!(response.signers_count, 2);
        assert_eq!(response.signers_consulted, 2);
        assert_eq!(response.defer_count, 1);
        assert!(response.prevented_index.is_none());

        Ok(())
    }

    #[test]
    fn callback_prevent_signing() -> Result<()> {
        let mut env = env_with_pfx_signer()?;

        env.eval("def callback1(request):\n    request.prevent_signing = True\n")?;
        env.eval("def callback2(request):\n    pass\n")?;
        env.eval("signer.set_signing_callback(callback1)")?;
        env.eval("signer.activate()")?;
        env.eval("signer.set_signing_callback(callback2)")?;
        env.eval("signer.activate()")?;
        env.eval("SIGNING_EVENT.run()")?;

        let event_value = env.eval("SIGNING_EVENT")?;
        let event = event_value.downcast_ref::<TestSigningEventValue>().unwrap();

        assert!(event.response.is_some());
        let response = event.response.as_ref().unwrap();

        assert_eq!(response.signers_count, 2);
        assert_eq!(response.signers_consulted, 1);
        assert_eq!(response.prevented_index, Some(0));

        Ok(())
    }
}
