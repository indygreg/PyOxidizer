// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    starlark::values::{Immutable, TypedValue, Value},
    tugger_file_manifest::FileEntry,
};

// TODO merge this into `FileValue`?
#[derive(Clone, Debug)]
pub struct FileContentValue {
    pub content: FileEntry,
}

impl TypedValue for FileContentValue {
    type Holder = Immutable<FileContentValue>;
    const TYPE: &'static str = "FileContent";

    fn values_for_descendant_check_and_freeze(&self) -> Box<dyn Iterator<Item = Value>> {
        Box::new(std::iter::empty())
    }
}
