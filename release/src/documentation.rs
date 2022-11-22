// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Documentation generation.

use {
    anyhow::{anyhow, Result},
    pulldown_cmark::{Event as MarkdownEvent, LinkType, Parser as MarkdownParser, Tag},
    rustdoc_types::{Crate, GenericArg, GenericArgs, ItemEnum, StructKind, Type},
    std::{
        fmt::Write,
        path::{Path, PathBuf},
    },
};

struct TypeReference {
    filename: PathBuf,
    name: String,
}

fn resolve_type_name(typ: &Type) -> Result<String> {
    match typ {
        Type::ResolvedPath(path) => {
            if let Some(args) = &path.args {
                if let GenericArgs::AngleBracketed { args, .. } = args.as_ref() {
                    let mut type_names = vec![];

                    for arg in args {
                        match arg {
                            GenericArg::Type(typ) => {
                                type_names.push(resolve_type_name(typ)?);
                            }
                            GenericArg::Lifetime(_) => {}
                            _ => {
                                return Err(anyhow!("do not know how to handle arg: {:?}", arg));
                            }
                        }
                    }

                    if type_names.is_empty() {
                        Ok(path.name.clone())
                    } else {
                        Ok(format!("{}<{}>", path.name, type_names.join(", ")))
                    }
                } else {
                    Err(anyhow!("do not know how to handle args"))
                }
            } else {
                Ok(path.name.clone())
            }
        }
        Type::Primitive(value) => Ok(value.to_string()),
        _ => Err(anyhow!("unable to resolve type name: {:?}", typ)),
    }
}

fn docstring_to_rst(docs: &str) -> Result<Vec<String>> {
    let mut lines = vec![];

    let parser = MarkdownParser::new(docs);

    let mut line = "".to_string();

    for event in parser {
        match event {
            MarkdownEvent::Start(Tag::Paragraph) => {
                line = "".to_string();
            }
            MarkdownEvent::End(Tag::Paragraph) => {
                lines.push(line.clone());
                lines.push("".to_string());
            }
            MarkdownEvent::Text(s) => match s.as_ref() {
                "[" | "]" => {
                    line.push_str("``");
                }
                _ => {
                    line.push_str(&s);
                }
            },
            MarkdownEvent::Code(s) => {
                write!(line, "``{}``", s)?;
            }
            MarkdownEvent::SoftBreak => {
                lines.push(line.clone());
                line = "".to_string();
            }
            MarkdownEvent::Start(Tag::Emphasis) | MarkdownEvent::End(Tag::Emphasis) => {
                line.push('*');
            }

            MarkdownEvent::Start(Tag::Link(LinkType::Autolink, ..)) => {}
            MarkdownEvent::End(Tag::Link(LinkType::Autolink, ..)) => {}
            MarkdownEvent::Start(Tag::Link(LinkType::Shortcut, ..)) => {}
            MarkdownEvent::End(Tag::Link(LinkType::Shortcut, ..)) => {}
            _ => {
                return Err(anyhow!("unhandled markdown event: {:?}", event));
            }
        }
    }

    Ok(lines)
}

