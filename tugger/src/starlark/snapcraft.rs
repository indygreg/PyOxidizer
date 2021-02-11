// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    crate::starlark::file_resource::FileManifestValue,
    starlark::{
        environment::TypeValues,
        values::{
            error::{RuntimeError, UnsupportedOperation, ValueError},
            none::NoneType,
            {Mutable, TypedValue, Value, ValueResult},
        },
        {
            starlark_fun, starlark_module, starlark_parse_param_type, starlark_signature,
            starlark_signature_extraction, starlark_signatures,
        },
    },
    starlark_dialect_build_targets::{
        get_context_value, optional_bool_arg, EnvironmentContext, ResolvedTarget,
        ResolvedTargetValue, RunMode, ToOptional, TryToOptional,
    },
    std::{borrow::Cow, collections::HashMap, convert::TryFrom},
    tugger_snapcraft::{
        Adapter, Architecture, Architectures, BuildAttribute, Confinement, Daemon, Grade,
        RestartCondition, SnapApp, SnapPart, Snapcraft, SnapcraftBuilder, SnapcraftInvocation,
        SourceType, Type,
    },
};

fn optional_str_vec_to_vec(value: Value) -> Result<Vec<Cow<'static, str>>, ValueError> {
    let v: Option<Vec<Cow<'static, str>>> = value.try_to_optional()?;

    if let Some(v) = v {
        Ok(v)
    } else {
        Ok(vec![])
    }
}

fn optional_str_hashmap_to_hashmap(
    value: Value,
) -> Result<HashMap<Cow<'static, str>, Cow<'static, str>>, ValueError> {
    let v: Option<HashMap<Cow<'static, str>, Cow<'static, str>>> = value.try_to_optional()?;

    if let Some(v) = v {
        Ok(v)
    } else {
        Ok(HashMap::new())
    }
}

type VecOfStrHashMap = Vec<HashMap<Cow<'static, str>, Cow<'static, str>>>;

fn optional_vec_str_hashmap_to_vec(value: Value) -> Result<VecOfStrHashMap, ValueError> {
    let v: Option<VecOfStrHashMap> = value.try_to_optional()?;

    if let Some(v) = v {
        Ok(v)
    } else {
        Ok(Vec::new())
    }
}

fn value_to_optional_adapter(value: Value) -> Result<Option<Adapter>, ValueError> {
    if value.get_type() == "NoneType" {
        Ok(None)
    } else {
        Ok(Some(
            Adapter::try_from(value.to_string().as_str()).map_err(|e| {
                ValueError::from(RuntimeError {
                    code: "TUGGER_SNAPCRAFT",
                    message: e.to_string(),
                    label: "adapter".to_string(),
                })
            })?,
        ))
    }
}

fn value_to_optional_architectures(value: Value) -> Result<Option<Architectures>, ValueError> {
    match value.get_type() {
        "NoneType" => Ok(None),
        "dict" => {
            let build_on_value = value.at(Value::from("build_on"))?;
            let run_on_value = value.at(Value::from("run_on"))?;

            let build_on_strings: Option<Vec<String>> = build_on_value.try_to_optional()?;
            let run_on_strings: Option<Vec<String>> = run_on_value.try_to_optional()?;

            let mut build_on_arches = Vec::new();
            if let Some(arches) = build_on_strings {
                for v in arches {
                    build_on_arches.push(Architecture::try_from(v.as_str()).map_err(|e| {
                        ValueError::from(RuntimeError {
                            code: "TUGGER_SNAPCRAFT",
                            message: format!("error parsing architecture string: {}", e),
                            label: "architectures".to_string(),
                        })
                    })?);
                }
            }

            let mut run_on_arches = Vec::new();
            if let Some(arches) = run_on_strings {
                for v in arches {
                    run_on_arches.push(Architecture::try_from(v.as_str()).map_err(|e| {
                        ValueError::from(RuntimeError {
                            code: "TUGGER_SNAPCRAFT",
                            message: format!("error parsing architecture string: {}", e),
                            label: "architectures".to_string(),
                        })
                    })?);
                }
            }

            Ok(Some(Architectures {
                build_on: build_on_arches,
                run_on: run_on_arches,
            }))
        }
        t => Err(ValueError::from(RuntimeError {
            code: "TUGGER_SNAPCRAFT",
            message: format!("architectures value must be None or dict; got {}", t),
            label: "architectures".to_string(),
        })),
    }
}

