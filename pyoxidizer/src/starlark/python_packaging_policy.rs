// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    python_packaging::policy::PythonPackagingPolicy,
    starlark::values::{Mutable, TypedValue, Value},
};

#[derive(Debug, Clone)]
pub struct PythonPackagingPolicyValue {
    pub inner: PythonPackagingPolicy,
}

impl TypedValue for PythonPackagingPolicyValue {
    type Holder = Mutable<PythonPackagingPolicyValue>;
    const TYPE: &'static str = "PythonPackagingPolicy";

    fn values_for_descendant_check_and_freeze(&self) -> Box<dyn Iterator<Item = Value>> {
        Box::new(std::iter::empty())
    }
}
