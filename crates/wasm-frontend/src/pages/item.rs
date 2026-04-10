//! Item detail page (type or function within an interface).

use crate::wit_doc::{FunctionDoc, HandleKind, TypeDoc, TypeKind, TypeRef, WitDocument};
use html::tables::{Table, TableRow};
use html::text_content::Division;
use wasm_meta_registry_client::{KnownPackage, PackageVersion};

use super::package_shell;

/// Render the item detail page for a type.
#[must_use]
pub(crate) fn render_type(
    pkg: &KnownPackage,
    version: &str,
    version_detail: Option<&PackageVersion>,
    iface_name: &str,
    ty: &TypeDoc,
    _doc: &WitDocument,
) -> String {
    let display_name = package_shell::display_name_for(pkg);
    let title = format!("{display_name} \u{2014} {iface_name}::{}", ty.name);

    let mut outer = Division::builder();

    // WIT definition block
    outer.push(render_type_definition(ty));

    // Type body content
    outer.push(render_type_body(&ty.kind));

    let ctx = package_shell::SidebarContext {
        pkg,
        version,
        version_detail,
        importers: &[],
        exporters: &[],
        description_override: Some(""),
    };
    let pkg_url = format!("/{}/{version}", display_name.replace(':', "/"));
    let extra = vec![
        crate::nav::Crumb {
            label: iface_name.to_owned(),
            href: Some(format!("{pkg_url}/interface/{iface_name}")),
        },
        crate::nav::Crumb {
            label: ty.name.clone(),
            href: None,
        },
    ];
    package_shell::render_page_with_crumbs(&ctx, &title, outer.build(), extra)
}

/// Render the item detail page for a freestanding function.
#[must_use]
pub(crate) fn render_function(
    pkg: &KnownPackage,
    version: &str,
    version_detail: Option<&PackageVersion>,
    iface_name: &str,
    func: &FunctionDoc,
    _doc: &WitDocument,
) -> String {
    let display_name = package_shell::display_name_for(pkg);
    let title = format!("{display_name} \u{2014} {iface_name}::{}", func.name);

    let mut outer = Division::builder();

    // WIT definition block
    outer.push(render_function_definition(func));

    // Function detail content
    outer.push(render_function_detail(func));

    let ctx = package_shell::SidebarContext {
        pkg,
        version,
        version_detail,
        importers: &[],
        exporters: &[],
        description_override: Some(""),
    };
    let pkg_url = format!("/{}/{version}", display_name.replace(':', "/"));
    let extra = vec![
        crate::nav::Crumb {
            label: iface_name.to_owned(),
            href: Some(format!("{pkg_url}/interface/{iface_name}")),
        },
        crate::nav::Crumb {
            label: func.name.clone(),
            href: None,
        },
    ];
    package_shell::render_page_with_crumbs(&ctx, &title, outer.build(), extra)
}