fn value_to_optional_confinement(value: Value) -> Result<Option<Confinement>, ValueError> {
    if value.get_type() == "NoneType" {
        Ok(None)
    } else {
        Ok(Some(
            Confinement::try_from(value.to_string().as_str()).map_err(|e| {
                ValueError::from(RuntimeError {
                    code: "TUGGER_SNAPCRAFT",
                    message: e.to_string(),
                    label: "confinement".to_string(),
                })
            })?,
        ))
    }
}

fn value_to_optional_daemon(value: Value) -> Result<Option<Daemon>, ValueError> {
    if value.get_type() == "NoneType" {
        Ok(None)
    } else {
        Ok(Some(Daemon::try_from(value.to_string().as_str()).map_err(
            |e| {
                ValueError::from(RuntimeError {
                    code: "TUGGER_SNAPCRAFT",
                    message: e.to_string(),
                    label: "daemon".to_string(),
                })
            },
        )?))
    }
}

fn value_to_optional_grade(value: Value) -> Result<Option<Grade>, ValueError> {
    if value.get_type() == "NoneType" {
        Ok(None)
    } else {
        Ok(Some(Grade::try_from(value.to_string().as_str()).map_err(
            |e| {
                ValueError::from(RuntimeError {
                    code: "TUGGER_SNAPCRAFT",
                    message: e.to_string(),
                    label: "grade".to_string(),
                })
            },
        )?))
    }
}

fn value_to_optional_restart_condition(
    value: Value,
) -> Result<Option<RestartCondition>, ValueError> {
    if value.get_type() == "NoneType" {
        Ok(None)
    } else {
        Ok(Some(
            RestartCondition::try_from(value.to_string().as_str()).map_err(|e| {
                ValueError::from(RuntimeError {
                    code: "TUGGER_SNAPCRAFT",
                    message: e.to_string(),
                    label: "restart_condition".to_string(),
                })
            })?,
        ))
    }
}

fn value_to_optional_source_type(value: Value) -> Result<Option<SourceType>, ValueError> {
    if value.get_type() == "NoneType" {
        Ok(None)
    } else {
        Ok(Some(
            SourceType::try_from(value.to_string().as_str()).map_err(|e| {
                ValueError::from(RuntimeError {
                    code: "TUGGER_SNAPCRAFT",
                    message: e.to_string(),
                    label: "restart_condition".to_string(),
                })
            })?,
        ))
    }
}

fn value_to_optional_type(value: Value) -> Result<Option<Type>, ValueError> {
    if value.get_type() == "NoneType" {
        Ok(None)
    } else {
        Ok(Some(Type::try_from(value.to_string().as_str()).map_err(
            |e| {
                ValueError::from(RuntimeError {
                    code: "TUGGER_SNAPCRAFT",
                    message: e.to_string(),
                    label: "type".to_string(),
                })
            },
        )?))
    }
}

fn value_to_build_attributes(value: Value) -> Result<Vec<BuildAttribute>, ValueError> {
    match value.get_type() {
        "NoneType" => Ok(vec![]),
        "list" => {
            let mut res = Vec::new();

            for v in &value.iter()? {
                res.push(
                    BuildAttribute::try_from(v.to_string().as_str()).map_err(|e| {
                        ValueError::from(RuntimeError {
                            code: "TUGGER_SNAPCRAFT",
                            message: e.to_string(),
                            label: "build_attributes".to_string(),
                        })
                    })?,
                );
            }

            Ok(res)
        }
        t => Err(ValueError::from(RuntimeError {
            code: "TUGGER_SNAPCRAFT",
            message: format!("build_attributes must be None or list; got {}", t),
            label: "build_attributes".to_string(),
        })),
    }
}

fn value_to_apps(value: Value) -> Result<HashMap<Cow<'static, str>, SnapApp<'static>>, ValueError> {
    match value.get_type() {
        "NoneType" => Ok(HashMap::new()),
        "dict" => {
            let mut res = HashMap::new();

            for key in &value.iter()? {
                let v = value.at(key.clone())?;

                let app_value = v.downcast_ref::<SnapAppValue<'_>>().ok_or_else(|| {
                    ValueError::from(RuntimeError {
                        code: "TUGGER_SNAPCRAFT",
                        message: format!("apps value must be SnapApp; got {}", v.get_type()),
                        label: "apps".to_string(),
                    })
                })?;

                res.insert(Cow::Owned(key.to_string()), app_value.inner.clone());
            }

            Ok(res)
        }
        t => Err(ValueError::from(RuntimeError {
            code: "TUGGER_SNAPCRAFT",
            message: format!("apps must be None or dict; got {}", t),
            label: "apps".to_string(),
        })),
    }
}

