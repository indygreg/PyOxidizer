# This file defines how PyOxidizer application building and packaging is
# performed. See the pyoxidizer crate's documentation for extensive
# documentation on this file format.

# Obtain the default PythonDistribution for our build target. We link
# this distribution into our produced executable and extract the Python
# standard library from it.
def make_dist():
    return default_python_distribution()

# Configuration files consist of functions which define build "targets."
# This function creates a Python executable and installs it in a destination
# directory.
def make_exe(dist):
    # This function creates a `PythonPackagingPolicy` instance, which
    # influences how executables are built and how resources are added to
    # the executable. You can customize the default behavior by assigning
    # to attributes and calling functions.
    policy = dist.make_python_packaging_policy()

    # Package all available Python extensions in the distribution.
    # policy.extension_module_filter = "all"

    # Package the minimum set of Python extensions in the distribution needed
    # to run a Python interpreter. Various functionality from the Python
    # standard library won't work with this setting! But it can be used to
    # reduce the size of generated executables by omitting unused extensions.
    # policy.extension_module_filter = "minimal"

    # Package Python extensions in the distribution not having additional
    # library dependencies. This will exclude working support for SSL,
    # compression formats, and other functionality.
    # policy.extension_module_filter = "no-libraries"

    # Package Python extensions in the distribution not having a dependency on
    # GPL licensed software.
    # policy.extension_module_filter = "no-gpl"

    # Toggle whether Python module source code for modules in the Python
    # distribution's standard library are included.
    # policy.include_distribution_sources = False

    # Toggle whether Python package resource files for the Python standard
    # library are included.
    # policy.include_distribution_resources = False

    # Toggle whether files associated with tests are included.
    # policy.include_test = False

    # Resources are loaded from "in-memory" or "filesystem-relative" paths.
    # The locations to attempt to add resources to are defined by the
    # `resources_location` and `resources_location_fallback` attributes.
    # The former is the first/primary location to try and the latter is
    # an optional fallback.

    # Use in-memory location for adding resources by default.
    # policy.resources_location = "in-memory"

    # Use filesystem-relative location for adding resources by default.
    # policy.resources_location = "filesystem-relative:prefix"

    # Attempt to add resources relative to the built binary when
    # `resources_location` fails.
    # policy.resources_location_fallback = "filesystem-relative:prefix"

    # Clear out a fallback resource location.
    # policy.resources_location_fallback = None

    # Define a preferred Python extension module variant in the Python distribution
    # to use.
    # policy.set_preferred_extension_module_variant("foo", "bar")

    # This variable defines the configuration of the embedded Python
    # interpreter. By default, the interpreter will run a Python REPL
    # using settings that are appropriate for an "isolated" run-time
    # environment.
    #
    # The configuration of the embedded Python interpreter can be modified
    # by setting attributes on the instance. Some of these are
    # documented below.
    python_config = dist.make_python_interpreter_config()

    # Make the embedded interpreter behave like a `python` process.
    # python_config.config_profile = "python"

    # Set initial value for `sys.path`. If the string `$ORIGIN` exists in
    # a value, it will be expanded to the directory of the built executable.
    # python_config.module_search_paths = ["$ORIGIN/lib"]

    # Use jemalloc as Python's memory allocator
    # python_config.raw_allocator = "jemalloc"

    # Use the system allocator as Python's memory allocator.
    # python_config.raw_allocator = "system"

    # Control whether `oxidized_importer` is the first importer on
    # `sys.meta_path`.
    # python_config.oxidized_importer = False

    # Enable the standard path-based importer which attempts to load
    # modules from the filesystem.
    # python_config.filesystem_importer = True

    # Set `sys.frozen = True`
    # python_config.sys_frozen = True

    # Set `sys.meipass`
    # python_config.sys_meipass = True

    # Write files containing loaded modules to the directory specified
    # by the given environment variable.
    # python_config.write_modules_directory_env = "/tmp/oxidized/loaded_modules"

    # Don't run any Python code when the interpreter starts.
    # python_config.run_mode = 'none'

    # Start a Python REPL when the interpreter starts.
    # python_config.run_mode = 'repl'

    # Evaluate a string as Python code when the interpreter starts.
    # python_config.run_mode = 'eval:<code>'

    # Run a Python module as __main__ when the interpreter starts.
    # python_config.run_mode = 'module:foo.bar'

    # Produce a PythonExecutable from a Python distribution, embedded
    # resources, and other options. The returned object represents the
    # standalone executable that will be built.
    exe = dist.to_python_executable(
        name="{{program_name}}",

        # If no argument passed, the default `PythonPackagingPolicy` for the
        # distribution is used.
        packaging_policy=policy,

        # If no argument passed, the default `PythonInterpreterConfig` is used.
        config=python_config,
    )

    # Invoke `pip download` to install a single package using wheel archives
    # obtained via `pip download`. `pip_download()` returns objects representing
    # collected files inside Python wheels. `add_python_resources()` adds these
    # objects to the binary, with a load location as defined by the packaging
    # policy's resource location attributes.
    #exe.add_python_resources(exe.pip_download(["pyflakes==2.2.0"]))

    # Invoke `pip install` with our Python distribution to install a single package.
    # `pip_install()` returns objects representing installed files.
    # `add_python_resources()` adds these objects to the binary, with a load
    # location as defined by the packaging policy's resource location
    # attributes.
    #exe.add_python_resources(exe.pip_install(["appdirs"]))

    # Invoke `pip install` using a requirements file and add the collected resources
    # to our binary.
    #exe.add_python_resources(exe.pip_install(["-r", "requirements.txt"]))

    {{#each pip_install_simple}}
    exe.add_python_resources(exe.pip_install("{{{ this }}}"))
    {{/each}}

    # Read Python files from a local directory and add them to our embedded
    # context, taking just the resources belonging to the `foo` and `bar`
    # Python packages.
    #exe.add_python_resources(exe.read_package_root(
    #    path="/src/mypackage",
    #    packages=["foo", "bar"],
    #))

    # Discover Python files from a virtualenv and add them to our embedded
    # context.
    #exe.add_python_resources(exe.read_virtualenv(path="/path/to/venv"))

    # Filter all resources collected so far through a filter of names
    # in a file.
    #exe.filter_from_files(files=["/path/to/filter-file"]))

    # Return our `PythonExecutable` instance so it can be built and
    # referenced by other consumers of this target.
    return exe

def make_embedded_resources(exe):
    return exe.to_embedded_resources()

def make_install(exe):
    # Create an object that represents our installed application file layout.
    files = FileManifest()

    # Add the generated executable to our install layout in the root directory.
    files.add_python_resource(".", exe)

    return files

# Tell PyOxidizer about the build targets defined above.
register_target("dist", make_dist)
register_target("exe", make_exe, depends=["dist"])
register_target("resources", make_embedded_resources, depends=["exe"], default_build_script=True)
register_target("install", make_install, depends=["exe"], default=True)

# Resolve whatever targets the invoker of this configuration file is requesting
# be resolved.
resolve_targets()

# END OF COMMON USER-ADJUSTED SETTINGS.
#
# Everything below this is typically managed by PyOxidizer and doesn't need
# to be updated by people.

PYOXIDIZER_VERSION = "{{{ pyoxidizer_version }}}"
PYOXIDIZER_COMMIT = "{{{ pyoxidizer_commit }}}"
