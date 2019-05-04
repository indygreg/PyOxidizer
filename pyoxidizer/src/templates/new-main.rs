use pyembed::{MainPythonInterpreter, PythonConfig};

fn main() {
    // Load the default Python configuration as derived by the PyOxidizer config
    // file used at build time.
    let config = PythonConfig::default();

    // Construct a new Python interpreter using that config.
    let mut interp = MainPythonInterpreter::new(config);

    // And run it using the default run configuration as specified by the
    // configuration. If an uncaught Python exception is raised, handle it.
    // In the case of the error being SystemExit, the process will exit and
    // any code following won't run.
    match interp.run() {
        Ok(_) => {}
        Err(err) => interp.print_err(err),
    }
}