fn value_to_parts(
    value: Value,
) -> Result<HashMap<Cow<'static, str>, SnapPart<'static>>, ValueError> {
    match value.get_type() {
        "NoneType" => Ok(HashMap::new()),
        "dict" => {
            let mut res = HashMap::new();

            for key in &value.iter()? {
                let v = value.at(key.clone())?;

                let app_value = v.downcast_ref::<SnapPartValue<'_>>().ok_or_else(|| {
                    ValueError::from(RuntimeError {
                        code: "TUGGER_SNAPCRAFT",
                        message: format!("parts value must be SnapPart; got {}", v.get_type()),
                        label: "parts".to_string(),
                    })
                })?;

                res.insert(Cow::Owned(key.to_string()), app_value.inner.clone());
            }

            Ok(res)
        }
        t => Err(ValueError::from(RuntimeError {
            code: "TUGGER_SNAPCRAFT",
            message: format!("parts must be None or dict; got {}", t),
            label: "parts".to_string(),
        })),
    }
}

fn value_to_filesets(
    value: Value,
) -> Result<HashMap<Cow<'static, str>, Vec<Cow<'static, str>>>, ValueError> {
    match value.get_type() {
        "NoneType" => Ok(HashMap::new()),
        "dict" => {
            let mut res = HashMap::new();

            for key in &value.iter()? {
                let v: Option<Vec<Cow<'static, str>>> = value.at(key.clone())?.try_to_optional()?;
                match v {
                    Some(v) => {
                        res.insert(Cow::Owned(key.to_string()), v);
                    }
                    None => {
                        return Err(ValueError::from(RuntimeError {
                            code: "TUGGER_SNAPCRAFT",
                            message: "filesets values must be lists of strings".to_string(),
                            label: "filesets".to_string(),
                        }));
                    }
                }
            }

            Ok(res)
        }
        t => Err(ValueError::from(RuntimeError {
            code: "TUGGER_SNAPCRAFT",
            message: format!("filesets must be None or dict; got {}", t),
            label: "filesets".to_string(),
        })),
    }
}

#[derive(Clone, Debug, Default)]
pub struct SnapAppValue<'a> {
    pub inner: SnapApp<'a>,
}

impl TypedValue for SnapAppValue<'static> {
    type Holder = Mutable<SnapAppValue<'static>>;
    const TYPE: &'static str = "SnapApp";

    fn values_for_descendant_check_and_freeze(&self) -> Box<dyn Iterator<Item = Value>> {
        Box::new(std::iter::empty())
    }

    fn set_attr(&mut self, attribute: &str, value: Value) -> Result<(), ValueError> {
        match attribute {
            "adapter" => {
                self.inner.adapter = value_to_optional_adapter(value)?;
            }
            "autostart" => {
                self.inner.autostart = value.to_optional();
            }
            "command_chain" => {
                self.inner.command_chain = optional_str_vec_to_vec(value)?;
            }
            "command" => {
                self.inner.command = value.to_optional();
            }
            "common_id" => {
                self.inner.common_id = value.to_optional();
            }
            "daemon" => {
                self.inner.daemon = value_to_optional_daemon(value)?;
            }
            "desktop" => {
                self.inner.desktop = value.to_optional();
            }
            "environment" => {
                self.inner.environment = optional_str_hashmap_to_hashmap(value)?;
            }
            "extensions" => {
                self.inner.extensions = optional_str_vec_to_vec(value)?;
            }
            "listen_stream" => {
                self.inner.listen_stream = value.to_optional();
            }
            "passthrough" => {
                self.inner.passthrough = optional_str_hashmap_to_hashmap(value)?;
            }
            "plugs" => {
                self.inner.plugs = optional_str_vec_to_vec(value)?;
            }
            "post_stop_command" => {
                self.inner.post_stop_command = value.to_optional();
            }
            "restart_condition" => {
                self.inner.restart_condition = value_to_optional_restart_condition(value)?;
            }
            "slots" => {
                self.inner.slots = optional_str_vec_to_vec(value)?;
            }
            "stop_command" => {
                self.inner.stop_command = value.to_optional();
            }
            "stop_timeout" => {
                self.inner.stop_timeout = value.to_optional();
            }
            "timer" => {
                self.inner.timer = value.to_optional();
            }
            "socket_mode" => {
                self.inner.socket_mode = value.try_to_optional()?;
            }
            "socket" => {
                self.inner.socket = optional_str_hashmap_to_hashmap(value)?;
            }
            attr => {
                return Err(ValueError::OperationNotSupported {
                    op: UnsupportedOperation::SetAttr(attr.to_string()),
                    left: Self::TYPE.to_string(),
                    right: None,
                })
            }
        }

        Ok(())
    }
}

