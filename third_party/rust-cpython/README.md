rust-cpython [![Build Status](https://travis-ci.org/dgrunwald/rust-cpython.svg?branch=master)](https://travis-ci.org/dgrunwald/rust-cpython)
====================

[Rust](http://www.rust-lang.org/) bindings for the [python](https://www.python.org/) interpreter.

* [Documentation](http://dgrunwald.github.io/rust-cpython/doc/cpython/)
* Cargo package: [cpython](https://crates.io/crates/cpython)

---

Copyright (c) 2015-2017 Daniel Grunwald.
Rust-cpython is licensed under the [MIT license](http://opensource.org/licenses/MIT).
Python is licensed under the [Python License](https://docs.python.org/2/license.html).

Supported Python versions:
* Python 2.7
* Python 3.3 to 3.7

Requires Rust 1.25.0 or later.

# Usage

To use `cpython`, add this to your `Cargo.toml`:

```toml
[dependencies]
cpython = "0.2"
```

#### Example program displaying the value of `sys.version`:

```rust
extern crate cpython;

use cpython::{Python, PyDict, PyResult};

fn main() {
    let gil = Python::acquire_gil();
    hello(gil.python()).unwrap();
}

fn hello(py: Python) -> PyResult<()> {
    let sys = py.import("sys")?;
    let version: String = sys.get(py, "version")?.extract(py)?;

    let locals = PyDict::new(py);
    locals.set_item(py, "os", py.import("os")?)?;
    let user: String = py.eval("os.getenv('USER') or os.getenv('USERNAME')", None, Some(&locals))?.extract(py)?;

    println!("Hello {}, I'm Python {}", user, version);
    Ok(())
}
```

#### Example library with python bindings:
The following two files will build with `cargo build`, and will generate a python-compatible library.
On Mac OS, you will need to rename the output from \*.dylib to \*.so.
On Windows, you will need to rename the output from \*.dll to \*.pyd.

###### Note:
At build time `python3-sys/build.rs` will look for interpreters in: 
* `PYTHON_SYS_EXECUTABLE`
* `python`
* `python3`

picking the first one that works and is compatible with the configured expected version (by default, any Python 3.X interpreter will do). If a specific interpreter is desired, the `PYTHON_SYS_EXECUTABLE` environment variable should point to it.

**`Cargo.toml`:**
```toml
[lib]
name = "rust2py"
crate-type = ["cdylib"]

[dependencies.cpython]
version = "0.2"
features = ["extension-module"]
```

**`src/lib.rs`**
```rust
#[macro_use] extern crate cpython;

use cpython::{PyResult, Python};

// add bindings to the generated python module
// N.B: names: "librust2py" must be the name of the `.so` or `.pyd` file
py_module_initializer!(librust2py, initlibrust2py, PyInit_librust2py, |py, m| {
    m.add(py, "__doc__", "This module is implemented in Rust.")?;
    m.add(py, "sum_as_string", py_fn!(py, sum_as_string_py(a: i64, b:i64)))?;
    Ok(())
});

// logic implemented as a normal rust function
fn sum_as_string(a:i64, b:i64) -> String {
    format!("{}", a + b).to_string()
}

// rust-cpython aware function. All of our python interface could be
// declared in a separate module.
// Note that the py_fn!() macro automatically converts the arguments from
// Python objects to Rust values; and the Rust return value back into a Python object.
fn sum_as_string_py(_: Python, a:i64, b:i64) -> PyResult<String> {
    let out = sum_as_string(a, b);
    Ok(out)
}
```

On windows and linux, you can build normally with cargo build --release. On Mac Os, you need to set additional linker arguments. The simplest solution is to create a `.cargo/config` with the following content:

```
[target.x86_64-apple-darwin]
rustflags = [
  "-C", "link-arg=-undefined",
  "-C", "link-arg=dynamic_lookup",
]
```

For `setup.py` integration, see https://github.com/fafhrd91/setuptools-rust
