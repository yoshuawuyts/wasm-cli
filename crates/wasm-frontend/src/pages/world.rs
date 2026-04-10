//! World detail page.

use crate::wit_doc::{WitDocument, WorldDoc, WorldItemDoc};
use html::text_content::{Division, ListItem, UnorderedList};
use wasm_meta_registry_client::{KnownPackage, PackageVersion};

use super::package_shell;

/// Render the world detail page.
#[must_use]
pub(crate) fn render(
    pkg: &KnownPackage,
    version: &str,
    version_detail: Option<&PackageVersion>,
    world: &WorldDoc,
    _doc: &WitDocument,
) -> String {
    let display_name = package_shell::display_name_for(pkg);
    let title = format!("{display_name} \u{2014} {}", world.name);

    let mut outer = Division::builder();

    let mut content = Division::builder();
    content.class("space-y-10");

    if !world.imports.is_empty() {
        content.push(render_item_section("Imports", &world.imports, true));
    }
    if !world.exports.is_empty() {
        content.push(render_item_section("Exports", &world.exports, false));
    }

    outer.push(content.build());

    let ctx = package_shell::SidebarContext {
        pkg,
        version,
        version_detail,
        importers: &[],
        exporters: &[],
        description_override: world.docs.as_deref(),
    };
    let extra = vec![crate::nav::Crumb {
        label: world.name.clone(),
        href: None,
    }];
    package_shell::render_page_with_crumbs(&ctx, &title, outer.build(), extra)
}

/// Render an imports or exports section, grouped by package namespace.
fn render_item_section(heading: &str, items: &[WorldItemDoc], is_import: bool) -> Division {
    let mut div = Division::builder();
    div.heading_2(|h2| {
        h2.class("text-sm font-medium text-fg-muted uppercase tracking-wide mb-3 pb-2 border-b-2 border-fg")
            .text(heading.to_owned())
    });

    let link_color = if is_import {
        "block font-mono text-wit-import hover:underline text-sm"
    } else {
        "block font-mono text-accent hover:underline text-sm"
    };

    let mut ul = UnorderedList::builder();
    ul.class("space-y-0.5");
    for item in items {
        ul.push(render_world_item_row(item, link_color));
    }

    div.push(ul.build());
    div.build()
}

/// Strip version suffix from a qualified name.
///
/// `"wasi:cli/environment@0.2.11"` → `"wasi:cli/environment"`
fn strip_version(name: &str) -> &str {
    name.split('@').next().unwrap_or(name)
}

/// Render a single world item row.
fn render_world_item_row(item: &WorldItemDoc, link_color: &str) -> ListItem {
    let mut li = ListItem::builder();
    li.class("py-3 px-2");

    match item {
        WorldItemDoc::Interface {
            name,
            url: Some(url),
        } => {
            let display = strip_version(name);
            li.anchor(|a| {
                a.href(url.clone())
                    .class(link_color.to_owned())
                    .text(display.to_owned())
            });
        }
        WorldItemDoc::Interface { name, url: None } => {
            let display = strip_version(name);
            li.span(|s| {
                s.class("block font-mono text-fg text-sm")
                    .text(display.to_owned())
            });
        }
        WorldItemDoc::Function(func) => {
            let sig = format_function_signature(func);
            li.code(|c| c.class("block font-mono text-sm text-accent").text(sig));
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
fn format_function_signature(func: &crate::wit_doc::FunctionDoc) -> String {
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
fn format_type_ref_short(ty: &crate::wit_doc::TypeRef) -> String {
    match ty {
        crate::wit_doc::TypeRef::Primitive { name }
        | crate::wit_doc::TypeRef::Named { name, .. } => name.clone(),
        crate::wit_doc::TypeRef::List { ty } => {
            format!("list<{}>", format_type_ref_short(ty))
        }
        crate::wit_doc::TypeRef::Option { ty } => {
            format!("option<{}>", format_type_ref_short(ty))
        }
        crate::wit_doc::TypeRef::Result { ok, err } => {
            let ok_s = ok
                .as_ref()
                .map_or_else(|| "_".to_owned(), |t| format_type_ref_short(t));
            let err_s = err
                .as_ref()
                .map_or_else(|| "_".to_owned(), |t| format_type_ref_short(t));
            format!("result<{ok_s}, {err_s}>")
        }
        crate::wit_doc::TypeRef::Tuple { types } => {
            let inner: Vec<String> = types.iter().map(format_type_ref_short).collect();
            format!("tuple<{}>", inner.join(", "))
        }
        crate::wit_doc::TypeRef::Handle {
            handle_kind,
            resource_name,
            ..
        } => match handle_kind {
            crate::wit_doc::HandleKind::Own => resource_name.clone(),
            crate::wit_doc::HandleKind::Borrow => format!("borrow<{resource_name}>"),
        },
        crate::wit_doc::TypeRef::Future { ty } => match ty {
            Some(t) => format!("future<{}>", format_type_ref_short(t)),
            None => "future".to_owned(),
        },
        crate::wit_doc::TypeRef::Stream { ty } => match ty {
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
