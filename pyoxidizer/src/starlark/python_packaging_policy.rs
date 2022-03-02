// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    super::python_resource::ResourceCollectionContext,
    linked_hash_map::LinkedHashMap,
    python_packaging::{
        location::ConcreteResourceLocation,
        policy::{ExtensionModuleFilter, PythonPackagingPolicy, ResourceHandlingMode},
    },
    starlark::{
        environment::TypeValues,
        eval::call_stack::CallStack,
        starlark_fun, starlark_module, starlark_parse_param_type, starlark_signature,
        starlark_signature_extraction, starlark_signatures,
        values::{
            error::{RuntimeError, UnsupportedOperation, ValueError},
            none::NoneType,
            Mutable, TypedValue, Value, ValueResult,
        },
    },
    starlark_dialect_build_targets::required_type_arg,
    std::{
        ops::Deref,
        sync::{Arc, Mutex, MutexGuard},
    },
};

#[derive(Debug, Clone)]
pub struct PythonPackagingPolicyValue {
    inner: Arc<Mutex<PythonPackagingPolicy>>,

    /// Starlark functions to influence PythonResourceAddCollectionContext creation.
    derive_context_callbacks: Vec<Value>,
}

impl PythonPackagingPolicyValue {
    pub fn new(inner: PythonPackagingPolicy) -> Self {
        Self {
            inner: Arc::new(Mutex::new(inner)),
            derive_context_callbacks: vec![],
        }
    }

    pub fn inner(&self, label: &str) -> Result<MutexGuard<PythonPackagingPolicy>, ValueError> {
        self.inner.try_lock().map_err(|e| {
            ValueError::Runtime(RuntimeError {
                code: "PYTHON_PACKAGING_POLICY",
                message: format!("unable to obtain lock: {}", e),
                label: label.to_string(),
            })
        })
    }

    /// Apply this policy to a resource.
    ///
    /// This has the effect of replacing the `PythonResourceAddCollectionContext`
    /// instance with a fresh one derived from the policy. If no context is
    /// currently defined on the resource, a new one will be created so there is.
    pub fn apply_to_resource<T>(
        &self,
        label: &str,
        type_values: &TypeValues,
        call_stack: &mut CallStack,
        value: &mut T,
    ) -> ValueResult
    where
        T: TypedValue + ResourceCollectionContext + Clone,
    {
        let new_context = self
            .inner(label)?
            .derive_add_collection_context(&value.as_python_resource()?);
        value.replace_add_collection_context(new_context)?;

        for func in &self.derive_context_callbacks {
            // This is a bit wonky. We pass in a `TypeValue`, which isn't a `Value`.
            // To go from `TypeValue` to `Value`, we need to construct a `Value`, which
            // takes ownership of the `TypeValue`. But we need to move a `Value` as an
            // argument into call().
            //
            // Our solution for this is to create a copy of the passed object and
            // construct a `Value` from it. After the call, we downcast it back to
            // our T, retrieve its add context, and replace that on the original value.
            //
            // There might be a way to pass a `Value` into this method. But for now,
            // this solution works.
            let temp_value = Value::new(value.clone());

            func.call(
                call_stack,
                type_values,
                vec![Value::new(self.clone()), temp_value.clone()],
                LinkedHashMap::new(),
                None,
                None,
            )?;

            let downcast_value = temp_value.downcast_ref::<T>().unwrap();
            let inner: &T = downcast_value.deref();
            value.replace_add_collection_context(inner.add_collection_context()?.unwrap())?;
        }

        Ok(Value::from(NoneType::None))
    }
}

impl TypedValue for PythonPackagingPolicyValue {
    type Holder = Mutable<PythonPackagingPolicyValue>;
    const TYPE: &'static str = "PythonPackagingPolicy";

