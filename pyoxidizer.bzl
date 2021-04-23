# The following variables can be passed in via --var or --var-env to control
# behavior:
#
# CODE_SIGNING_SHA1_THUMBPRINT
#    Defines the SHA-1 thumbprint of the code signing certificate in the
#    Windows certificate store to use.
#
# CODE_SIGNING_PFX_PATH / CODE_SIGNING_PFX_PASSWORD
#    Path to code signing certificate PFX file and the password to use to
#    read it.
#
# TIME_STAMP_SERVER_URL
#    URL of Time-Stamp Protocol Server to use.
#
# CODE_SIGNING_APPLE_KEYCHAIN
#    Attempt to access the Apple keychain to load issuer certificates.

PYOXIDIZER_VERSION = "0.14.0"
AUTHOR = "Gregory Szorc"

# Whether we are running in CI.
IN_CI = VARS.get("IN_CI", False)
CODE_SIGNING_SHA1_THUMBPRINT = VARS.get("CODE_SIGNING_SHA1_THUMBPRINT")
CODE_SIGNING_PFX_PATH = VARS.get("CODE_SIGNING_PFX_PATH")
CODE_SIGNING_PFX_PASSWORD = VARS.get("CODE_SIGNING_PFX_PASSWORD")
CODE_SIGNING_APPLE_KEYCHAIN = VARS.get("CODE_SIGNING_APPLE_KEYCHAIN")
TIME_STAMP_SERVER_URL = VARS.get("TIME_STAMP_SERVER_URL", "http://timestamp.digicert.com")


def make_msi(target_triple, add_vc_redist):
    msi = WiXMSIBuilder(
        id_prefix = "pyoxidizer",
        product_name = "PyOxidizer",
        product_version = PYOXIDIZER_VERSION,
        product_manufacturer = AUTHOR,
    )
    msi.help_url = "https://pyoxidizer.readthedocs.io/"
    msi.target_triple = target_triple
    msi.license_path = CWD + "/LICENSE"

    if target_triple == "i686-pc-windows-msvc":
        platform = "x86"
    elif target_triple == "x86_64-pc-windows-msvc":
        platform = "x64"
    else:
        platform = "unknown"

    msi.msi_filename = "pyoxidizer-" + PYOXIDIZER_VERSION + "-" + platform + ".msi"

    if add_vc_redist:
        msi.add_visual_cpp_redistributable(redist_version = "14", platform = platform)

    m = FileManifest()

    if IN_CI:
        exe_prefix = "dist/" + target_triple + "/"
    else:
        exe_prefix = "target/" + target_triple + "/release/"

    m.add_path(
        path = exe_prefix + "pyoxidizer.exe",
        strip_prefix = exe_prefix,
    )

    msi.add_program_files_manifest(m)

    return msi


def make_msi_x86():
    return make_msi("i686-pc-windows-msvc", True)


def make_msi_x86_64():
    return make_msi("x86_64-pc-windows-msvc", True)


def make_exe_installer():
    bundle = WiXBundleBuilder(
        id_prefix = "pyoxidizer",
        name = "PyOxidizer",
        version = PYOXIDIZER_VERSION,
        manufacturer = AUTHOR,
    )

    bundle.add_vc_redistributable("x86")
    bundle.add_vc_redistributable("x64")

    bundle.add_wix_msi_builder(
        builder = make_msi("i686-pc-windows-msvc", False),
        display_internal_ui = True,
        install_condition = "Not VersionNT64",
    )
    bundle.add_wix_msi_builder(
        builder = make_msi("x86_64-pc-windows-msvc", False),
        display_internal_ui = True,
        install_condition = "VersionNT64",
    )

    return bundle


def make_macos_app_bundle():
    bundle = MacOsApplicationBundleBuilder("PyOxidizer")
    bundle.set_info_plist_required_keys(
        display_name = "PyOxidizer",
        identifier = "com.gregoryszorc.pyox",
        version = PYOXIDIZER_VERSION,
        signature = "pyox",
        executable = "pyoxidizer",
    )

    m = FileManifest()
    m.add_path(
        path = "target/release/pyoxidizer",
        strip_prefix = "target/release/",
    )
    bundle.add_macos_manifest(m)

    return bundle


def register_code_signers():
    signer = None

    if CODE_SIGNING_SHA1_THUMBPRINT:
        print("registering Windows store signer from SHA-1 thumbprint")
        signer = code_signer_from_windows_store_sha1_thumbprint(CODE_SIGNING_SHA1_THUMBPRINT)

    if CODE_SIGNING_PFX_PATH:
        if CODE_SIGNING_PFX_PASSWORD:
            password = CODE_SIGNING_PFX_PASSWORD
        else:
            password = ""

        print("registering code signer from PFX file")
        signer = code_signer_from_pfx_file(CODE_SIGNING_PFX_PATH, password)

    if signer:
        if CODE_SIGNING_APPLE_KEYCHAIN:
            signer.chain_issuer_certificates_macos_keychain()
        else:
            # Apple time server will be used automatically on Apple.
            signer.set_time_stamp_server(TIME_STAMP_SERVER_URL)
        signer.activate()



register_code_signers()

register_target("msi_x86", make_msi_x86)
register_target("msi_x86_64", make_msi_x86_64)
register_target("exe_installer", make_exe_installer, default = True)
register_target("macos_app_bundle", make_macos_app_bundle)

resolve_targets()
