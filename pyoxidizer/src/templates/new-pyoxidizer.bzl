# This file controls the PyOxidizer build configuration. See the
# pyoxidizer crate's documentation for extensive documentation
# on this file format.

# BuildConfig(application_name, build_path=None)
#     """Defines the application build configuration."""
build_config = BuildConfig(application_name="{{program_name}}")

# EmbeddedPythonConfig(
#     dont_write_bytecode=True,
#     ignore_environment=True,
#     no_site=True,
#     no_user_site_directory=True,
#     optimize_level=0,
#     stdio_encoding=None,
#     unbuffered_stdio=False,
#     filesystem_importer=False,
#     sys_frozen=False,
#     sys_meipass=False,
#     sys_paths=None,
#     raw_allocator=None,
#     terminfo_resolution="dynamic",
#     terminfo_dirs=None,
#     write_modules_directory_env=None,
# )
#     """Defines the configuration of the embedded Python interpreter."""

embedded_python_config = EmbeddedPythonConfig()

# This variable captures all packaging rules. Append to it to perform
# additional packaging at build time.
packaging_rules = []

# Package all available extension modules from the Python distribution.
# The Python interpreter will be fully featured.
packaging_rules.append(StdlibExtensionsPolicy("all"))

# Only package the minimal set of extension modules needed to initialize
# a Python interpreter. Many common packages in Python's standard
# library won't work with this setting.
#packaging_rules.append(StdlibExtensionsPolicy("minimal"))

# Only package extension modules that don't require linking against
# non-Python libraries. e.g. will exclude support for OpenSSL, SQLite3,
# other features that require external libraries.
#packaging_rules.append(StdlibExtensionsPolicy("no-libraries"))

# Package the entire Python standard library without sources.
packaging_rules.append(Stdlib(include_source=False))

# Explicit list of extension modules from the distribution to include.
#packaging_rules.append(StdlibExtensionsExplicitIncludes([
#    "binascii", "errno", "itertools", "math", "select", "_socket"
#]))

# Explicit list of extension modules from the distribution to exclude.
#packaging_rules.append(StdlibExtensionsExplicitExcludes(["_ssl"]))

# Write out license files next to the produced binary.
packaging_rules.append(WriteLicenseFiles(""))

{{#each pip_install_simple}}
packaging_rules.append(PipInstallSimple("{{{ this }}}"))
{{/each}}

# Package .py files discovered in a local directory.
#packaging_rules.append(PackageRoot(
#    path="/src/mypackage", packages=["foo", "bar"],
#))

# Package things from a populated virtualenv.
#packaging_rules.append(Virtualenv(path="/path/to/venv"))

# Filter all resources collected so far through a filter of names
# in a file.
#packaging_rules.append(FilterInclude(files=["/path/to/filter-file"]))

# How Python should run by default. This is only needed if you
# call ``run()``. For applications customizing how the embedded
# Python interpreter is invoked, this section is not relevant.

# Run an interactive Python interpreter.
{{#unless code~}}
python_run_mode = python_run_mode_repl()
{{~else~}}
# python_run_mode = python_run_mode_repl()
{{~/unless}}

# Import a Python module and run it.
# python_run_mode = python_run_mode_module("mypackage.__main__")

# Evaluate some Python code.
{{#if code~}}
python_run_mode = python_run_mode_eval(r"""{{{code}}}""")
{{~else~}}
#python_run_mode = python_run_mode_eval("from mypackage import main; main()")
{{~/if}}

Config(
    build_config=build_config,
    embedded_python_config=embedded_python_config,
    python_distribution=default_python_distribution(),
    python_run_mode=python_run_mode,
    packaging_rules=packaging_rules,
)

# END OF COMMON USER-ADJUSTED SETTINGS.
#
# Everything below this is typically managed by PyOxidizer and doesn't need
# to be updated by people.

PYOXIDIZER_VERSION = "{{{ pyoxidizer_version }}}"
PYOXIDIZER_COMMIT = "{{{ pyoxidizer_commit }}}"