    fn values_for_descendant_check_and_freeze<'a>(
        &'a self,
    ) -> Box<dyn Iterator<Item = Value> + 'a> {
        Box::new(self.derive_context_callbacks.iter().cloned())
    }

    fn get_attr(&self, attribute: &str) -> ValueResult {
        let inner = self.inner(&format!("PythonPackagingPolicy.{}", attribute))?;

        let v = match attribute {
            "allow_files" => Value::from(inner.allow_files()),
            "allow_in_memory_shared_library_loading" => {
                Value::from(inner.allow_in_memory_shared_library_loading())
            }
            "bytecode_optimize_level_zero" => Value::from(inner.bytecode_optimize_level_zero()),
            "bytecode_optimize_level_one" => Value::from(inner.bytecode_optimize_level_one()),
            "bytecode_optimize_level_two" => Value::from(inner.bytecode_optimize_level_two()),
            "extension_module_filter" => Value::from(inner.extension_module_filter().as_ref()),
            "file_scanner_classify_files" => Value::from(inner.file_scanner_classify_files()),
            "file_scanner_emit_files" => Value::from(inner.file_scanner_emit_files()),
            "include_distribution_sources" => Value::from(inner.include_distribution_sources()),
            "include_distribution_resources" => Value::from(inner.include_distribution_resources()),
            "include_classified_resources" => Value::from(inner.include_classified_resources()),
            "include_file_resources" => Value::from(inner.include_file_resources()),
            "include_non_distribution_sources" => {
                Value::from(inner.include_non_distribution_sources())
            }
            "include_test" => Value::from(inner.include_test()),
            "preferred_extension_module_variants" => {
                Value::try_from(inner.preferred_extension_module_variants().clone())?
            }
            "resources_location" => Value::from(inner.resources_location().to_string()),
            "resources_location_fallback" => match inner.resources_location_fallback() {
                Some(location) => Value::from(location.to_string()),
                None => Value::from(NoneType::None),
            },
            attr => {
                return Err(ValueError::OperationNotSupported {
                    op: UnsupportedOperation::GetAttr(attr.to_string()),
                    left: "PythonPackagingPolicy".to_string(),
                    right: None,
                })
            }
        };

        Ok(v)
    }

    fn has_attr(&self, attribute: &str) -> Result<bool, ValueError> {
        Ok(matches!(
            attribute,
            "allow_files"
                | "allow_in_memory_shared_library_loading"
                | "bytecode_optimize_level_zero"
                | "bytecode_optimize_level_one"
                | "bytecode_optimize_level_two"
                | "extension_module_filter"
                | "file_scanner_classify_files"
                | "file_scanner_emit_files"
                | "include_distribution_sources"
                | "include_distribution_resources"
                | "include_classified_resources"
                | "include_file_resources"
                | "include_non_distribution_sources"
                | "include_test"
                | "preferred_extension_module_variants"
                | "resources_location"
                | "resources_location_fallback"
        ))
    }

    fn set_attr(&mut self, attribute: &str, value: Value) -> Result<(), ValueError> {
        let mut inner = self.inner(&format!("PythonPackagingPolicy.{}", attribute))?;

        match attribute {
            "allow_files" => {
                inner.set_allow_files(value.to_bool());
            }
            "allow_in_memory_shared_library_loading" => {
                inner.set_allow_in_memory_shared_library_loading(value.to_bool());
            }
            "bytecode_optimize_level_zero" => {
                inner.set_bytecode_optimize_level_zero(value.to_bool());
            }
            "bytecode_optimize_level_one" => {
                inner.set_bytecode_optimize_level_one(value.to_bool());
            }
            "bytecode_optimize_level_two" => {
                inner.set_bytecode_optimize_level_two(value.to_bool());
            }
            "extension_module_filter" => {
                let filter =
                    ExtensionModuleFilter::try_from(value.to_string().as_str()).map_err(|e| {
                        ValueError::from(RuntimeError {
                            code: "PYOXIDIZER_BUILD",
                            message: e,
                            label: format!("{}.{} = {}", Self::TYPE, attribute, value),
                        })
                    })?;

                inner.set_extension_module_filter(filter);
            }
            "file_scanner_classify_files" => {
                inner.set_file_scanner_classify_files(value.to_bool());
            }
            "file_scanner_emit_files" => {
                inner.set_file_scanner_emit_files(value.to_bool());
            }
            "include_classified_resources" => {
                inner.set_include_classified_resources(value.to_bool());
            }
            "include_distribution_sources" => {
                inner.set_include_distribution_sources(value.to_bool());
            }
            "include_distribution_resources" => {
                inner.set_include_distribution_resources(value.to_bool());
            }
            "include_file_resources" => {
                inner.set_include_file_resources(value.to_bool());
            }
            "include_non_distribution_sources" => {
                inner.set_include_non_distribution_sources(value.to_bool());
            }
            "include_test" => {
                inner.set_include_test(value.to_bool());
            }
            "resources_location" => {
                inner.set_resources_location(
                    ConcreteResourceLocation::try_from(value.to_string().as_str()).map_err(
                        |e| {
                            ValueError::from(RuntimeError {
                                code: "PYOXIDIZER_BUILD",
                                message: e,
                                label: format!("{}.{} = {}", Self::TYPE, attribute, value),
                            })
                        },
                    )?,
                );
            }
            "resources_location_fallback" => {
                if value.get_type() == "NoneType" {
                    inner.set_resources_location_fallback(None);
                } else {
                    inner.set_resources_location_fallback(Some(
                        ConcreteResourceLocation::try_from(value.to_string().as_str()).map_err(
                            |e| {
                                ValueError::from(RuntimeError {
                                    code: "PYOXIDIZER_BUILD",
                                    message: e,
                                    label: format!("{}.{} = {}", Self::TYPE, attribute, value),
                                })
                            },
                        )?,
                    ));
                }
            }
            attr => {
                return Err(ValueError::OperationNotSupported {
                    op: UnsupportedOperation::SetAttr(attr.to_string()),
                    left: Self::TYPE.to_owned(),
                    right: None,
                });
            }
        }

        Ok(())
    }
}