#[derive(Clone, Debug, Default)]
pub struct SnapPartValue<'a> {
    pub inner: SnapPart<'a>,
}

impl TypedValue for SnapPartValue<'static> {
    type Holder = Mutable<SnapPartValue<'static>>;
    const TYPE: &'static str = "SnapPart";

    fn values_for_descendant_check_and_freeze(&self) -> Box<dyn Iterator<Item = Value>> {
        Box::new(std::iter::empty())
    }

    fn set_attr(&mut self, attribute: &str, value: Value) -> Result<(), ValueError> {
        match attribute {
            "after" => {
                self.inner.after = optional_str_vec_to_vec(value)?;
            }
            "build_attributes" => {
                self.inner.build_attributes = value_to_build_attributes(value)?;
            }
            "build_environment" => {
                self.inner.build_environment = optional_vec_str_hashmap_to_vec(value)?;
            }
            "build_packages" => {
                self.inner.build_packages = optional_str_vec_to_vec(value)?;
            }
            "build_snaps" => {
                self.inner.build_snaps = optional_str_vec_to_vec(value)?;
            }
            "filesets" => {
                self.inner.filesets = value_to_filesets(value)?;
            }
            "organize" => {
                self.inner.organize = optional_str_hashmap_to_hashmap(value)?;
            }
            "override_build" => {
                self.inner.override_build = value.to_optional();
            }
            "override_prime" => {
                self.inner.override_prime = value.to_optional();
            }
            "override_pull" => {
                self.inner.override_pull = value.to_optional();
            }
            "override_stage" => {
                self.inner.override_stage = value.to_optional();
            }
            "parse_info" => {
                self.inner.parse_info = value.to_optional();
            }
            "plugin" => {
                self.inner.plugin = value.to_optional();
            }
            "prime" => {
                self.inner.prime = optional_str_vec_to_vec(value)?;
            }
            "source_branch" => {
                self.inner.source_branch = value.to_optional();
            }
            "source_checksum" => {
                self.inner.source_checksum = value.to_optional();
            }
            "source_commit" => {
                self.inner.source_commit = value.to_optional();
            }
            "source_depth" => {
                self.inner.source_depth = value.try_to_optional()?;
            }
            "source_subdir" => {
                self.inner.source_subdir = value.to_optional();
            }
            "source_tag" => {
                self.inner.source_tag = value.to_optional();
            }
            "source_type" => {
                self.inner.source_type = value_to_optional_source_type(value)?;
            }
            "source" => {
                self.inner.source = value.to_optional();
            }
            "stage_packages" => {
                self.inner.stage_packages = optional_str_vec_to_vec(value)?;
            }
            "stage_snaps" => {
                self.inner.stage_snaps = optional_str_vec_to_vec(value)?;
            }
            "stage" => {
                self.inner.stage = optional_str_vec_to_vec(value)?;
            }
            attr => {
                return Err(ValueError::OperationNotSupported {
                    op: UnsupportedOperation::SetAttr(attr.to_string()),
                    left: Self::TYPE.to_string(),
                    right: None,
                })
            }
        }

        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct SnapValue<'a> {
    pub inner: Snapcraft<'a>,
}

