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

    # Python resources are to be loaded from memory only.
    # policy.resources_policy = "in-memory-only"

    # Python resources are to be loaded from the filesystem, from a
    # directory relative to the produced binary. (The directory name
    # follows the colon. Use "." to denote the same directory as the
    # binary.) In order to import Python modules from the filesystem,
    # you will need to define `sys_paths` on the `PythonInterpreterConfig`
    # instance so the Python interpreter is configured to locate resources
    # in said path.
    # policy.resources_policy = "filesystem-relative-only:<prefix>"

    # Python resources are loaded from memory if memory loading is supported
    # and from the filesystem if they are not. This is a hybrid of
    # `in-memory-only` and `filesystem-relative-only:<prefix>`. See the
    # "Managing Resources and Their Locations" packaging documentation for
    # more on behavior.
    # policy.resources_policy = "prefer-in-memory-fallback-filesystem-relative:<prefix>"

    # Define a preferred Python extension module variant in the Python distribution
    # to use.
    # policy.set_preferred_extension_module_variant("foo", "bar")

    # This variable defines the configuration of the
    # embedded Python interpreter.
    python_config = PythonInterpreterConfig(
    #     bytes_warning=0,
    #     write_bytecode=False,
    #     ignore_environment=True,
    #     inspect=False,
    #     interactive=False,
    #     isolated=True,
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
    #     run_eval={{#if code}}(r"""{{{code}}}"""{{else}}None{{/if}},
    #     run_module=None,
    #     run_noop=False,
    #     run_repl={{#if code}}False{{else}}True{{/if}},
    )

    # The run_eval, run_module, run_noop, and run_repl arguments are mutually
    # exclusive controls over what the interpreter should do once it initializes.
    #
    # run_eval -- Run the specified string value via `eval()`.
    # run_module -- Import the specified module as __main__ and run it.
    # run_noop -- Do nothing.
    # run_repl -- Start a Python REPL.
    #
    # These arguments can be ignored if you are providing your own Rust code for
    # starting the interpreter, as Rust code has full control over interpreter
    # behavior.

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
    # objects to the binary, with a load location as defined by the current
    # `resources_policy`.
    #exe.add_python_resources(exe.pip_download(["pyflakes==2.2.0"]))

    # Invoke `pip install` with our Python distribution to install a single package.
    # `pip_install()` returns objects representing installed files.
    # `add_python_resources()` adds these objects to the binary, with a load
    # location as defined by the current `resources_policy`.
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
register_target("exe", make_exe, depends=["dist"], default=True)
register_target("resources", make_embedded_resources, depends=["exe"], default_build_script=True)
register_target("install", make_install, depends=["exe"])

# Resolve whatever targets the invoker of this configuration file is requesting
# be resolved.
resolve_targets()

# END OF COMMON USER-ADJUSTED SETTINGS.
#
# Everything below this is typically managed by PyOxidizer and doesn't need
# to be updated by people.

PYOXIDIZER_VERSION = "{{{ pyoxidizer_version }}}"
PYOXIDIZER_COMMIT = "{{{ pyoxidizer_commit }}}"