// Starlark methods.
impl PythonPackagingPolicyValue {
    fn starlark_register_resource_callback(&mut self, func: &Value) -> ValueResult {
        required_type_arg("func", "function", func)?;

        self.derive_context_callbacks.push(func.clone());

        Ok(Value::from(NoneType::None))
    }

    #[allow(clippy::unnecessary_wraps)]
    fn starlark_set_preferred_extension_module_variant(
        &mut self,
        name: String,
        value: String,
    ) -> ValueResult {
        self.inner("PythonPackagingPolicy.set_preferred_extension_module_variant()")?
            .set_preferred_extension_module_variant(&name, &value);

        Ok(Value::from(NoneType::None))
    }

    fn starlark_set_resource_handling_mode(&mut self, value: String) -> ValueResult {
        const LABEL: &str = "PythonPackagingPolicy.set_resource_handling_mode()";

        let mode = ResourceHandlingMode::try_from(value.as_str()).map_err(|e| {
            ValueError::from(RuntimeError {
                code: "PYTHON_PACKAGING_POLICY",
                message: e,
                label: LABEL.to_string(),
            })
        })?;

        self.inner(LABEL)?.set_resource_handling_mode(mode);

        Ok(Value::from(NoneType::None))
    }
}

starlark_module! { python_packaging_policy_module =>
    PythonPackagingPolicy.register_resource_callback(this, func) {
        let mut this = this.downcast_mut::<PythonPackagingPolicyValue>().unwrap().unwrap();
        this.starlark_register_resource_callback(&func)
    }

    PythonPackagingPolicy.set_preferred_extension_module_variant(
        this,
        name: String,
        value: String
    ) {
        let mut this = this.downcast_mut::<PythonPackagingPolicyValue>().unwrap().unwrap();
        this.starlark_set_preferred_extension_module_variant(name, value)
    }

    PythonPackagingPolicy.set_resource_handling_mode(this, mode: String) {
        let mut this = this.downcast_mut::<PythonPackagingPolicyValue>().unwrap().unwrap();
        this.starlark_set_resource_handling_mode(mode)
    }
}

#[cfg(test)]
mod tests {
    use {
        super::super::testutil::*,
        super::super::{
            python_distribution::PythonDistributionValue, python_executable::PythonExecutableValue,
        },
        super::*,
        anyhow::Result,
        indoc::indoc,
    };

