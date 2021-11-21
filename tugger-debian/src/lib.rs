// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Debian packaging primitives.

This crate defines pure Rust implementations of Debian packaging primitives.
*/

mod changelog;
mod control;
mod deb;
pub mod pgp;
pub mod repository;

pub use {
    changelog::{Changelog, ChangelogEntry},
    control::{
        ControlError, ControlField, ControlFieldValue, ControlFile, ControlParagraph, SourceControl,
    },
    deb::{write_deb_tar, ControlTarBuilder, DebBuilder, DebError},
};