/// Render the WIT definition code block for a type, with linked type refs.
fn render_type_definition(ty: &TypeDoc) -> Division {
    let pre_class = "border-2 border-fg px-4 py-3 text-sm font-mono text-fg overflow-x-auto";

    Division::builder()
        .class("mb-6")
        .push(match &ty.kind {
            TypeKind::Record { fields } => {
                let mut pre = html::text_content::PreformattedText::builder();
                pre.class(pre_class);
                pre.code(|c| {
                    c.span(|s| s.class("text-wit-struct").text("record "))
                        .span(|s| s.class("font-medium").text(ty.name.clone()))
                        .text(" {\n".to_owned());
                    for f in fields {
                        c.text("    ".to_owned())
                            .text(format!("{}: ", f.name))
                            .push(render_type_ref(&f.ty))
                            .text(",\n".to_owned());
                    }
                    c.text("}".to_owned())
                });
                pre.build()
            }
            TypeKind::Variant { cases } => {
                let mut pre = html::text_content::PreformattedText::builder();
                pre.class(pre_class);
                pre.code(|c| {
                    c.span(|s| s.class("text-wit-struct").text("variant "))
                        .span(|s| s.class("font-medium").text(ty.name.clone()))
                        .text(" {\n".to_owned());
                    for case in cases {
                        c.text(format!("    {}", case.name));
                        if let Some(t) = &case.ty {
                            c.text("(".to_owned())
                                .push(render_type_ref(t))
                                .text(")".to_owned());
                        }
                        c.text(",\n".to_owned());
                    }
                    c.text("}".to_owned())
                });
                pre.build()
            }
            TypeKind::Enum { cases } => {
                let mut pre = html::text_content::PreformattedText::builder();
                pre.class(pre_class);
                pre.code(|c| {
                    c.span(|s| s.class("text-wit-enum").text("enum "))
                        .span(|s| s.class("font-medium").text(ty.name.clone()))
                        .text(" {\n".to_owned());
                    for case in cases {
                        c.text(format!("    {},\n", case.name));
                    }
                    c.text("}".to_owned())
                });
                pre.build()
            }
            TypeKind::Flags { flags } => {
                let mut pre = html::text_content::PreformattedText::builder();
                pre.class(pre_class);
                pre.code(|c| {
                    c.span(|s| s.class("text-wit-enum").text("flags "))
                        .span(|s| s.class("font-medium").text(ty.name.clone()))
                        .text(" {\n".to_owned());
                    for f in flags {
                        c.text(format!("    {},\n", f.name));
                    }
                    c.text("}".to_owned())
                });
                pre.build()
            }
            TypeKind::Resource { .. } => html::text_content::PreformattedText::builder()
                .class(pre_class)
                .code(|c| {
                    c.span(|s| s.class("text-wit-resource").text("resource "))
                        .span(|s| s.class("font-medium").text(ty.name.clone()))
                        .text(";".to_owned())
                })
                .build(),
            TypeKind::Alias(type_ref) => html::text_content::PreformattedText::builder()
                .class(pre_class)
                .code(|c| {
                    c.span(|s| s.class("text-accent").text("type "))
                        .span(|s| s.class("font-medium").text(ty.name.clone()))
                        .text(" = ".to_owned())
                        .push(render_type_ref(type_ref))
                        .text(";".to_owned())
                })
                .build(),
        })
        .build()
}

/// Render the WIT definition code block for a function, with linked type refs.
fn render_function_definition(func: &FunctionDoc) -> Division {
    let pre_class = "border-2 border-fg px-4 py-3 text-sm font-mono text-fg overflow-x-auto";

    Division::builder()
        .class("mb-6")
        .push(
            html::text_content::PreformattedText::builder()
                .class(pre_class)
                .code(|c| {
                    c.span(|s| s.class("font-medium").text(func.name.clone()))
                        .text(": ".to_owned())
                        .span(|s| s.class("text-wit-func").text("func"))
                        .text("(".to_owned());
                    let visible_params: Vec<_> =
                        func.params.iter().filter(|p| p.name != "self").collect();
                    for (i, p) in visible_params.iter().enumerate() {
                        if i > 0 {
                            c.text(", ".to_owned());
                        }
                        c.text(format!("{}: ", p.name)).push(render_type_ref(&p.ty));
                    }
                    c.text(")".to_owned());
                    if let Some(ret) = &func.result {
                        c.text(" -> ".to_owned()).push(render_type_ref(ret));
                    }
                    c.text(";".to_owned())
                })
                .build(),
        )
        .build()
}

/// Render the body for a type based on its kind.
fn render_type_body(kind: &TypeKind) -> Division {
    match kind {
        TypeKind::Record { fields } => render_field_table("Fields", fields),
        TypeKind::Variant { cases } => render_variant_table(cases),
        TypeKind::Enum { cases } => render_enum_list(cases),
        TypeKind::Flags { flags } => render_flags_list(flags),
        TypeKind::Resource {
            constructor,
            methods,
            statics,
        } => render_resource_body(constructor.as_ref(), methods, statics),
        TypeKind::Alias(type_ref) => render_alias(type_ref),
    }
}

