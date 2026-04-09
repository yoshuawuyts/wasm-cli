//! Interface detail page.

use html::content::Navigation;
use html::text_content::{Division, ListItem, UnorderedList};
use wasm_wit_doc::{FunctionDoc, InterfaceDoc, TypeDoc, TypeKind, WitDocument};

use super::sidebar::{SidebarActive, SidebarContext, render_sidebar};
use crate::layout;

/// Render the interface detail page.
#[must_use]
pub(crate) fn render(
    display_name: &str,
    version: &str,
    iface: &InterfaceDoc,
    doc: &WitDocument,
) -> String {
    let title = format!("{display_name} — {}", iface.name);
    let pkg_url = format!("/{}/{version}", display_name.replace(':', "/"));

    let mut body = Division::builder();
    body.class("pt-8");

    // Breadcrumb
    body.push(render_breadcrumb(display_name, &pkg_url, &iface.name));

    // Header
    body.division(|div| {
        div.class("mb-6")
            .heading_1(|h1| {
                h1.class("text-3xl font-bold tracking-tight font-mono")
                    .span(|s| s.class("text-fg-muted").text(format!("{display_name} / ")))
                    .span(|s| s.class("text-accent").text(iface.name.clone()))
            });
        if let Some(docs) = &iface.docs {
            div.paragraph(|p| {
                p.class("text-lg text-fg-secondary mt-2")
                    .text(docs.clone())
            });
        }
        div
    });

    // Grid: main content + sidebar
    let mut grid = Division::builder();
    grid.class("grid grid-cols-1 md:grid-cols-3 gap-12");

    // Group types by kind
    let resources: Vec<&TypeDoc> = iface
        .types
        .iter()
        .filter(|t| matches!(t.kind, TypeKind::Resource { .. }))
        .collect();
    let records: Vec<&TypeDoc> = iface
        .types
        .iter()
        .filter(|t| matches!(t.kind, TypeKind::Record { .. }))
        .collect();
    let variants: Vec<&TypeDoc> = iface
        .types
        .iter()
        .filter(|t| matches!(t.kind, TypeKind::Variant { .. }))
        .collect();
    let enums: Vec<&TypeDoc> = iface
        .types
        .iter()
        .filter(|t| matches!(t.kind, TypeKind::Enum { .. }))
        .collect();
    let flags: Vec<&TypeDoc> = iface
        .types
        .iter()
        .filter(|t| matches!(t.kind, TypeKind::Flags { .. }))
        .collect();
    let aliases: Vec<&TypeDoc> = iface
        .types
        .iter()
        .filter(|t| matches!(t.kind, TypeKind::Alias(_)))
        .collect();

    let mut content = Division::builder();
    content.class("md:col-span-2 space-y-8");

    if !resources.is_empty() {
        content.push(render_type_section("Resources", &resources));
    }
    if !records.is_empty() {
        content.push(render_type_section("Records", &records));
    }
    if !variants.is_empty() {
        content.push(render_type_section("Variants", &variants));
    }
    if !enums.is_empty() {
        content.push(render_type_section("Enums", &enums));
    }
    if !flags.is_empty() {
        content.push(render_type_section("Flags", &flags));
    }
    if !aliases.is_empty() {
        content.push(render_type_section("Type Aliases", &aliases));
    }
    if !iface.functions.is_empty() {
        content.push(render_function_section(&iface.functions));
    }

    grid.push(content.build());

    // Sidebar
    let sidebar_ctx = SidebarContext {
        display_name,
        version,
        doc,
        active: SidebarActive::Interface(&iface.name),
    };
    grid.push(render_sidebar(&sidebar_ctx));

    body.push(grid.build());

    layout::document(&title, &body.build().to_string())
}

/// Render a breadcrumb: Home / package / interface
fn render_breadcrumb(display_name: &str, pkg_url: &str, iface_name: &str) -> Navigation {
    Navigation::builder()
        .class("text-sm text-fg-muted mb-4")
        .anchor(|a| {
            a.href("/")
                .class("hover:text-accent transition-colors")
                .text("Home")
        })
        .span(|s| s.class("mx-1").text("/"))
        .anchor(|a| {
            a.href(pkg_url.to_owned())
                .class("hover:text-accent transition-colors")
                .text(display_name.to_owned())
        })
        .span(|s| s.class("mx-1").text("/"))
        .span(|s| s.class("text-fg font-medium").text(iface_name.to_owned()))
        .build()
}

/// Render a section of types grouped by kind.
fn render_type_section(heading: &str, types: &[&TypeDoc]) -> Division {
    let mut div = Division::builder();
    div.heading_2(|h2| {
        h2.class("text-sm font-semibold text-fg-muted uppercase tracking-wide mb-3").text(heading.to_owned())
    });

    let mut ul = UnorderedList::builder();
    ul.class("space-y-2");
    for ty in types {
        ul.push(render_type_row(ty));
    }
    div.push(ul.build());
    div.build()
}

/// Render a single type row as a linked list item.
fn render_type_row(ty: &TypeDoc) -> ListItem {
    let summary = type_kind_summary(&ty.kind);

    let mut li = ListItem::builder();
    li.class(
        "border border-border rounded-lg px-4 py-3 \
         hover:border-accent/50 transition-colors",
    );
    li.anchor(|a| {
        a.href(ty.url.clone())
            .class("block group")
            .division(|div| {
                div.class("flex items-baseline gap-2")
                    .span(|s| {
                        s.class(
                            "font-mono font-semibold text-accent \
                             group-hover:underline",
                        )
                        .text(ty.name.clone())
                    })
                    .span(|s| {
                        s.class("text-xs text-fg-secondary bg-surface-muted px-1.5 py-0.5 rounded").text(summary)
                    })
            });
        if let Some(docs) = &ty.docs {
            a.paragraph(|p| {
                p.class("text-sm text-fg-secondary mt-1 line-clamp-2")
                    .text(first_sentence(docs))
            });
        }
        a
    });
    li.build()
}

