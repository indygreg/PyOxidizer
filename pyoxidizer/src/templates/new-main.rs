use pyembed::MainPythonInterpreter;

// Include an auto-generated file containing the default
// `pyembed::PythonConfig` derived by the PyOxidizer configuration file.
//
// If you do not want to use PyOxidizer to generate this file, simply
// remove this line and instantiate your own instance of
// `pyembed::PythonConfig`.
include!(env!("PYOXIDIZER_DEFAULT_PYTHON_CONFIG_RS"));

fn main() {
    // The following code is in a block so the MainPythonInterpreter is destroyed in an
    // orderly manner, before process exit.
    let code = {
        // Load the default Python configuration as derived by the PyOxidizer config
        // file used at build time.
        let config = default_python_config();

        // Construct a new Python interpreter using that config, handling any errors
        // from construction.
        match MainPythonInterpreter::new(config) {
            Ok(mut interp) => {
                // And run it using the default run configuration as specified by the
                // configuration. If an uncaught Python exception is raised, handle it.
                // This includes the special SystemExit, which is a request to terminate the
                // process.
                interp.run_as_main()
            }
            Err(msg) => {
                eprintln!("{}", msg);
                1
            }
        }
    };

    // And exit the process according to code execution results.
    std::process::exit(code);
}
