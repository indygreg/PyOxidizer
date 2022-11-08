# The following variables can be passed in via --var or --var-env to control
# behavior:
#
# CODE_SIGNING_ENABLE
#    Whether to enable code signing.
#
#    If present, will prompt for code signing parameters.
#
# TIME_STAMP_SERVER_URL
#    URL of Time-Stamp Protocol Server to use.

PYOXIDIZER_VERSION = "0.24.0"
AUTHOR = "Gregory Szorc"

# Whether we are running in CI.
IN_CI = VARS.get("IN_CI", False)
CODE_SIGNING_ENABLE = VARS.get("CODE_SIGNING_ENABLE", False)
TIME_STAMP_SERVER_URL = VARS.get(
    "TIME_STAMP_SERVER_URL", "http://timestamp.digicert.com"
)

WHEEL_METADATA = (
    """Metadata-Version: 2.1
Name: pyoxidizer
Version: %s
Summary: Package self-contained Python applications
Keywords: python
Home-Page: https://github.com/indygreg/PyOxidizer
Author: Gregory Szorc <gregory.szorc@gmail.com>
Author-Email: Gregory Szorc <gregory.szorc@gmail.com>
License: MPL-2.0
Description-Content-Type: text/markdown; charset=UTF-8; variant=GFM

PyOxidizer is a utility for producing distributable Python applications.

See the docs at https://pyoxidizer.readthedocs.org/ for more.
"""
    % PYOXIDIZER_VERSION
)


def make_msi(target_triple, add_vc_redist):
    if target_triple == "i686-pc-windows-msvc":
        arch = "x86"
    elif target_triple == "x86_64-pc-windows-msvc":
        arch = "x64"
    else:
        arch = "unknown"

    msi = WiXMSIBuilder(
        id_prefix="pyoxidizer",
        product_name="PyOxidizer",
        product_version=PYOXIDIZER_VERSION,
        product_manufacturer=AUTHOR,
        arch=arch,
    )
    msi.help_url = "https://gregoryszorc.com/docs/pyoxidizer/stable/"
    msi.license_path = CWD + "/LICENSE"

    msi.msi_filename = "PyOxidizer-" + PYOXIDIZER_VERSION + "-" + arch + ".msi"

    if add_vc_redist:
        msi.add_visual_cpp_redistributable(redist_version="14", platform=arch)

    m = FileManifest()

    if IN_CI:
        exe_prefix = "dist/" + target_triple + "/"
    else:
        exe_prefix = "target/" + target_triple + "/release/"

    m.add_path(
        path=exe_prefix + "pyoxidizer.exe",
        strip_prefix=exe_prefix,
    )

    msi.add_program_files_manifest(m)

    return msi


def make_msi_x86():
    return make_msi("i686-pc-windows-msvc", True)


def make_msi_x86_64():
    return make_msi("x86_64-pc-windows-msvc", True)


def make_exe_installer():
    bundle = WiXBundleBuilder(
        id_prefix="pyoxidizer",
        name="PyOxidizer",
        version=PYOXIDIZER_VERSION,
        manufacturer=AUTHOR,
    )

    bundle.add_vc_redistributable("x86")
    bundle.add_vc_redistributable("x64")

    bundle.add_wix_msi_builder(
        builder=make_msi("i686-pc-windows-msvc", False),
        display_internal_ui=True,
        install_condition="Not VersionNT64",
    )
    bundle.add_wix_msi_builder(
        builder=make_msi("x86_64-pc-windows-msvc", False),
        display_internal_ui=True,
        install_condition="VersionNT64",
    )

    return bundle


def make_macos_app_bundle():
    ARCHES = ["aarch64-apple-darwin", "x86_64-apple-darwin"]

    bundle = MacOsApplicationBundleBuilder("PyOxidizer")
    bundle.set_info_plist_required_keys(
        display_name="PyOxidizer",
        identifier="com.gregoryszorc.pyox",
        version=PYOXIDIZER_VERSION,
        signature="pyox",
        executable="pyoxidizer",
    )

    universal = AppleUniversalBinary("pyoxidizer")

    for arch in ARCHES:
        if IN_CI:
            path = "dist/" + arch + "/pyoxidizer"
        else:
            path = "target/" + arch + "/release/pyoxidizer"

        universal.add_path(path)

    m = FileManifest()
    m.add_file(universal.to_file_content())
    bundle.add_macos_manifest(m)

    return bundle


def make_wheel(platform_tag, target_triple):
    # PyOxidizer's wheel is simply the executable binary as a "script" file.
    wheel = PythonWheelBuilder("pyoxidizer", PYOXIDIZER_VERSION)
    wheel.generator = "PyOxidizer (%s)" % PYOXIDIZER_VERSION
    wheel.tag = "py3-none-%s" % platform_tag

    wheel.add_file_dist_info(FileContent(filename="METADATA", content=WHEEL_METADATA))

    path = "target/%s/release/pyoxidizer" % target_triple

    if "-windows-" in target_triple:
        path = "%s.exe" % path

    wheel.add_file_data("scripts", FileContent(path=path, executable=True))

    return wheel


def make_wheel_linux_aarch64():
    return make_wheel("manylinux2014_aarch64", "aarch64-unknown-linux-musl")


def make_wheel_linux_x86_64():
    return make_wheel("manylinux2010_x86_64", "x86_64-unknown-linux-musl")


def make_wheel_macos_aarch64():
    return make_wheel("macosx_11_0_arm64", "aarch64-apple-darwin")


def make_wheel_macos_x86_64():
    return make_wheel("macosx_10_9_x86_64", "x86_64-apple-darwin")


def make_wheel_windows_x86():
    return make_wheel("win32", "i686-pc-windows-msvc")


def make_wheel_windows_x86_64():
    return make_wheel("win_amd64", "x86_64-pc-windows-msvc")


def register_code_signers():
    signer = None

    if CODE_SIGNING_ENABLE:
        pfx_path = prompt_input("path to PFX file")
        pfx_password = prompt_password("PFX password", confirm=True)

        signer = code_signer_from_pfx_file(pfx_path, pfx_password)
        signer.activate()


register_code_signers()

register_target("msi_x86", make_msi_x86)
register_target("msi_x86_64", make_msi_x86_64)
register_target("exe_installer", make_exe_installer, default=True)
register_target("macos_app_bundle", make_macos_app_bundle)
register_target("wheel_aarch64-unknown-linux-musl", make_wheel_linux_aarch64)
register_target("wheel_x86_64-unknown-linux-musl", make_wheel_linux_x86_64)
register_target("wheel_aarch64-apple-darwin", make_wheel_macos_aarch64)
register_target("wheel_x86_64-apple-darwin", make_wheel_macos_x86_64)
register_target("wheel_i686-pc-windows-msvc", make_wheel_windows_x86)
register_target("wheel_x86_64-pc-windows-msvc", make_wheel_windows_x86_64)

resolve_targets()