/// Render the freestanding functions section.
fn render_function_section(functions: &[FunctionDoc]) -> Division {
    let mut div = Division::builder();
    div.heading_2(|h2| {
        h2.class("text-sm font-semibold text-fg-muted uppercase tracking-wide mb-3").text("Functions")
    });

    let mut ul = UnorderedList::builder();
    ul.class("space-y-2");
    for func in functions {
        ul.push(render_function_row(func));
    }
    div.push(ul.build());
    div.build()
}

/// Render a single function row with its full signature visible.
fn render_function_row(func: &FunctionDoc) -> ListItem {
    let mut li = ListItem::builder();
    li.class(
        "border border-border rounded-lg px-4 py-3 \
         hover:border-accent/50 transition-colors",
    );
    li.anchor(|a| {
        a.href(func.url.clone())
            .class("block group")
            .division(|div| {
                div.push(
                    html::text_content::PreformattedText::builder()
                        .class("font-mono text-sm text-fg group-hover:text-accent transition-colors overflow-x-auto")
                        .code(|c| {
                            c.span(|s| s.class("text-accent font-semibold group-hover:underline").text(func.name.clone()))
                             .text(escape_html(&format!("({})", format_params(func))))
                             .text(escape_html(&format_return(func)))
                        })
                        .build()
                )
            });
        if let Some(docs) = &func.docs {
            a.paragraph(|p| {
                p.class("text-sm text-fg-secondary mt-1 line-clamp-2")
                    .text(first_sentence(docs))
            });
        }
        a
    });
    li.build()
}

/// Get a short summary string for a type kind.
fn type_kind_summary(kind: &TypeKind) -> String {
    match kind {
        TypeKind::Record { fields } => format!(
            "{} {}",
            fields.len(),
            if fields.len() == 1 { "field" } else { "fields" }
        ),
        TypeKind::Variant { cases } => format!(
            "{} {}",
            cases.len(),
            if cases.len() == 1 { "case" } else { "cases" }
        ),
        TypeKind::Enum { cases } => format!(
            "{} {}",
            cases.len(),
            if cases.len() == 1 { "case" } else { "cases" }
        ),
        TypeKind::Flags { flags } => format!(
            "{} {}",
            flags.len(),
            if flags.len() == 1 { "flag" } else { "flags" }
        ),
        TypeKind::Resource {
            methods, statics, ..
        } => {
            let total = methods.len() + statics.len();
            format!(
                "{total} {}",
                if total == 1 { "method" } else { "methods" }
            )
        }
        TypeKind::Alias(_) => "alias".to_owned(),
    }
}

/// Format function parameters as a comma-separated string.
fn format_params(func: &FunctionDoc) -> String {
    func.params
        .iter()
        .filter(|p| p.name != "self")
        .map(|p| format!("{}: {}", p.name, format_type_ref_short(&p.ty)))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Format the return type as ` -> type` or empty string.
fn format_return(func: &FunctionDoc) -> String {
    func.result
        .as_ref()
        .map(|r| format!(" -> {}", format_type_ref_short(r)))
        .unwrap_or_default()
}

/// Format a `TypeRef` as a short inline string.
fn format_type_ref_short(ty: &wasm_wit_doc::TypeRef) -> String {
    match ty {
        wasm_wit_doc::TypeRef::Primitive { name }
        | wasm_wit_doc::TypeRef::Named { name, .. } => name.clone(),
        wasm_wit_doc::TypeRef::List { ty } => {
            format!("list<{}>", format_type_ref_short(ty))
        }
        wasm_wit_doc::TypeRef::Option { ty } => {
            format!("option<{}>", format_type_ref_short(ty))
        }
        wasm_wit_doc::TypeRef::Result { ok, err } => {
            let ok_str = ok
                .as_ref()
                .map_or_else(|| "_".to_owned(), |t| format_type_ref_short(t));
            let err_str = err
                .as_ref()
                .map_or_else(|| "_".to_owned(), |t| format_type_ref_short(t));
            format!("result<{ok_str}, {err_str}>")
        }
        wasm_wit_doc::TypeRef::Tuple { types } => {
            let inner: Vec<String> = types.iter().map(format_type_ref_short).collect();
            format!("tuple<{}>", inner.join(", "))
        }
        wasm_wit_doc::TypeRef::Handle {
            handle_kind,
            resource_name,
            ..
        } => match handle_kind {
            wasm_wit_doc::HandleKind::Own => resource_name.clone(),
            wasm_wit_doc::HandleKind::Borrow => format!("borrow<{resource_name}>"),
        },
        wasm_wit_doc::TypeRef::Future { ty } => match ty {
            Some(t) => format!("future<{}>", format_type_ref_short(t)),
            None => "future".to_owned(),
        },
        wasm_wit_doc::TypeRef::Stream { ty } => match ty {
            Some(t) => format!("stream<{}>", format_type_ref_short(t)),
            None => "stream".to_owned(),
        },
    }
}

/// Extract the first sentence from a doc comment.
fn first_sentence(text: &str) -> String {
    text.split_once(". ")
        .map_or_else(|| text.to_owned(), |(first, _)| format!("{first}."))
}

/// Escape HTML special characters in code text.
fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
