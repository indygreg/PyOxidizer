// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Functionality for the Snapcraft packaging format. */

mod builder;
mod yaml;

pub use {
    builder::SnapcraftBuilder,
    yaml::{
        Adapter, Architecture, Architectures, BuildAttribute, Confinement, Daemon, Grade,
        RestartCondition, SnapApp, SnapPart, Snapcraft, SourceType, Type,
    },
};
