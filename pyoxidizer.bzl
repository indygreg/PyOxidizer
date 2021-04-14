PYOXIDIZER_VERSION = "0.13.0"

def make_msi(target_triple):
    msi = WiXMSIBuilder(
        id_prefix = "pyoxidizer",
        product_name = "PyOxidizer",
        product_version = PYOXIDIZER_VERSION,
        product_manufacturer = "Gregory Szorc",
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
    return make_msi("i686-pc-windows-msvc")


def make_msi_x86_64():
    return make_msi("x86_64-pc-windows-msvc")


register_target("msi_x86", make_msi_x86)
register_target("msi_x86_64", make_msi_x86_64, default = True)

resolve_targets()