/// Render a table of record fields.
fn render_field_table(heading: &str, fields: &[crate::wit_doc::FieldDoc]) -> Division {
    let mut div = Division::builder();
    div.heading_2(|h2| {
        h2.class("text-sm font-medium text-fg-muted uppercase tracking-wide mb-3")
            .text(heading.to_owned())
    });

    let mut table = Table::builder();
    table.class("w-full text-sm");
    table.table_row(|tr| {
        tr.class("border-b-2 border-fg text-left text-fg-muted")
            .table_header(|th| th.class("py-2 pr-4 font-medium").text("Name"))
            .table_header(|th| th.class("py-2 pr-4 font-medium").text("Type"))
            .table_header(|th| th.class("py-2 font-medium").text("Description"))
    });
    for field in fields {
        table.push(render_field_row(
            &field.name,
            &field.ty,
            field.docs.as_deref(),
        ));
    }
    div.push(table.build());
    div.build()
}

/// Render a single field/param row.
fn render_field_row(name: &str, ty: &TypeRef, docs: Option<&str>) -> TableRow {
    TableRow::builder()
        .class("border-b-2 border-fg/50")
        .table_cell(|td| {
            td.class("py-2 pr-4 font-mono text-accent")
                .text(name.to_owned())
        })
        .table_cell(|td| {
            td.class("py-2 pr-4 font-mono text-fg")
                .push(render_type_ref(ty))
        })
        .table_cell(|td| {
            td.class("py-2 text-fg-secondary")
                .text(docs.unwrap_or("").to_owned())
        })
        .build()
}

/// Render a variant cases table.
fn render_variant_table(cases: &[crate::wit_doc::CaseDoc]) -> Division {
    let mut div = Division::builder();
    div.heading_2(|h2| {
        h2.class("text-sm font-medium text-fg-muted uppercase tracking-wide mb-3")
            .text("Cases")
    });

    let mut table = Table::builder();
    table.class("w-full text-sm");
    table.table_row(|tr| {
        tr.class("border-b-2 border-fg text-left text-fg-muted")
            .table_header(|th| th.class("py-2 pr-4 font-medium").text("Case"))
            .table_header(|th| th.class("py-2 pr-4 font-medium").text("Payload"))
            .table_header(|th| th.class("py-2 font-medium").text("Description"))
    });
    for case in cases {
        let payload = case
            .ty
            .as_ref()
            .map_or_else(|| "—".to_owned(), format_type_ref_short);
        table.table_row(|tr| {
            tr.class("border-b-2 border-fg/50")
                .table_cell(|td| {
                    td.class("py-2 pr-4 font-mono text-accent")
                        .text(case.name.clone())
                })
                .table_cell(|td| td.class("py-2 pr-4 font-mono text-fg").text(payload))
                .table_cell(|td| {
                    td.class("py-2 text-fg-secondary")
                        .text(case.docs.clone().unwrap_or_default())
                })
        });
    }
    div.push(table.build());
    div.build()
}

/// Render an enum cases list.
fn render_enum_list(cases: &[crate::wit_doc::EnumCaseDoc]) -> Division {
    let mut div = Division::builder();
    div.heading_2(|h2| {
        h2.class("text-sm font-medium text-fg-muted uppercase tracking-wide mb-3")
            .text("Cases")
    });
    let mut table = Table::builder();
    table.class("w-full text-sm");
    table.table_row(|tr| {
        tr.class("border-b-2 border-fg text-left text-fg-muted")
            .table_header(|th| th.class("py-2 pr-4 font-medium").text("Case"))
            .table_header(|th| th.class("py-2 font-medium").text("Description"))
    });
    for case in cases {
        table.table_row(|tr| {
            tr.class("border-b-2 border-fg/50")
                .table_cell(|td| {
                    td.class("py-2 pr-4 font-mono text-accent")
                        .text(case.name.clone())
                })
                .table_cell(|td| {
                    td.class("py-2 text-fg-secondary")
                        .text(case.docs.clone().unwrap_or_default())
                })
        });
    }
    div.push(table.build());
    div.build()
}