    #[test]
    fn test_basic() -> Result<()> {
        let mut env = test_evaluation_context_builder()?.into_context()?;

        env.eval("dist = default_python_distribution()")?;
        env.eval("policy = dist.make_python_packaging_policy()")?;

        let dist_value = env.eval("dist")?;
        let dist = dist_value
            .downcast_ref::<PythonDistributionValue>()
            .unwrap();
        let dist_ref = dist.distribution.as_ref().unwrap();

        let policy = dist_ref.create_packaging_policy().unwrap();

        // Need value to go out of scope to avoid double borrow.
        {
            let policy_value = env.eval("policy")?;
            assert_eq!(policy_value.get_type(), "PythonPackagingPolicy");

            let x = policy_value
                .downcast_ref::<PythonPackagingPolicyValue>()
                .unwrap();

            // Distribution method should return a policy equivalent to what Starlark gives us.
            assert_eq!(&policy, x.inner("test").unwrap().deref());
        }

        // attributes work
        let value = env.eval("policy.extension_module_filter")?;
        assert_eq!(value.get_type(), "string");
        assert_eq!(value.to_string(), policy.extension_module_filter().as_ref());

        let value =
            env.eval("policy.extension_module_filter = 'minimal'; policy.extension_module_filter")?;
        assert_eq!(value.to_string(), "minimal");

        let value = env.eval("policy.file_scanner_classify_files")?;
        assert_eq!(value.get_type(), "bool");
        assert!(value.to_bool());

        let value = env.eval(
            "policy.file_scanner_classify_files = False; policy.file_scanner_classify_files",
        )?;
        assert_eq!(value.get_type(), "bool");
        assert!(!value.to_bool());

        let value = env.eval("policy.file_scanner_emit_files")?;
        assert_eq!(value.get_type(), "bool");
        assert!(!value.to_bool());

        let value =
            env.eval("policy.file_scanner_emit_files = True; policy.file_scanner_emit_files")?;
        assert_eq!(value.get_type(), "bool");
        assert!(value.to_bool());

        let value = env.eval("policy.include_classified_resources")?;
        assert_eq!(value.get_type(), "bool");
        assert!(value.to_bool());

        let value = env.eval(
            "policy.include_classified_resources = False; policy.include_classified_resources",
        )?;
        assert_eq!(value.get_type(), "bool");
        assert!(!value.to_bool());

        let value = env.eval("policy.include_distribution_sources")?;
        assert_eq!(value.get_type(), "bool");
        assert_eq!(value.to_bool(), policy.include_distribution_sources());

        let value = env.eval(
            "policy.include_distribution_sources = False; policy.include_distribution_sources",
        )?;
        assert!(!value.to_bool());

        let value = env.eval(
            "policy.include_distribution_sources = True; policy.include_distribution_sources",
        )?;
        assert!(value.to_bool());

        let value = env.eval("policy.include_distribution_resources")?;
        assert_eq!(value.get_type(), "bool");
        assert_eq!(value.to_bool(), policy.include_distribution_resources());

        let value = env.eval(
            "policy.include_distribution_resources = False; policy.include_distribution_resources",
        )?;
        assert!(!value.to_bool());

        let value = env.eval(
            "policy.include_distribution_resources = True; policy.include_distribution_resources",
        )?;
        assert!(value.to_bool());

        let value = env.eval("policy.include_file_resources")?;
        assert_eq!(value.get_type(), "bool");
        assert!(!value.to_bool());

        let value =
            env.eval("policy.include_file_resources = True; policy.include_file_resources")?;
        assert_eq!(value.get_type(), "bool");
        assert!(value.to_bool());

        let value = env.eval("policy.include_non_distribution_sources")?;
        assert_eq!(value.get_type(), "bool");
        assert_eq!(value.to_bool(), policy.include_non_distribution_sources());

        let value = env.eval(
            "policy.include_non_distribution_sources = False; policy.include_non_distribution_sources",
        )?;
        assert!(!value.to_bool());

        let value = env.eval(
            "policy.include_non_distribution_sources = True; policy.include_non_distribution_sources",
        )?;
        assert!(value.to_bool());

        let value = env.eval("policy.include_test")?;
        assert_eq!(value.get_type(), "bool");
        assert_eq!(value.to_bool(), policy.include_test());

        let value = env.eval("policy.include_test = False; policy.include_test")?;
        assert!(!value.to_bool());

        let value = env.eval("policy.include_test = True; policy.include_test")?;
        assert!(value.to_bool());

        let value = env.eval("policy.resources_location")?;
        assert_eq!(value.get_type(), "string");
        assert_eq!(value.to_string(), "in-memory");

        let value = env.eval(
            "policy.resources_location = 'filesystem-relative:lib'; policy.resources_location",
        )?;
        assert_eq!(value.to_string(), "filesystem-relative:lib");

        let value = env.eval("policy.resources_location_fallback")?;
        if dist_ref.supports_in_memory_shared_library_loading() {
            assert_eq!(value.get_type(), "string");
            assert_eq!(value.to_string(), "filesystem-relative:lib");
        } else {
            assert_eq!(value.get_type(), "NoneType");
        }

        let value = env.eval("policy.resources_location_fallback = 'filesystem-relative:lib'; policy.resources_location_fallback")?;
        assert_eq!(value.to_string(), "filesystem-relative:lib");

        let value = env.eval(
            "policy.resources_location_fallback = None; policy.resources_location_fallback",
        )?;
        assert_eq!(value.get_type(), "NoneType");

        let value = env.eval("policy.allow_files")?;
        assert_eq!(value.get_type(), "bool");
        assert!(!value.to_bool());

        let value = env.eval("policy.allow_files = True; policy.allow_files")?;
        assert_eq!(value.get_type(), "bool");
        assert!(value.to_bool());

        let value = env.eval("policy.allow_in_memory_shared_library_loading")?;
        assert_eq!(value.get_type(), "bool");
        assert!(!value.to_bool());

        let value = env.eval("policy.allow_in_memory_shared_library_loading = True; policy.allow_in_memory_shared_library_loading")?;
        assert_eq!(value.get_type(), "bool");
        assert!(value.to_bool());

        // bytecode_optimize_level_zero
        let value = env.eval("policy.bytecode_optimize_level_zero")?;
        assert_eq!(value.get_type(), "bool");
        assert_eq!(value.to_bool(), policy.bytecode_optimize_level_zero());

        let value = env.eval(
            "policy.bytecode_optimize_level_zero = False; policy.bytecode_optimize_level_zero",
        )?;
        assert!(!value.to_bool());

        let value = env.eval(
            "policy.bytecode_optimize_level_zero = True; policy.bytecode_optimize_level_zero",
        )?;
        assert!(value.to_bool());

        // bytecode_optimize_level_one
        let value = env.eval("policy.bytecode_optimize_level_one")?;
        assert_eq!(value.get_type(), "bool");
        assert_eq!(value.to_bool(), policy.bytecode_optimize_level_one());

        let value = env.eval(
            "policy.bytecode_optimize_level_one = False; policy.bytecode_optimize_level_one",
        )?;
        assert!(!value.to_bool());

        let value = env.eval(
            "policy.bytecode_optimize_level_one = True; policy.bytecode_optimize_level_one",
        )?;
        assert!(value.to_bool());

        // bytecode_optimize_level_two
        let value = env.eval("policy.bytecode_optimize_level_two")?;
        assert_eq!(value.get_type(), "bool");
        assert_eq!(value.to_bool(), policy.bytecode_optimize_level_two());

        let value = env.eval(
            "policy.bytecode_optimize_level_two = False; policy.bytecode_optimize_level_two",
        )?;
        assert!(!value.to_bool());

        let value = env.eval(
            "policy.bytecode_optimize_level_two = True; policy.bytecode_optimize_level_two",
        )?;
        assert!(value.to_bool());

        Ok(())
    }

