# This file defines how PyOxidizer application building and packaging is
# performed. See the pyoxidizer crate's documentation for extensive
# documentation on this file format.

# Create an object that represents our installed application file layout.
files = FileManifest()

# Obtain the default PythonDistribution for our build target. We link
# this distribution into our produced executable and extract the Python
# standard library from it.
dist = default_python_distribution()

# Create a PythonEmbeddedResources instance from our Python distribution.
# This type represents the Python distribution and embedded Python resources
# (source modules, bytecode modules, and non-module resource files) to embed
# in a binary.
embedded = dist.to_embedded_resources(
    # Embed all extension modules, making this a fully-featured Python.
    extension_module_filter='all',

    # Only package the minimal set of extension modules needed to initialize
    # a Python interpreter. Many common packages in Python's standard
    # library won't work with this setting.
    #extension_module_filter='minimal',

    # Only package extension modules that don't require linking against
    # non-Python libraries. e.g. will exclude support for OpenSSL, SQLite3,
    # other features that require external libraries.
    #extension_module_filter='no-libraries',

    # Only package extension modules that don't link against GPL licensed
    # libraries.
    #extension_module_filter='no-gpl',

    # Include Python module sources. This isn't strictly required and it does
    # make binary sizes larger. But having the sources can be useful for
    # activities such as debugging.
    include_sources=True,

    # Whether to include non-module resource data/files.
    include_resources=False,

    # Do not include functionality for testing Python itself.
    include_test=False,
)

# Invoke `pip install` with our Python distribution to install a single package.
# `pip_install()` returns objects representing installed files.
# `add_python_resources()` adds these objects to our embedded context.
#embedded.add_python_resources(dist.pip_install(["appdirs"]))

# Invoke `pip install` using a requirements file and add the collected files
# to our embedded context.
#embedded.add_python_resources(dist.pip_install(["-r", "requirements.txt"]))

{{#each pip_install_simple}}
embedded.add_python_resources(dist.pip_install("{{{ this }}}"))
{{/each}}

# Read Python files from a local directory and add them to our embedded
# context, taking just the resources belonging to the `foo` and `bar`
# Python packages.
#embedded.add_python_resources(dist.read_package_root(
#    path="/src/mypackage",
#    packages=["foo", "bar"],
#)

# Discover Python files from a virtualenv and add them to our embedded
# context.
#embedded.add_python_resources(dist.read_virtualenv(path="/path/to/venv"))

# Filter all resources collected so far through a filter of names
# in a file.
#embedded.filter_from_files(files=["/path/to/filter-file"]))

# This variable defines the configuration of the
# embedded Python interpreter
embedded_python_config = EmbeddedPythonConfig(
#     bytes_warning=0,
#     dont_write_bytecode=True,
#     ignore_environment=True,
#     inspect=False,
#     interactive=False,
#     isolated=False,
#     legacy_windows_fs_encoding=False,
#     legacy_windows_stdio=False,
#     no_site=True,
#     no_user_site_directory=True,
#     optimize_level=0,
#     parser_debug=False,
#     stdio_encoding=None,
#     unbuffered_stdio=False,
#     filesystem_importer=False,
#     sys_frozen=False,
#     sys_meipass=False,
#     sys_paths=None,
#     raw_allocator=None,
#     terminfo_resolution="dynamic",
#     terminfo_dirs=None,
#     use_hash_seed=False,
#     verbose=0,
#     write_modules_directory_env=None,
)

# What the Python interpreter should run by default. This value can be
# ignored for projects not producing Python executables or customizing
# the Rust code that manages to embedded Python interpreter.

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

# Produce a Python executable from a Python distribution, embedded
# resources, and other options. The returned object represents the
# standalone executable that will be built.
exe = PythonExecutable(
    name="{{program_name}}",
    distribution=dist,
    resources=embedded,
    config=embedded_python_config,
    run_mode=python_run_mode,
)

# Add the generated executable to our install layout in the root directory.
# TODO this is busted
#files.add_python_resource(".", exe)

# Materialize the install layout in the destination directory.
#files.install()

# LEGACY CONTENT BELOW.

# This variable captures all packaging rules. Append to it to perform
# additional packaging at build time.
packaging_rules = []

# Write out license files next to the produced binary.
#packaging_rules.append(WriteLicenseFiles(""))

Config(
    application_name="{{program_name}}",
    embedded_python_config=embedded_python_config,
    python_distribution=dist,
    python_run_mode=python_run_mode,
)

# END OF COMMON USER-ADJUSTED SETTINGS.
#
# Everything below this is typically managed by PyOxidizer and doesn't need
# to be updated by people.

PYOXIDIZER_VERSION = "{{{ pyoxidizer_version }}}"
PYOXIDIZER_COMMIT = "{{{ pyoxidizer_commit }}}"