/// Render a flags list.
fn render_flags_list(flags: &[crate::wit_doc::FlagDoc]) -> Division {
    let mut div = Division::builder();
    div.heading_2(|h2| {
        h2.class("text-sm font-medium text-fg-muted uppercase tracking-wide mb-3")
            .text("Flags")
    });
    let mut table = Table::builder();
    table.class("w-full text-sm");
    table.table_row(|tr| {
        tr.class("border-b-2 border-fg text-left text-fg-muted")
            .table_header(|th| th.class("py-2 pr-4 font-medium").text("Flag"))
            .table_header(|th| th.class("py-2 font-medium").text("Description"))
    });
    for flag in flags {
        table.table_row(|tr| {
            tr.class("border-b-2 border-fg/50")
                .table_cell(|td| {
                    td.class("py-2 pr-4 font-mono text-accent")
                        .text(flag.name.clone())
                })
                .table_cell(|td| {
                    td.class("py-2 text-fg-secondary")
                        .text(flag.docs.clone().unwrap_or_default())
                })
        });
    }
    div.push(table.build());
    div.build()
}

/// Render a resource body with constructor, methods, and statics.
fn render_resource_body(
    constructor: Option<&FunctionDoc>,
    methods: &[FunctionDoc],
    statics: &[FunctionDoc],
) -> Division {
    let mut div = Division::builder();
    div.class("space-y-6");

    if let Some(ctor) = constructor {
        div.division(|d| {
            d.heading_2(|h2| {
                h2.class("text-sm font-medium text-fg-muted uppercase tracking-wide mb-3")
                    .text("Constructor")
            })
            .push(render_function_detail(ctor))
        });
    }
    if !methods.is_empty() {
        div.division(|d| {
            d.heading_2(|h2| {
                h2.class("text-sm font-medium text-fg-muted uppercase tracking-wide mb-3")
                    .text("Methods")
            });
            for func in methods {
                d.push(render_function_detail(func));
            }
            d
        });
    }
    if !statics.is_empty() {
        div.division(|d| {
            d.heading_2(|h2| {
                h2.class("text-sm font-medium text-fg-muted uppercase tracking-wide mb-3")
                    .text("Static Functions")
            });
            for func in statics {
                d.push(render_function_detail(func));
            }
            d
        });
    }

    div.build()
}

/// Render a type alias.
fn render_alias(type_ref: &TypeRef) -> Division {
    Division::builder()
        .heading_2(|h2| {
            h2.class("text-sm font-medium text-fg-muted uppercase tracking-wide mb-3")
                .text("Definition")
        })
        .paragraph(|p| p.class("font-mono text-fg").push(render_type_ref(type_ref)))
        .build()
}

/// Render function detail: signature + param table.
fn render_function_detail(func: &FunctionDoc) -> Division {
    let mut div = Division::builder();
    div.class("mb-6");

    // Param table (skip `self`)
    let visible_params: Vec<_> = func.params.iter().filter(|p| p.name != "self").collect();
    if !visible_params.is_empty() {
        let mut table = Table::builder();
        table.class("w-full text-sm mt-3");
        table.table_row(|tr| {
            tr.class("border-b-2 border-fg text-left text-fg-muted")
                .table_header(|th| th.class("py-2 pr-4 font-medium").text("Parameter"))
                .table_header(|th| th.class("py-2 font-medium").text("Type"))
        });
        for param in &visible_params {
            table.table_row(|tr| {
                tr.class("border-b-2 border-fg/50")
                    .table_cell(|td| {
                        td.class("py-2 pr-4 font-mono text-accent")
                            .text(param.name.clone())
                    })
                    .table_cell(|td| {
                        td.class("py-2 font-mono text-fg")
                            .push(render_type_ref(&param.ty))
                    })
            });
        }
        div.push(table.build());
    }

    // Return type
    if let Some(ret) = &func.result {
        div.division(|d| {
            d.class("mt-3 text-sm")
                .span(|s| s.class("text-fg-muted").text("Returns: "))
                .span(|s| s.class("font-mono text-fg").push(render_type_ref(ret)))
        });
    }

    div.build()
}

