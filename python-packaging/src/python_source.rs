// Copyright 2022 Gregory Szorc.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

/*! Utility functions related to Python source code. */

use {anyhow::Result, once_cell::sync::Lazy};

static RE_CODING: Lazy<regex::bytes::Regex> = Lazy::new(|| {
    regex::bytes::Regex::new(r"^[ \t\f]*#.*?coding[:=][ \t]*([-_.a-zA-Z0-9]+)").unwrap()
});

/// Derive the source encoding from Python source code.
pub fn python_source_encoding(source: &[u8]) -> Vec<u8> {
    // Default source encoding is UTF-8. But per PEP 263, the first or second
    // line of source can match a regular expression to define a custom
    // encoding.
    let lines = source.split(|v| v == &b'\n');

    for (i, line) in lines.enumerate() {
        if i > 1 {
            break;
        }

        if let Some(m) = RE_CODING.find(line) {
            return m.as_bytes().to_vec();
        }
    }

    b"utf-8".to_vec()
}

/// Whether __file__ occurs in Python source code.
pub fn has_dunder_file(source: &[u8]) -> Result<bool> {
    // We can't just look for b"__file__ because the source file may be in
    // encodings like UTF-16. So we need to decode to Unicode first then look for
    // the code points.
    let encoding = python_source_encoding(source);

    let encoder = match encoding_rs::Encoding::for_label(&encoding) {
        Some(encoder) => encoder,
        None => encoding_rs::UTF_8,
    };

    let (source, ..) = encoder.decode(source);

    Ok(source.contains("__file__"))
}
