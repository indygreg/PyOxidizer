use pyembed::{MainPythonInterpreter, PythonConfig};

fn main() {
    // The following code is in a block so the MainPythonInterpreter is destroyed in an
    // orderly manner, before process exit.
    let code = {
        // Load the default Python configuration as derived by the PyOxidizer config
        // file used at build time.
        let config = PythonConfig::default();

        // Construct a new Python interpreter using that config.
        let mut interp = MainPythonInterpreter::new(config);

        // And run it using the default run configuration as specified by the
        // configuration. If an uncaught Python exception is raised, handle it.
        // This includes the special SystemExit, which is a request to terminate the
        // process.
        interp.run_as_main()
    };

    // And exit the process according to code execution results.
    std::process::exit(code);
}
