// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Functionality related to running Python interpreters. */

use std::path::PathBuf;

/// Defines the profile to use to configure a Python interpreter.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PythonInterpreterProfile {
    /// Python is isolated from the system.
    ///
    /// See https://docs.python.org/3/c-api/init_config.html#isolated-configuration.
    Isolated,

    /// Python interpreter behaves like `python`.
    ///
    /// See https://docs.python.org/3/c-api/init_config.html#python-configuration.
    Python,
}

impl Default for PythonInterpreterProfile {
    fn default() -> Self {
        PythonInterpreterProfile::Isolated
    }
}

/// Defines Python code to run.
#[derive(Clone, Debug, PartialEq)]
pub enum PythonRunMode {
    /// No-op.
    None,
    /// Run a Python REPL.
    Repl,
    /// Run a Python module as the main module.
    Module { module: String },
    /// Evaluate Python code from a string.
    Eval { code: String },
    /// Execute Python code in a file.
    ///
    /// We define this as a CString because the underlying API wants
    /// a char* and we want the constructor of this type to worry about
    /// the type coercion.
    File { path: PathBuf },
}

/// Defines `terminfo`` database resolution semantics.
#[derive(Clone, Debug, PartialEq)]
pub enum TerminfoResolution {
    /// Resolve `terminfo` database using appropriate behavior for current OS.
    Dynamic,
    /// Do not attempt to resolve the `terminfo` database. Basically a no-op.
    None,
    /// Use a specified string as the `TERMINFO_DIRS` value.
    Static(String),
}

/// Defines a backend for a memory allocator.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MemoryAllocatorBackend {
    /// The default system allocator.
    System,
    /// Use jemalloc.
    Jemalloc,
    /// Use Rust's global allocator.
    Rust,
}

/// Defines configuration for Python's raw allocator.
///
/// This allocator is what Python uses for all memory allocations.
///
/// See https://docs.python.org/3/c-api/memory.html for more.
#[derive(Clone, Copy, Debug)]
pub struct PythonRawAllocator {
    /// Which allocator backend to use.
    pub backend: MemoryAllocatorBackend,
    /// Whether memory debugging should be enabled.
    pub debug: bool,
}

impl PythonRawAllocator {
    pub fn system() -> Self {
        Self {
            backend: MemoryAllocatorBackend::System,
            ..PythonRawAllocator::default()
        }
    }

    pub fn jemalloc() -> Self {
        Self {
            backend: MemoryAllocatorBackend::Jemalloc,
            ..PythonRawAllocator::default()
        }
    }

    pub fn rust() -> Self {
        Self {
            backend: MemoryAllocatorBackend::Rust,
            ..PythonRawAllocator::default()
        }
    }
}

impl Default for PythonRawAllocator {
    fn default() -> Self {
        Self {
            backend: if cfg!(windows) {
                MemoryAllocatorBackend::System
            } else {
                MemoryAllocatorBackend::Jemalloc
            },
            debug: false,
        }
    }
}
