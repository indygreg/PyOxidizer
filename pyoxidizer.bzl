PYOXIDIZER_VERSION = "0.14.0"
AUTHOR = "Gregory Szorc"

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


register_target("msi_x86", make_msi_x86)
register_target("msi_x86_64", make_msi_x86_64)
register_target("exe_installer", make_exe_installer, default = True)
register_target("macos_app_bundle", make_macos_app_bundle)

resolve_targets()