    #[test]
    fn test_preferred_extension_module_variants() -> Result<()> {
        let mut env = test_evaluation_context_builder()?.into_context()?;

        env.eval("dist = default_python_distribution()")?;
        env.eval("policy = dist.make_python_packaging_policy()")?;

        let value = env.eval("policy.preferred_extension_module_variants")?;
        assert_eq!(value.get_type(), "dict");
        assert_eq!(value.length().unwrap(), 0);

        env.eval("policy.set_preferred_extension_module_variant('foo', 'bar')")?;

        let value = env.eval("policy.preferred_extension_module_variants")?;
        assert_eq!(value.get_type(), "dict");
        assert_eq!(value.length().unwrap(), 1);
        assert_eq!(value.at(Value::from("foo")).unwrap(), Value::from("bar"));

        Ok(())
    }

    #[test]
    fn test_register_resource_callback() -> Result<()> {
        let mut env = test_evaluation_context_builder()?.into_context()?;

        env.eval("dist = default_python_distribution()")?;
        env.eval("policy = dist.make_python_packaging_policy()")?;
        env.eval("def my_func(policy, resource):\n    return None")?;

        env.eval("policy.register_resource_callback(my_func)")?;

        let policy_value = env.eval("policy")?;
        let policy = policy_value
            .downcast_ref::<PythonPackagingPolicyValue>()
            .unwrap();
        assert_eq!(policy.derive_context_callbacks.len(), 1);

        let func = policy.derive_context_callbacks[0].clone();
        assert_eq!(func.get_type(), "function");
        assert_eq!(func.to_str(), "my_func(policy, resource)");

        Ok(())
    }