impl TypedValue for SnapValue<'static> {
    type Holder = Mutable<SnapValue<'static>>;
    const TYPE: &'static str = "Snap";

    fn values_for_descendant_check_and_freeze(&self) -> Box<dyn Iterator<Item = Value>> {
        Box::new(std::iter::empty())
    }

    fn set_attr(&mut self, attribute: &str, value: Value) -> Result<(), ValueError> {
        match attribute {
            "adopt_info" => {
                self.inner.adopt_info = value.to_optional();
            }
            "apps" => {
                self.inner.apps = value_to_apps(value)?;
            }
            "architectures" => {
                self.inner.architectures = value_to_optional_architectures(value)?;
            }
            "assumes" => {
                self.inner.assumes = optional_str_vec_to_vec(value)?;
            }
            "base" => {
                self.inner.base = value.to_optional();
            }
            "confinement" => {
                self.inner.confinement = value_to_optional_confinement(value)?;
            }
            "description" => {
                self.inner.description = Cow::Owned(value.to_string());
            }
            "grade" => {
                self.inner.grade = value_to_optional_grade(value)?;
            }
            "icon" => {
                self.inner.icon = value.to_optional();
            }
            "license" => {
                self.inner.license = value.to_optional();
            }
            "name" => {
                self.inner.name = Cow::Owned(value.to_string());
            }
            "passthrough" => {
                self.inner.passthrough = optional_str_hashmap_to_hashmap(value)?;
            }
            "parts" => {
                self.inner.parts = value_to_parts(value)?;
            }
            "plugs" => {
                self.inner.plugs = match value.try_to_optional()? {
                    Some(value) => value,
                    None => {
                        return Err(ValueError::from(RuntimeError {
                            code: "TUGGER_SNAPCRAFT",
                            message: "expected a dict of dict[string, string]; got None"
                                .to_string(),
                            label: "plugs".to_string(),
                        }));
                    }
                }
            }
            "slots" => {
                self.inner.slots = match value.try_to_optional()? {
                    Some(value) => value,
                    None => {
                        return Err(ValueError::from(RuntimeError {
                            code: "TUGGER_SNAPCRAFT",
                            message: "expected a dict of dict[string, string]; got None"
                                .to_string(),
                            label: "slots".to_string(),
                        }));
                    }
                }
            }
            "summary" => {
                self.inner.summary = Cow::Owned(value.to_string());
            }
            "title" => {
                self.inner.title = value.to_optional();
            }
            "type" => {
                self.inner.snap_type = value_to_optional_type(value)?;
            }
            "version" => {
                self.inner.version = Cow::Owned(value.to_string());
            }
            attr => {
                return Err(ValueError::OperationNotSupported {
                    op: UnsupportedOperation::SetAttr(attr.to_string()),
                    left: Self::TYPE.to_string(),
                    right: None,
                })
            }
        }

        Ok(())
    }
}

impl<'a> SnapValue<'a> {
    fn new_from_args(name: String, version: String, summary: String, description: String) -> Self {
        SnapValue {
            inner: Snapcraft::new(
                Cow::Owned(name),
                Cow::Owned(version),
                Cow::Owned(summary),
                Cow::Owned(description),
            ),
        }
    }
}

/// Starlark Value wrapper for `SnapcraftBuilder`.
#[derive(Clone, Debug)]
pub struct SnapcraftBuilderValue<'a> {
    pub inner: SnapcraftBuilder<'a>,
}

impl TypedValue for SnapcraftBuilderValue<'static> {
    type Holder = Mutable<SnapcraftBuilderValue<'static>>;
    const TYPE: &'static str = "SnapcraftBuilder";

    fn values_for_descendant_check_and_freeze(&self) -> Box<dyn Iterator<Item = Value>> {
        Box::new(std::iter::empty())
    }
}

impl SnapcraftBuilderValue<'static> {
    pub fn new_from_snap_value(value: SnapValue<'static>) -> Self {
        let inner = SnapcraftBuilder::new(value.inner.clone());

        SnapcraftBuilderValue { inner }
    }

    pub fn add_invocation(&mut self, args: Vec<String>, purge_build: Value) -> ValueResult {
        let purge_build = optional_bool_arg("purge_build", &purge_build)?;

        match purge_build {
            Some(purge_build) => {
                let invocation = SnapcraftInvocation { args, purge_build };

                self.inner = self.inner.clone().add_invocation(invocation);
            }
            None => {
                self.inner = self.inner.clone().add_invocation_args(&args);
            }
        }

        Ok(Value::new(NoneType::None))
    }

    pub fn add_file_manifest(&mut self, manifest: FileManifestValue) -> ValueResult {
        self.inner = self
            .inner
            .clone()
            .install_manifest(&manifest.manifest)
            .map_err(|e| {
                ValueError::Runtime(RuntimeError {
                    code: "TUGGER_SNAPCRAFT",
                    message: format!("{:?}", e),
                    label: "add_file_manifest()".to_string(),
                })
            })?;

        Ok(Value::new(NoneType::None))
    }

    pub fn build(&self, type_values: &TypeValues, target: String) -> ValueResult {
        let context_value = get_context_value(type_values)?;
        let context = context_value
            .downcast_ref::<EnvironmentContext>()
            .ok_or(ValueError::IncorrectParameterType)?;

        let output_path = context.target_build_path(&target);

        self.inner
            .build(context.logger(), &output_path)
            .map_err(|e| {
                ValueError::Runtime(RuntimeError {
                    code: "TUGGER_SNAPCRAFT",
                    message: format!("{:?}", e),
                    label: "build()".to_string(),
                })
            })?;

        Ok(Value::new(ResolvedTargetValue {
            inner: ResolvedTarget {
                run_mode: RunMode::None,
                output_path,
            },
        }))
    }
}

