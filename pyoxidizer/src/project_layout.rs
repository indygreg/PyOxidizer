// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Handle file layout of PyOxidizer projects.

use handlebars::Handlebars;
use lazy_static::lazy_static;
use std::collections::BTreeMap;

lazy_static! {
    pub static ref PYEMBED_RS_FILES: BTreeMap<&'static str, &'static [u8]> = {
        let mut res: BTreeMap<&'static str, &'static [u8]> = BTreeMap::new();

        res.insert("config.rs", include_bytes!("pyembed/config.rs"));
        res.insert("lib.rs", include_bytes!("pyembed/lib.rs"));
        res.insert("data.rs", include_bytes!("pyembed/data.rs"));
        res.insert("importer.rs", include_bytes!("pyembed/importer.rs"));
        res.insert("osutils.rs", include_bytes!("pyembed/osutils.rs"));
        res.insert("pyalloc.rs", include_bytes!("pyembed/pyalloc.rs"));
        res.insert("pyinterp.rs", include_bytes!("pyembed/pyinterp.rs"));
        res.insert("pystr.rs", include_bytes!("pyembed/pystr.rs"));

        res
    };
    pub static ref HANDLEBARS: Handlebars = {
        let mut handlebars = Handlebars::new();

        handlebars
            .register_template_string("new-main.rs", include_str!("templates/new-main.rs"))
            .unwrap();
        handlebars
            .register_template_string(
                "new-pyoxidizer.bzl",
                include_str!("templates/new-pyoxidizer.bzl"),
            )
            .unwrap();
        handlebars
            .register_template_string(
                "pyembed-build.rs",
                include_str!("templates/pyembed-build.rs"),
            )
            .unwrap();
        handlebars
            .register_template_string(
                "pyembed-cargo.toml",
                include_str!("templates/pyembed-cargo.toml"),
            )
            .unwrap();

        handlebars
    };
}
