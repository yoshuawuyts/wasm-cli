//! Item detail page (type or function within an interface).

use crate::wit_doc::{FunctionDoc, TypeDoc, TypeKind, TypeRef, WitDocument};
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
    let fqn = format!("{display_name}/{iface_name}/{}", ty.name);

    let kind_label = type_kind_label(&ty.kind);

    // Code block
    let code_block = render_type_definition(ty).to_string();

    // Description
    let docs_html = ty
        .docs
        .as_deref()
        .map(|docs| crate::markdown::render_block(docs, crate::markdown::DOC_CLASS))
        .unwrap_or_default();

    let copy_icon = "<svg xmlns='http://www.w3.org/2000/svg' width='14' height='14' viewBox='0 0 24 24' fill='none' stroke='currentColor' stroke-width='2' stroke-linecap='round' stroke-linejoin='round'><rect x='9' y='9' width='13' height='13' rx='2' ry='2'/><path d='M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1'/></svg>";
    let check_icon = "<svg xmlns='http://www.w3.org/2000/svg' width='14' height='14' viewBox='0 0 24 24' fill='none' stroke='currentColor' stroke-width='2' stroke-linecap='round' stroke-linejoin='round'><polyline points='20 6 9 17 4 12'/></svg>";

    // Header row: name on left, docs on right
    let header = format!(
        r#"<div class="max-w-3xl mb-6">
  <h2 class="text-3xl font-light tracking-display font-display flex items-baseline gap-2 group">
    <span class="{kind_color}">{name}</span>
    <button id="copy-fqn-btn" class="text-fg-faint hover:text-fg transition-opacity cursor-pointer opacity-0 group-hover:opacity-100" style="font-size:0.5em;vertical-align:middle" title="Copy item path to clipboard">{copy_icon}</button>
  </h2>
  <span class="text-sm text-fg-muted mt-1 block">{kind_label}</span>
  <div class="mt-4">
    {code_block}
    {docs_html}
  </div>
</div>
<script>
(function(){{
  var btn=document.getElementById('copy-fqn-btn');
  var copyIcon="{copy_icon}";
  var checkIcon="{check_icon}";
  btn.addEventListener('click',function(){{
    navigator.clipboard.writeText('{fqn}').then(function(){{
      btn.innerHTML=checkIcon;
      setTimeout(function(){{btn.innerHTML=copyIcon}},2000);
    }});
  }});
}})();
</script>"#,
        kind_color = type_kind_color(&ty.kind),
        name = ty.name,
    );

    // Type body content (fields, variants, etc.)
    let body = render_type_body(&ty.kind).to_string();

    let content = format!("{header}<div class=\"max-w-3xl\">{body}</div>");

    let ctx = package_shell::SidebarContext {
        pkg,
        version,
        version_detail,
        importers: &[],
        exporters: &[],
    };
    let iface_url = format!(
        "/{}/{version}/interface/{iface_name}",
        display_name.replace(':', "/")
    );
    let extra = vec![crate::nav::Crumb {
        label: iface_name.to_owned(),
        href: Some(iface_url),
    }];
    package_shell::render_page_with_crumbs(&ctx, &title, &content, &extra)
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
    let fqn = format!("{display_name}/{iface_name}/{}", func.name);

    // Code block
    let code_block = render_function_definition(func).to_string();

    // Description
    let docs_html = func
        .docs
        .as_deref()
        .map(|docs| crate::markdown::render_block(docs, crate::markdown::DOC_CLASS))
        .unwrap_or_default();

    let copy_icon = "<svg xmlns='http://www.w3.org/2000/svg' width='14' height='14' viewBox='0 0 24 24' fill='none' stroke='currentColor' stroke-width='2' stroke-linecap='round' stroke-linejoin='round'><rect x='9' y='9' width='13' height='13' rx='2' ry='2'/><path d='M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1'/></svg>";
    let check_icon = "<svg xmlns='http://www.w3.org/2000/svg' width='14' height='14' viewBox='0 0 24 24' fill='none' stroke='currentColor' stroke-width='2' stroke-linecap='round' stroke-linejoin='round'><polyline points='20 6 9 17 4 12'/></svg>";

    // Header row: name on left, docs on right
    let header = format!(
        r#"<div class="max-w-3xl mb-6">
  <h2 class="text-3xl font-light tracking-display font-display flex items-baseline gap-2 group">
    <span class="text-wit-func">{name}</span>
    <button id="copy-fqn-btn" class="text-fg-faint hover:text-fg transition-opacity cursor-pointer opacity-0 group-hover:opacity-100" style="font-size:0.5em;vertical-align:middle" title="Copy item path to clipboard">{copy_icon}</button>
  </h2>
  <span class="text-sm text-fg-muted mt-1 block">Function</span>
  <div class="mt-4">
    {code_block}
    {docs_html}
  </div>
</div>
<script>
(function(){{
  var btn=document.getElementById('copy-fqn-btn');
  var copyIcon="{copy_icon}";
  var checkIcon="{check_icon}";
  btn.addEventListener('click',function(){{
    navigator.clipboard.writeText('{fqn}').then(function(){{
      btn.innerHTML=checkIcon;
      setTimeout(function(){{btn.innerHTML=copyIcon}},2000);
    }});
  }});
}})();
</script>"#,
        name = func.name,
    );

    let content = header;

    let ctx = package_shell::SidebarContext {
        pkg,
        version,
        version_detail,
        importers: &[],
        exporters: &[],
    };
    let iface_url = format!(
        "/{}/{version}/interface/{iface_name}",
        display_name.replace(':', "/")
    );
    let extra = vec![crate::nav::Crumb {
        label: iface_name.to_owned(),
        href: Some(iface_url),
    }];
    package_shell::render_page_with_crumbs(&ctx, &title, &content, &extra)
}