fn struct_to_rst(docs: &Crate, type_ref: TypeReference, rst_prefix: &str) -> Result<Vec<String>> {
    let index = docs
        .index
        .iter()
        .find_map(|(id, item)| {
            if let Some(span) = &item.span {
                if span.filename == type_ref.filename
                    && item.name.as_ref() == Some(&type_ref.name)
                    && matches!(item.inner, ItemEnum::Struct(_))
                {
                    Some(id)
                } else {
                    None
                }
            } else {
                None
            }
        })
        .ok_or_else(|| anyhow!("unable to find {:?} struct", type_ref.name))?;

    let main_struct = docs
        .index
        .get(index)
        .ok_or_else(|| anyhow!("id not found: {:?}", index))?;

    let struct_name = main_struct
        .name
        .as_ref()
        .ok_or_else(|| anyhow!("struct has no name"))?;

    let mut lines = vec![
        format!(".. _{}_struct_{}:", rst_prefix, struct_name),
        "".to_string(),
        format!("``{}`` Struct", struct_name),
        "=".repeat(struct_name.len() + 4 + 7),
        "".to_string(),
    ];

    if let Some(docs) = &main_struct.docs {
        lines.extend(docstring_to_rst(docs)?.into_iter());
        lines.push("".to_string());
    }

    if let ItemEnum::Struct(inner) = &main_struct.inner {
        if let StructKind::Plain { fields, .. } = &inner.kind {
            for field_id in fields {
                let field_item = docs
                    .index
                    .get(field_id)
                    .ok_or_else(|| anyhow!("field index not found"))?;

                let field_name = field_item
                    .name
                    .as_ref()
                    .ok_or_else(|| anyhow!("field name not defined"))?;

                lines.push(format!(
                    ".. _{}_struct_{}_{}:",
                    rst_prefix, struct_name, field_name
                ));
                lines.push("".to_string());
                lines.push(format!("``{}`` Field", field_name));
                lines.push("-".repeat(field_name.len() + 4 + 6));
                lines.push("".to_string());

                if let ItemEnum::StructField(typ) = &field_item.inner {
                    if let Some(docs) = &field_item.docs {
                        lines.extend(docstring_to_rst(docs)?.into_iter());
                    }
                    lines.push(format!("Type: ``{}``", resolve_type_name(typ)?));
                    lines.push("".to_string());
                }
            }
        }
    }

    Ok(lines)
}

fn enum_to_rst(docs: &Crate, type_ref: TypeReference, rst_prefix: &str) -> Result<Vec<String>> {
    let index = docs
        .index
        .iter()
        .find_map(|(id, item)| {
            if let Some(span) = &item.span {
                if span.filename == type_ref.filename
                    && item.name.as_ref() == Some(&type_ref.name)
                    && matches!(item.inner, ItemEnum::Enum(_))
                {
                    Some(id)
                } else {
                    None
                }
            } else {
                None
            }
        })
        .ok_or_else(|| anyhow!("unable to find {:?} enum", type_ref.name))?;

    let main_enum = docs
        .index
        .get(index)
        .ok_or_else(|| anyhow!("id not found: {:?}", index))?;

    let enum_name = main_enum
        .name
        .as_ref()
        .ok_or_else(|| anyhow!("enum has no name"))?;

    let mut lines = vec![
        format!(".. _{}_enum_{}:", rst_prefix, enum_name),
        "".to_string(),
        format!("``{}`` Enum", enum_name),
        "=".repeat(enum_name.len() + 4 + 5),
        "".to_string(),
    ];

    if let Some(docs) = &main_enum.docs {
        lines.extend(docstring_to_rst(docs)?.into_iter());
        lines.push("".to_string());
    }

    if let ItemEnum::Enum(inner) = &main_enum.inner {
        for variant_id in &inner.variants {
            let variant = docs
                .index
                .get(variant_id)
                .ok_or_else(|| anyhow!("failed to locate variant {:?}", variant_id))?;

            let variant_name = variant
                .name
                .as_ref()
                .ok_or_else(|| anyhow!("variant has no name"))?;

            lines.push(format!("``{}`` Variant", variant_name));

            if let Some(docs) = &variant.docs {
                for line in docstring_to_rst(docs)? {
                    lines.push(format!("   {}", line));
                }
                lines.push("".to_string());
            }
        }
    }

    Ok(lines)
}

fn types_to_rst(
    repo_root: &Path,
    json_file: &str,
    rst_prefix: &str,
    structs: Vec<TypeReference>,
    enums: Vec<TypeReference>,
) -> Result<Vec<String>> {
    let fh = std::fs::File::open(repo_root.join("target").join("doc").join(json_file))?;
    let docs: Crate = serde_json::from_reader(fh)?;

    let mut lines = vec![];

    for item in structs {
        lines.extend(struct_to_rst(&docs, item, rst_prefix)?.into_iter());
        lines.push("".to_string());
    }

    for item in enums {
        lines.extend(enum_to_rst(&docs, item, rst_prefix)?.into_iter());
        lines.push("".to_string());
    }

    Ok(lines)
}