starlark_module! { snapcraft_module =>
    #[allow(non_snake_case)]
    SnapApp() {
        Ok(Value::new(SnapAppValue::default()))
    }

    #[allow(non_snake_case)]
    SnapPart() {
        Ok(Value::new(SnapPartValue::default()))
    }

    #[allow(non_snake_case)]
    Snap(name: String, version: String, summary: String, description: String) {
        Ok(Value::new(SnapValue::new_from_args(name, version, summary, description)))
    }

    #[allow(non_snake_case)]
    Snap.to_builder(this: SnapValue<'_>) {
        Ok(Value::new(SnapcraftBuilderValue::new_from_snap_value(this)))
    }

    #[allow(non_snake_case)]
    SnapcraftBuilder(snap: SnapValue<'_>) {
        Ok(Value::new(SnapcraftBuilderValue::new_from_snap_value(snap)))
    }

    #[allow(non_snake_case)]
    SnapcraftBuilder.add_invocation(this, args: Vec<String>, purge_build = NoneType::None) {
        let mut this = this.downcast_mut::<SnapcraftBuilderValue<'_>>().unwrap().unwrap();

        this.add_invocation(args, purge_build)
    }

    #[allow(non_snake_case)]
    SnapcraftBuilder.add_file_manifest(this, manifest: FileManifestValue) {
        let mut this = this.downcast_mut::<SnapcraftBuilderValue<'_>>().unwrap().unwrap();

        this.add_file_manifest(manifest)
    }

    #[allow(non_snake_case)]
    SnapcraftBuilder.build(env env, this, target: String) {
        let this = this.downcast_ref::<SnapcraftBuilderValue<'_>>().unwrap();
        this.build(env, target)
    }
}

#[cfg(test)]
mod tests {
    use {
        super::*, crate::starlark::testutil::*, anyhow::Result, tugger_file_manifest::FileManifest,
    };

    #[test]
    fn test_app_basic() -> Result<()> {
        let mut env = StarlarkEnvironment::new()?;

        let app_value = env.eval("app = SnapApp(); app")?;
        assert_eq!(app_value.get_type(), "SnapApp");

        env.eval("app.adapter = 'full'")?;
        env.eval("app.autostart = 'autostart'")?;
        env.eval("app.command_chain = ['chain0', 'chain1']")?;
        env.eval("app.command = 'command'")?;
        env.eval("app.common_id = 'common_id'")?;
        env.eval("app.daemon = 'oneshot'")?;
        env.eval("app.desktop = 'desktop'")?;
        env.eval("app.environment = {'env0': 'env0_value'}")?;
        env.eval("app.extensions = ['ext0', 'ext1']")?;
        env.eval("app.listen_stream = 'listen_stream'")?;
        env.eval("app.passthrough = {'key0': 'key0_value'}")?;
        env.eval("app.plugs = ['plug0', 'plug1']")?;
        env.eval("app.post_stop_command = 'post_stop_command'")?;
        env.eval("app.restart_condition = 'on-failure'")?;
        env.eval("app.slots = ['slot0', 'slot1']")?;
        env.eval("app.stop_command = 'stop_command'")?;
        env.eval("app.stop_timeout = 'stop_timeout'")?;
        env.eval("app.timer = 'timer'")?;
        env.eval("app.socket_mode = 42")?;
        env.eval("app.socket = {'sock0': 'sock0_value'}")?;

        let app = app_value.downcast_ref::<SnapAppValue<'_>>().unwrap();
        assert_eq!(
            app.inner,
            SnapApp {
                adapter: Some(Adapter::Full),
                autostart: Some("autostart".into()),
                command_chain: vec!["chain0".into(), "chain1".into()],
                command: Some("command".into()),
                common_id: Some("common_id".into()),
                daemon: Some(Daemon::Oneshot),
                desktop: Some("desktop".into()),
                environment: [("env0".into(), "env0_value".into())]
                    .iter()
                    .cloned()
                    .collect(),
                extensions: vec!["ext0".into(), "ext1".into()],
                listen_stream: Some("listen_stream".into()),
                passthrough: [("key0".into(), "key0_value".into())]
                    .iter()
                    .cloned()
                    .collect(),
                plugs: vec!["plug0".into(), "plug1".into()],
                post_stop_command: Some("post_stop_command".into()),
                restart_condition: Some(RestartCondition::OnFailure),
                slots: vec!["slot0".into(), "slot1".into()],
                stop_command: Some("stop_command".into()),
                stop_timeout: Some("stop_timeout".into()),
                timer: Some("timer".into()),
                socket_mode: Some(42),
                socket: [("sock0".into(), "sock0_value".into())]
                    .iter()
                    .cloned()
                    .collect(),
            }
        );

        Ok(())
    }