/// Get the display label for a type kind.
fn type_kind_label(kind: &TypeKind) -> &'static str {
    match kind {
        TypeKind::Record { .. } => "Record",
        TypeKind::Variant { .. } => "Variant",
        TypeKind::Enum { .. } => "Enum",
        TypeKind::Flags { .. } => "Flags",
        TypeKind::Resource { .. } => "Resource",
        TypeKind::Alias(_) => "Type",
    }
}

/// Get the CSS color class for a type kind heading.
fn type_kind_color(kind: &TypeKind) -> &'static str {
    match kind {
        TypeKind::Record { .. } | TypeKind::Variant { .. } => "text-wit-struct",
        TypeKind::Enum { .. } | TypeKind::Flags { .. } => "text-wit-enum",
        TypeKind::Resource { .. } => "text-wit-resource",
        TypeKind::Alias(_) => "text-accent",
    }
}

/// Render the WIT definition code block for a type, with linked type refs.
fn render_type_definition(ty: &TypeDoc) -> Division {
    use super::wit_render::{self, CODE_BLOCK_CLASS};

    Division::builder()
        .class("mb-4")
        .push(
            html::text_content::PreformattedText::builder()
                .class(CODE_BLOCK_CLASS)
                .code(|c| {
                    wit_render::render_type_in_code(c, ty, "");
                    c
                })
                .build(),
        )
        .build()
}

/// Render the WIT definition code block for a function, with linked type refs.
fn render_function_definition(func: &FunctionDoc) -> Division {
    use super::wit_render::{self, CODE_BLOCK_CLASS};

    Division::builder()
        .class("mb-4")
        .push(
            html::text_content::PreformattedText::builder()
                .class(CODE_BLOCK_CLASS)
                .code(|c| {
                    wit_render::render_func_in_code(c, func, "");
                    c
                })
                .build(),
        )
        .build()
}

/// Render a function signature inline (no border/box), like docs.rs style.
fn render_function_signature(func: &FunctionDoc) -> Division {
    use super::wit_render;

    Division::builder()
        .class("mb-2 bg-surface px-3 py-2")
        .push(
            html::text_content::PreformattedText::builder()
                .class("text-base font-mono text-fg overflow-x-auto")
                .code(|c| {
                    wit_render::render_func_in_code(c, func, "");
                    c
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
        h2.class("text-lg font-medium text-fg-muted mb-3")
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
                .push(super::wit_render::render_type_ref(ty))
        })
        .table_cell(|td| {
            td.class("py-2 text-fg-secondary")
                .text(crate::markdown::render_inline(docs.unwrap_or("")))
        })
        .build()
}

/// Render a variant cases table.
fn render_variant_table(cases: &[crate::wit_doc::CaseDoc]) -> Division {
    let mut div = Division::builder();
    div.heading_2(|h2| {
        h2.class("text-lg font-medium text-fg-muted mb-3")
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
        table.table_row(|tr| {
            tr.class("border-b-2 border-fg/50")
                .table_cell(|td| {
                    td.class("py-2 pr-4 font-mono text-accent")
                        .text(case.name.clone())
                })
                .table_cell(|td| {
                    td.class("py-2 pr-4 font-mono text-fg");
                    if let Some(t) = &case.ty {
                        td.push(super::wit_render::render_type_ref(t));
                    } else {
                        td.text("\u{2014}".to_owned());
                    }
                    td
                })
                .table_cell(|td| {
                    td.class("py-2 text-fg-secondary")
                        .text(crate::markdown::render_inline(
                            case.docs.as_deref().unwrap_or(""),
                        ))
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
        h2.class("text-lg font-medium text-fg-muted mb-3")
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
                        .text(crate::markdown::render_inline(
                            case.docs.as_deref().unwrap_or(""),
                        ))
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
        h2.class("text-lg font-medium text-fg-muted mb-3")
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
                        .text(crate::markdown::render_inline(
                            flag.docs.as_deref().unwrap_or(""),
                        ))
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
                h2.class("text-lg font-medium text-fg-muted mb-3")
                    .text("Constructor")
            })
            .push(render_function_signature(ctor));
            if let Some(docs) = &ctor.docs {
                d.text(crate::markdown::render_block(
                    docs,
                    "text-base text-fg-secondary leading-relaxed prose-doc",
                ));
            }
            d
        });
    }
    if !methods.is_empty() {
        div.division(|d| {
            d.heading_2(|h2| {
                h2.class("text-lg font-medium text-fg-muted mb-3")
                    .text("Methods")
            });
            for func in methods {
                d.division(|m| {
                    m.class("py-3 border-b border-border-light");
                    m.push(render_function_signature(func));
                    if let Some(docs) = &func.docs {
                        m.text(crate::markdown::render_block(
                            docs,
                            "text-base text-fg-secondary leading-relaxed prose-doc",
                        ));
                    }
                    m
                });
            }
            d
        });
    }
    if !statics.is_empty() {
        div.division(|d| {
            d.heading_2(|h2| {
                h2.class("text-lg font-medium text-fg-muted mb-3")
                    .text("Static Functions")
            });
            for func in statics {
                d.division(|m| {
                    m.class("py-3 border-b border-border-light");
                    m.push(render_function_signature(func));
                    if let Some(docs) = &func.docs {
                        m.text(crate::markdown::render_block(
                            docs,
                            "text-base text-fg-secondary leading-relaxed prose-doc",
                        ));
                    }
                    m
                });
            }
            d
        });
    }

    div.build()
}

/// Render a type alias (no-op — the code block already shows the definition).
fn render_alias(_type_ref: &TypeRef) -> Division {
    Division::builder().build()
}