pub fn generate_sphinx_files(repo_root: &Path) -> Result<()> {
    let packages = vec!["pyembed", "python-packaging"];

    for package in packages {
        crate::run_cmd(
            "docs",
            repo_root,
            "cargo",
            vec![
                "+nightly",
                "rustdoc",
                "-p",
                package,
                "--",
                "-Z",
                "unstable-options",
                "--output-format",
                "json",
            ],
            vec![],
        )?;
    }

    for prefix in ["pyembed", "pyoxy"] {
        let mut lines = vec![
            format!(".. _{}_interpreter_config:", prefix),
            "".to_string(),
            "================================================".to_string(),
            "Python Interpreter Configuration Data Structures".to_string(),
            "================================================".to_string(),
            "".to_string(),
            "This document describes the data structures for configuring the behavior of"
                .to_string(),
            "a Python interpreter. The data structures are consumed by the ``pyembed`` Rust crate."
                .to_string(),
            "All type names should correspond to public symbols in the ``pyembed`` crate."
                .to_string(),
            "".to_string(),
            "This documentation is auto-generated from the inline documentation in Rust source"
                .to_string(),
            "files. Some formatting has been lost as part of the conversion.".to_string(),
            "See https://docs.rs/pyembed/ for the native Rust API documentation".to_string(),
            "".to_string(),
        ];

        let pyembed_structs = vec![TypeReference {
            filename: "pyembed/src/config.rs".into(),
            name: "OxidizedPythonInterpreterConfig".into(),
        }];

        let python_packaging_structs = vec![TypeReference {
            filename: "python-packaging/src/interpreter.rs".into(),
            name: "PythonInterpreterConfig".into(),
        }];

        let python_packaging_enums = vec![
            TypeReference {
                filename: "python-packaging/src/interpreter.rs".into(),
                name: "MemoryAllocatorBackend".into(),
            },
            TypeReference {
                filename: "python-packaging/src/interpreter.rs".into(),
                name: "PythonInterpreterProfile".into(),
            },
            TypeReference {
                filename: "python-packaging/src/interpreter.rs".into(),
                name: "Allocator".into(),
            },
            TypeReference {
                filename: "python-packaging/src/resource.rs".into(),
                name: "BytecodeOptimizationLevel".into(),
            },
            TypeReference {
                filename: "python-packaging/src/interpreter.rs".into(),
                name: "BytesWarning".into(),
            },
            TypeReference {
                filename: "python-packaging/src/interpreter.rs".into(),
                name: "CheckHashPycsMode".into(),
            },
            TypeReference {
                filename: "python-packaging/src/interpreter.rs".into(),
                name: "CoerceCLocale".into(),
            },
            TypeReference {
                filename: "python-packaging/src/interpreter.rs".into(),
                name: "MultiprocessingStartMethod".into(),
            },
            TypeReference {
                filename: "python-packaging/src/interpreter.rs".into(),
                name: "TerminfoResolution".into(),
            },
        ];

        lines.push("Structs:".to_string());
        lines.push("".to_string());

        for item in pyembed_structs
            .iter()
            .chain(python_packaging_structs.iter())
        {
            lines.push(format!(
                "* :ref:`{} <{}_struct_{}>`",
                item.name, prefix, item.name
            ));
        }

        lines.push("".to_string());
        lines.push("Enums:".to_string());
        lines.push("".to_string());

        for item in python_packaging_enums.iter() {
            lines.push(format!(
                "* :ref:`{} <{}_enum_{}>`",
                item.name, prefix, item.name
            ));
        }

        lines.push("".to_string());

        lines.extend(
            types_to_rst(repo_root, "pyembed.json", prefix, pyembed_structs, vec![])?.into_iter(),
        );

        lines.extend(
            types_to_rst(
                repo_root,
                "python_packaging.json",
                prefix,
                python_packaging_structs,
                python_packaging_enums,
            )?
            .into_iter(),
        );

        let output_path = repo_root
            .join(prefix)
            .join("docs")
            .join(format!("{}_interpreter_config.rst", prefix));

        let rst = lines.join("\n");
        std::fs::write(&output_path, rst.as_bytes())?;
    }

    Ok(())
}
