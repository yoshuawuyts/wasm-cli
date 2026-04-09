//! World detail page.

use html::content::Navigation;
use html::text_content::{Division, ListItem, UnorderedList};
use wasm_wit_doc::{WitDocument, WorldDoc, WorldItemDoc};

use super::sidebar::{SidebarActive, SidebarContext, render_sidebar};
use crate::layout;

/// Render the world detail page.
#[must_use]
pub(crate) fn render(
    display_name: &str,
    version: &str,
    world: &WorldDoc,
    doc: &WitDocument,
) -> String {
    let title = format!("{display_name} — {}", world.name);
    let pkg_url = format!("/{}/{version}", display_name.replace(':', "/"));

    let mut body = Division::builder();
    body.class("pt-8");

    body.push(render_breadcrumb(display_name, &pkg_url, &world.name));

    // Header
    body.division(|div| {
        div.class("mb-6")
            .heading_1(|h1| {
                h1.class("text-3xl font-bold tracking-tight font-mono")
                    .span(|s| s.class("text-fg-muted").text("world "))
                    .span(|s| s.class("text-accent").text(world.name.clone()))
            });
        if let Some(docs) = &world.docs {
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

    let mut content = Division::builder();
    content.class("md:col-span-2 space-y-8");

    if !world.imports.is_empty() {
        content.push(render_item_section("Imports", &world.imports));
    }
    if !world.exports.is_empty() {
        content.push(render_item_section("Exports", &world.exports));
    }

    grid.push(content.build());

    // Sidebar
    let sidebar_ctx = SidebarContext {
        display_name,
        version,
        doc,
        active: SidebarActive::World(&world.name),
    };
    grid.push(render_sidebar(&sidebar_ctx));

    body.push(grid.build());

    layout::document(&title, &body.build().to_string())
}

/// Breadcrumb: Home / package / world
fn render_breadcrumb(display_name: &str, pkg_url: &str, world_name: &str) -> Navigation {
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
        .span(|s| s.class("text-fg font-medium").text(world_name.to_owned()))
        .build()
}

/// Render an imports or exports section.
fn render_item_section(heading: &str, items: &[WorldItemDoc]) -> Division {
    let mut div = Division::builder();
    div.heading_2(|h2| {
        h2.class("text-lg font-semibold mb-3")
            .text(heading.to_owned())
    });

    let mut ul = UnorderedList::builder();
    ul.class("space-y-2");
    for item in items {
        ul.push(render_world_item_row(item));
    }
    div.push(ul.build());
    div.build()
}

/// Render a single world item row.
fn render_world_item_row(item: &WorldItemDoc) -> ListItem {
    let mut li = ListItem::builder();
    li.class(
        "border border-border rounded-lg px-4 py-3 \
         hover:border-accent/50 transition-colors",
    );

    match item {
        WorldItemDoc::Interface {
            name,
            url: Some(url),
        } => {
            li.anchor(|a| {
                a.href(url.clone())
                    .class("block font-mono text-accent hover:underline text-sm")
                    .text(name.clone())
            });
        }
        WorldItemDoc::Interface { name, url: None } => {
            li.span(|s| {
                s.class("block font-mono text-fg text-sm")
                    .text(name.clone())
            });
        }
        WorldItemDoc::Function(func) => {
            let sig = format_function_signature(func);
            li.code(|c| {
                c.class("block font-mono text-sm text-accent")
                    .text(sig)
            });
            if let Some(docs) = &func.docs {
                li.paragraph(|p| {
                    p.class("text-sm text-fg-secondary mt-1")
                        .text(first_sentence(docs))
                });
            }
        }
        WorldItemDoc::Type(ty) => {
            li.span(|s| {
                s.class("block font-mono text-sm")
                    .span(|s2| s2.class("text-fg-muted").text("type "))
                    .span(|s2| s2.class("text-accent").text(ty.name.clone()))
            });
            if let Some(docs) = &ty.docs {
                li.paragraph(|p| {
                    p.class("text-sm text-fg-secondary mt-1")
                        .text(first_sentence(docs))
                });
            }
        }
    }

    li.build()
}

/// Format a function signature.
fn format_function_signature(func: &wasm_wit_doc::FunctionDoc) -> String {
    let params: Vec<String> = func
        .params
        .iter()
        .filter(|p| p.name != "self")
        .map(|p| format!("{}: {}", p.name, format_type_ref_short(&p.ty)))
        .collect();
    let ret = func
        .result
        .as_ref()
        .map(|r| format!(" -> {}", format_type_ref_short(r)))
        .unwrap_or_default();
    format!("{}({}){ret}", func.name, params.join(", "))
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
            let ok_s = ok
                .as_ref()
                .map_or_else(|| "_".to_owned(), |t| format_type_ref_short(t));
            let err_s = err
                .as_ref()
                .map_or_else(|| "_".to_owned(), |t| format_type_ref_short(t));
            format!("result<{ok_s}, {err_s}>")
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