    #[test]
    fn test_part_basic() -> Result<()> {
        let mut env = StarlarkEnvironment::new()?;

        let part_value = env.eval("part = SnapPart(); part")?;
        assert_eq!(part_value.get_type(), "SnapPart");

        env.eval("part.after = ['after0', 'after1']")?;
        env.eval("part.build_attributes = ['debug', 'no-patchelf']")?;
        env.eval("part.build_environment = [{'env0': 'env1'}]")?;
        env.eval("part.build_packages = ['p0', 'p1']")?;
        env.eval("part.build_snaps = ['snap0', 'snap1']")?;
        env.eval("part.filesets = {'set0': ['val0', 'val1']}")?;
        env.eval("part.organize = {'org0': 'org0_value'}")?;
        env.eval("part.override_build = 'build'")?;
        env.eval("part.override_prime = 'prime'")?;
        env.eval("part.override_pull = 'pull'")?;
        env.eval("part.override_stage = 'stage'")?;
        env.eval("part.parse_info = 'parse_info'")?;
        env.eval("part.plugin = 'plugin'")?;
        env.eval("part.prime = ['prime0', 'prime1']")?;
        env.eval("part.source_branch = 'source_branch'")?;
        env.eval("part.source_checksum = 'source_checksum'")?;
        env.eval("part.source_commit = 'source_commit'")?;
        env.eval("part.source_depth = 42")?;
        env.eval("part.source_subdir = 'source_subdir'")?;
        env.eval("part.source_tag = 'source_tag'")?;
        env.eval("part.source_type = 'hg'")?;
        env.eval("part.source = 'source'")?;
        env.eval("part.stage_packages = ['pack0', 'pack1']")?;
        env.eval("part.stage_snaps = ['snap0', 'snap1']")?;
        env.eval("part.stage = ['stage0', 'stage1']")?;

        let part = part_value.downcast_ref::<SnapPartValue<'_>>().unwrap();
        assert_eq!(
            part.inner,
            SnapPart {
                after: vec!["after0".into(), "after1".into()],
                build_attributes: vec![BuildAttribute::Debug, BuildAttribute::NoPatchelf],
                build_environment: vec![[("env0".into(), "env1".into())].iter().cloned().collect()],
                build_packages: vec!["p0".into(), "p1".into()],
                build_snaps: vec!["snap0".into(), "snap1".into()],
                filesets: [("set0".into(), vec!["val0".into(), "val1".into()])]
                    .iter()
                    .cloned()
                    .collect(),
                organize: [("org0".into(), "org0_value".into())]
                    .iter()
                    .cloned()
                    .collect(),
                override_build: Some("build".into()),
                override_prime: Some("prime".into()),
                override_pull: Some("pull".into()),
                override_stage: Some("stage".into()),
                parse_info: Some("parse_info".into()),
                plugin: Some("plugin".into()),
                prime: vec!["prime0".into(), "prime1".into()],
                source_branch: Some("source_branch".into()),
                source_checksum: Some("source_checksum".into()),
                source_commit: Some("source_commit".into()),
                source_depth: Some(42),
                source_subdir: Some("source_subdir".into()),
                source_tag: Some("source_tag".into()),
                source_type: Some(SourceType::Hg),
                source: Some("source".into()),
                stage_packages: vec!["pack0".into(), "pack1".into()],
                stage_snaps: vec!["snap0".into(), "snap1".into()],
                stage: vec!["stage0".into(), "stage1".into()],
            }
        );

        Ok(())
    }

