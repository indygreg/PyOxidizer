// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Documentation generation.

use {
    anyhow::{anyhow, Result},
    pulldown_cmark::{Event as MarkdownEvent, LinkType, Parser as MarkdownParser, Tag},
    rustdoc_types::{Crate, GenericArg, GenericArgs, ItemEnum, ItemKind, Type},
    std::path::Path,
};

fn resolve_type_name(typ: &Type) -> Result<String> {
    match typ {
        Type::ResolvedPath { name, args, .. } => {
            if let Some(args) = args {
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
                        Ok(name.to_string())
                    } else {
                        Ok(format!("{}<{}>", name, type_names.join(", ")))
                    }
                } else {
                    return Err(anyhow!("do not know how to handle args"));
                }
            } else {
                Ok(name.to_string())
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
                    line.extend("``".chars());
                }
                _ => {
                    line.extend(s.chars());
                }
            },
            MarkdownEvent::Code(s) => {
                line.extend(format!("``{}``", s).chars());
            }
            MarkdownEvent::SoftBreak => {
                lines.push(line.clone());
                line = "".to_string();
            }
            MarkdownEvent::Start(Tag::Emphasis) | MarkdownEvent::End(Tag::Emphasis) => {
                line.extend("*".chars());
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

fn struct_to_rst(docs: &Crate, path: Vec<String>, rst_prefix: &str) -> Result<Vec<String>> {
    let index = docs
        .paths
        .iter()
        .find_map(|(id, summary)| {
            if summary.kind == ItemKind::Struct && summary.path == path {
                Some(id)
            } else {
                None
            }
        })
        .ok_or_else(|| anyhow!("unable to find {:?} struct", path))?;

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
        for field_id in &inner.fields {
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
            lines.push(format!("``{}`` Field", field_name.to_string()));
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

    Ok(lines)
}

fn enum_to_rst(docs: &Crate, path: Vec<String>, rst_prefix: &str) -> Result<Vec<String>> {
    let index = docs
        .paths
        .iter()
        .find_map(|(id, summary)| {
            if summary.kind == ItemKind::Enum && summary.path == path {
                Some(id)
            } else {
                None
            }
        })
        .ok_or_else(|| anyhow!("unable to find {:?} enum", path))?;

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
    struct_paths: Vec<Vec<String>>,
    enum_paths: Vec<Vec<String>>,
) -> Result<Vec<String>> {
    let fh = std::fs::File::open(repo_root.join("target").join("doc").join(json_file))?;
    let docs: Crate = serde_json::from_reader(fh)?;

    let mut lines = vec![];

    for path in struct_paths {
        lines.extend(struct_to_rst(&docs, path, rst_prefix)?.into_iter());
        lines.push("".to_string());
    }

    for path in enum_paths {
        lines.extend(enum_to_rst(&docs, path, rst_prefix)?.into_iter());
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

        let pyembed_structs = vec![vec![
            "pyembed".to_string(),
            "OxidizedPythonInterpreterConfig".to_string(),
        ]];

        let python_packaging_structs = vec![vec![
            "python_packaging".to_string(),
            "interpreter".to_string(),
            "PythonInterpreterConfig".to_string(),
        ]];

        let python_packaging_enums = vec![
            vec![
                "python_packaging".to_string(),
                "interpreter".to_string(),
                "MemoryAllocatorBackend".to_string(),
            ],
            vec![
                "python_packaging".to_string(),
                "interpreter".to_string(),
                "PythonInterpreterProfile".to_string(),
            ],
            vec![
                "python_packaging".to_string(),
                "interpreter".to_string(),
                "Allocator".to_string(),
            ],
            vec![
                "python_packaging".to_string(),
                "resource".to_string(),
                "BytecodeOptimizationLevel".to_string(),
            ],
            vec![
                "python_packaging".to_string(),
                "interpreter".to_string(),
                "BytesWarning".to_string(),
            ],
            vec![
                "python_packaging".to_string(),
                "interpreter".to_string(),
                "CheckHashPycsMode".to_string(),
            ],
            vec![
                "python_packaging".to_string(),
                "interpreter".to_string(),
                "CoerceCLocale".to_string(),
            ],
            vec![
                "python_packaging".to_string(),
                "interpreter".to_string(),
                "MultiprocessingStartMethod".to_string(),
            ],
            vec![
                "python_packaging".to_string(),
                "interpreter".to_string(),
                "TerminfoResolution".to_string(),
            ],
        ];

        lines.push("Structs:".to_string());
        lines.push("".to_string());

        for path in pyembed_structs
            .iter()
            .chain(python_packaging_structs.iter())
        {
            let name = path
                .iter()
                .last()
                .ok_or_else(|| anyhow!("failed to resolve struct name"))?;
            lines.push(format!("* :ref:`{} <{}_struct_{}>`", name, prefix, name));
        }

        lines.push("".to_string());
        lines.push("Enums:".to_string());
        lines.push("".to_string());

        for path in python_packaging_enums.iter() {
            let name = path
                .iter()
                .last()
                .ok_or_else(|| anyhow!("failed to resolve struct name"))?;
            lines.push(format!("* :ref:`{} <{}_enum_{}>`", name, prefix, name));
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
