// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use pyoxidizerlib::logging::logger_from_env;
use pyoxidizerlib::run_from_build;

fn main() {
    let logger_context = logger_from_env();

    run_from_build(&logger_context.logger, "build.rs");
}