    #[test]
    fn test_snap_basic() -> Result<()> {
        let mut env = StarlarkEnvironment::new()?;

        let snap_value =
            env.eval("snap = Snap('name', 'version', 'summary', 'description'); snap")?;
        assert_eq!(snap_value.get_type(), "Snap");

        env.eval("snap.adopt_info = 'adopt_info'")?;
        env.eval("snap.apps = {'app0': SnapApp()}")?;
        env.eval(
            "snap.architectures = {'build_on': ['s390x', 'arm64'], 'run_on': ['i386', 'amd64']}",
        )?;
        env.eval("snap.assumes = ['assume0', 'assume1']")?;
        env.eval("snap.base = 'base'")?;
        env.eval("snap.confinement = 'classic'")?;
        env.eval("snap.grade = 'stable'")?;
        env.eval("snap.icon = 'icon'")?;
        env.eval("snap.license = 'license'")?;
        env.eval("snap.passthrough = {'key0': 'value0'}")?;
        env.eval("snap.parts = {'part0': SnapPart()}")?;
        env.eval("snap.plugs = {'plug0': {'key0': 'value0'}}")?;
        env.eval("snap.slots = {'slot0': {'key0': 'value0'}}")?;
        env.eval("snap.title = 'title'")?;
        env.eval("snap.type = 'kernel'")?;

        let snap = snap_value.downcast_ref::<SnapValue<'_>>().unwrap();
        let mut expected = Snapcraft::new(
            "name".into(),
            "version".into(),
            "summary".into(),
            "description".into(),
        );
        expected.adopt_info = Some("adopt_info".into());
        expected.apps.insert("app0".into(), SnapApp::default());
        expected.architectures = Some(Architectures {
            build_on: vec![Architecture::S390x, Architecture::Arm64],
            run_on: vec![Architecture::I386, Architecture::Amd64],
        });
        expected.assumes = vec!["assume0".into(), "assume1".into()];
        expected.base = Some("base".into());
        expected.confinement = Some(Confinement::Classic);
        expected.grade = Some(Grade::Stable);
        expected.icon = Some("icon".into());
        expected.license = Some("license".into());
        expected.passthrough = [("key0".into(), "value0".into())].iter().cloned().collect();
        expected.parts = [("part0".into(), SnapPart::default())]
            .iter()
            .cloned()
            .collect();
        expected.plugs = [(
            "plug0".into(),
            [("key0".into(), "value0".into())].iter().cloned().collect(),
        )]
        .iter()
        .cloned()
        .collect();
        expected.slots = [(
            "slot0".into(),
            [("key0".into(), "value0".into())].iter().cloned().collect(),
        )]
        .iter()
        .cloned()
        .collect();
        expected.title = Some("title".into());
        expected.snap_type = Some(Type::Kernel);

        assert_eq!(snap.inner, expected);

        let builder = env.eval("builder = snap.to_builder(); builder")?;
        assert_eq!(builder.get_type(), "SnapcraftBuilder");

        Ok(())
    }

    #[test]
    fn test_snapcraft_builder_add_invocation() -> Result<()> {
        let mut env = StarlarkEnvironment::new()?;

        env.eval("snap = Snap('name', 'version', 'summary', 'description')")?;
        let builder_value = env.eval("builder = snap.to_builder(); builder")?;
        env.eval("builder.add_invocation(['cmd0', 'arg1'])")?;
        env.eval("builder.add_invocation(['cmd1', 'arg2'], purge_build = True)")?;
        env.eval("builder.add_invocation(['cmd2'], purge_build = False)")?;

        let builder = builder_value
            .downcast_ref::<SnapcraftBuilderValue<'_>>()
            .unwrap();
        assert_eq!(
            builder.inner.snap(),
            &Snapcraft::new(
                "name".into(),
                "version".into(),
                "summary".into(),
                "description".into()
            ),
        );
        assert_eq!(
            builder.inner.invocations(),
            &vec![
                SnapcraftInvocation {
                    args: vec!["cmd0".into(), "arg1".into()],
                    purge_build: true,
                },
                SnapcraftInvocation {
                    args: vec!["cmd1".into(), "arg2".into()],
                    purge_build: true,
                },
                SnapcraftInvocation {
                    args: vec!["cmd2".into()],
                    purge_build: false,
                },
            ]
        );
        assert_eq!(builder.inner.install_files(), &FileManifest::default());

        Ok(())
    }

    #[test]
    fn test_snapcraft_builder_add_manifest() -> Result<()> {
        let mut env = StarlarkEnvironment::new()?;

        env.eval("snap = Snap('name', 'version', 'summary', 'description')")?;
        env.eval("builder = snap.to_builder()")?;
        env.eval("manifest = FileManifest()")?;
        env.eval("builder.add_file_manifest(manifest)")?;

        Ok(())
    }
}