    #[test]
    fn test_set_resource_handling_mode() -> Result<()> {
        let mut env = test_evaluation_context_builder()?.into_context()?;

        env.eval("dist = default_python_distribution()")?;
        env.eval("policy = dist.make_python_packaging_policy()")?;

        assert!(env
            .eval("policy.set_resource_handling_mode('invalid')")
            .is_err());

        env.eval("policy.set_resource_handling_mode('classify')")?;
        env.eval("policy.set_resource_handling_mode('files')")?;

        Ok(())
    }

    #[test]
    fn test_stdlib_extension_module_enable() -> Result<()> {
        let mut env = test_evaluation_context_builder()?.into_context()?;

        let exe_value = env.eval(indoc! {r#"
            dist = default_python_distribution()
            policy = dist.make_python_packaging_policy()
            policy.extension_module_filter = "minimal"
            policy.resources_location_fallback = "filesystem-relative:lib"

            def cb(policy, resource):
                if type(resource) == "PythonExtensionModule":
                    if resource.name == "_ssl":
                        resource.add_include = True

            policy.register_resource_callback(cb)

            exe = dist.to_python_executable(
                name = "myapp",
                packaging_policy = policy
            )

            exe
        "#})?;

        let exe = exe_value.downcast_ref::<PythonExecutableValue>().unwrap();
        let inner = exe.inner("ignored").unwrap();

        assert_eq!(
            inner
                .iter_resources()
                .filter(|(_, r)| { r.is_extension_module && r.name == "_ssl" })
                .count(),
            // TODO arguably should be 1.
            0,
        );

        Ok(())
    }

    #[test]
    fn test_stdlib_extension_module_disable() -> Result<()> {
        let mut env = test_evaluation_context_builder()?.into_context()?;

        let exe_value = env.eval(indoc! {r#"
            dist = default_python_distribution()
            policy = dist.make_python_packaging_policy()
            policy.resources_location_fallback = "filesystem-relative:lib"

            def cb(policy, resource):
                if type(resource) == "PythonExtensionModule":
                    if resource.name == "_ssl":
                        resource.add_include = False

            policy.register_resource_callback(cb)

            exe = dist.to_python_executable(
                name = "myapp",
                packaging_policy = policy
            )

            exe
        "#})?;

        let exe = exe_value.downcast_ref::<PythonExecutableValue>().unwrap();
        let inner = exe.inner("ignored").unwrap();

        assert_eq!(
            inner
                .iter_resources()
                .filter(|(_, r)| { r.is_extension_module && r.name == "_ssl" })
                .count(),
            // TODO this seems buggy.
            if cfg!(windows) { 1 } else { 0 }
        );

        Ok(())
    }

    #[test]
    fn test_ignore_non_stdlib_extension_module() -> Result<()> {
        let mut env = test_evaluation_context_builder()?.into_context()?;

        let exe_value = env.eval(indoc! {r#"
            dist = default_python_distribution()
            policy = dist.make_python_packaging_policy()
            policy.resources_location_fallback = "filesystem-relative:lib"

            exe = dist.to_python_executable(
                name = "myapp",
                packaging_policy = policy
            )

            exe.add_python_resources(exe.pip_install(["zstandard==0.16.0"]))

            exe
        "#})?;

        let exe = exe_value.downcast_ref::<PythonExecutableValue>().unwrap();
        let inner = exe.inner("ignored").unwrap();

        assert_eq!(
            inner
                .iter_resources()
                .filter(|(_, r)| { r.is_extension_module && r.name == "zstandard.backend_c" })
                .count(),
            1
        );

        let exe_value = env.eval(indoc! {r#"
            dist = default_python_distribution()
            policy = dist.make_python_packaging_policy()
            policy.resources_location_fallback = "filesystem-relative:lib"

            def cb(policy, resource):
                if type(resource) == "PythonExtensionModule":
                    if resource.name == "zstandard.backend_c":
                        resource.add_include = False

            policy.register_resource_callback(cb)

            exe = dist.to_python_executable(
                name = "myapp",
                packaging_policy = policy,
            )

            exe.add_python_resources(exe.pip_install(["zstandard==0.16.0"]))

            exe
        "#})?;

        let exe = exe_value.downcast_ref::<PythonExecutableValue>().unwrap();
        let inner = exe.inner("ignored").unwrap();

        assert_eq!(
            inner
                .iter_resources()
                .filter(|(_, r)| { r.is_extension_module && r.name == "zstandard.backend_c" })
                .count(),
            // TODO should be 0.
            1
        );

        Ok(())
    }
}
