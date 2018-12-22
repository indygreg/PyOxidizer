const CFG_KEY: &'static str = "py_sys_config";

#[cfg(feature="python27-sys")]
const PYTHONSYS_ENV_VAR: &'static str = "DEP_PYTHON27_PYTHON_FLAGS";

#[cfg(feature="python3-sys")]
const PYTHONSYS_ENV_VAR: &'static str = "DEP_PYTHON3_PYTHON_FLAGS";

fn main() {
    // python{27,3.x}-sys/build.rs passes python interpreter compile flags via 
    // environment variable (using the 'links' mechanism in the cargo.toml).
    let flags = "";

    if flags.len() > 0 {
        for f in flags.split(",") {
            // write out flags as --cfg so that the same #cfg blocks can be used
            // in rust-cpython as in the -sys libs
            let key_and_val: Vec<&str> = f.split("=").collect();
            let key = key_and_val[0];
            let val = key_and_val[1];
            if key.starts_with("FLAG") {
                println!("cargo:rustc-cfg={}=\"{}\"", CFG_KEY, &key[5..])
            } else {
                println!("cargo:rustc-cfg={}=\"{}_{}\"", CFG_KEY, &key[4..], val);
            }
        }
    }
}
