// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use anyhow::Result;

pub mod binary;
pub mod cli;
pub mod db;
pub mod import;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    cli::run().await
}