/// Render a `TypeRef` as an inline HTML span with links.
fn render_type_ref(ty: &TypeRef) -> html::inline_text::Span {
    let mut span = html::inline_text::Span::builder();
    match ty {
        TypeRef::Primitive { name } => {
            span.class("text-fg-muted").text(name.clone());
        }
        TypeRef::Named {
            name,
            url: Some(url),
        } => {
            span.anchor(|a| {
                a.href(url.clone())
                    .class("text-accent hover:underline")
                    .text(name.clone())
            });
        }
        TypeRef::Named { name, url: None } => {
            span.text(name.clone());
        }
        TypeRef::List { ty } => {
            span.text("list\u{200b}<".to_owned())
                .push(render_type_ref(ty))
                .text(">".to_owned());
        }
        TypeRef::Option { ty } => {
            span.text("option\u{200b}<".to_owned())
                .push(render_type_ref(ty))
                .text(">".to_owned());
        }
        TypeRef::Result { ok, err } => {
            span.text("result\u{200b}<".to_owned());
            if let Some(ok) = ok {
                span.push(render_type_ref(ok));
            } else {
                span.text("_".to_owned());
            }
            span.text(", ".to_owned());
            if let Some(err) = err {
                span.push(render_type_ref(err));
            } else {
                span.text("_".to_owned());
            }
            span.text(">".to_owned());
        }
        TypeRef::Tuple { types } => {
            span.text("tuple\u{200b}<".to_owned());
            for (i, t) in types.iter().enumerate() {
                if i > 0 {
                    span.text(", ".to_owned());
                }
                span.push(render_type_ref(t));
            }
            span.text(">".to_owned());
        }
        TypeRef::Handle {
            handle_kind,
            resource_name,
            resource_url,
        } => match handle_kind {
            HandleKind::Own => {
                if let Some(url) = resource_url {
                    span.anchor(|a| {
                        a.href(url.clone())
                            .class("text-accent hover:underline")
                            .text(resource_name.clone())
                    });
                } else {
                    span.text(resource_name.clone());
                }
            }
            HandleKind::Borrow => {
                span.text("borrow\u{200b}<".to_owned());
                if let Some(url) = resource_url {
                    span.anchor(|a| {
                        a.href(url.clone())
                            .class("text-accent hover:underline")
                            .text(resource_name.clone())
                    });
                } else {
                    span.text(resource_name.clone());
                }
                span.text(">".to_owned());
            }
        },
        TypeRef::Future { ty } => match ty {
            Some(t) => {
                span.text("future\u{200b}<".to_owned())
                    .push(render_type_ref(t))
                    .text(">".to_owned());
            }
            None => {
                span.text("future".to_owned());
            }
        },
        TypeRef::Stream { ty } => match ty {
            Some(t) => {
                span.text("stream\u{200b}<".to_owned())
                    .push(render_type_ref(t))
                    .text(">".to_owned());
            }
            None => {
                span.text("stream".to_owned());
            }
        },
    }
    span.build()
}

/// Format a `TypeRef` as a short inline string (no links).
fn format_type_ref_short(ty: &TypeRef) -> String {
    match ty {
        TypeRef::Primitive { name } | TypeRef::Named { name, .. } => name.clone(),
        TypeRef::List { ty } => format!("list<{}>", format_type_ref_short(ty)),
        TypeRef::Option { ty } => format!("option<{}>", format_type_ref_short(ty)),
        TypeRef::Result { ok, err } => {
            let ok_s = ok
                .as_ref()
                .map_or_else(|| "_".to_owned(), |t| format_type_ref_short(t));
            let err_s = err
                .as_ref()
                .map_or_else(|| "_".to_owned(), |t| format_type_ref_short(t));
            format!("result<{ok_s}, {err_s}>")
        }
        TypeRef::Tuple { types } => {
            let inner: Vec<String> = types.iter().map(format_type_ref_short).collect();
            format!("tuple<{}>", inner.join(", "))
        }
        TypeRef::Handle {
            handle_kind,
            resource_name,
            ..
        } => match handle_kind {
            HandleKind::Own => resource_name.clone(),
            HandleKind::Borrow => format!("borrow<{resource_name}>"),
        },
        TypeRef::Future { ty } => match ty {
            Some(t) => format!("future<{}>", format_type_ref_short(t)),
            None => "future".to_owned(),
        },
        TypeRef::Stream { ty } => match ty {
            Some(t) => format!("stream<{}>", format_type_ref_short(t)),
            None => "stream".to_owned(),
        },
    }
}
