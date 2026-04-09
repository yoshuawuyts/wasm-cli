//! Interface detail page.

use crate::wit_doc::{FunctionDoc, InterfaceDoc, TypeDoc, TypeKind, WitDocument};
use html::content::Navigation;
use html::text_content::{Division, ListItem, UnorderedList};

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
        div.class("mb-6").heading_1(|h1| {
            h1.class("text-3xl font-bold tracking-tight font-mono")
                .span(|s| s.class("text-fg-muted").text(format!("{display_name} / ")))
                .span(|s| s.class("text-accent").text(iface.name.clone()))
        });
        if let Some(docs) = &iface.docs {
            div.paragraph(|p| p.class("text-lg text-fg-secondary mt-2").text(docs.clone()));
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
    div.class("pt-6 border-t border-border/50 first:pt-0 first:border-0");
    div.heading_2(|h2| {
        h2.class("text-sm font-semibold text-fg-muted uppercase tracking-wide mb-3")
            .text(heading.to_owned())
    });

    let mut ul = UnorderedList::builder();
    ul.class("space-y-0.5");
    for ty in types {
        ul.push(render_type_row(ty));
    }
    div.push(ul.build());
    div.build()
}

/// Render a single type row in docs.rs style: linked name + doc excerpt.
fn render_type_row(ty: &TypeDoc) -> ListItem {
    let color_class = kind_color_class(&ty.kind);

    let mut li = ListItem::builder();
    li.class("py-3 flex gap-6");

    // Left: kind-colored name
    li.division(|left| {
        left.class("shrink-0 w-52").anchor(|a| {
            a.href(ty.url.clone())
                .class(format!(
                    "font-mono text-sm font-semibold hover:underline {color_class}"
                ))
                .text(ty.name.clone())
        })
    });

    // Right: doc excerpt
    if let Some(docs) = &ty.docs {
        li.division(|right| {
            right
                .class("text-sm leading-relaxed text-fg-secondary line-clamp-2 min-w-0")
                .text(first_sentence(docs))
        });
    }

    li.build()
}

/// Render the freestanding functions section.
fn render_function_section(functions: &[FunctionDoc]) -> Division {
    let mut div = Division::builder();
    div.class("pt-6 border-t border-border/50 first:pt-0 first:border-0");
    div.heading_2(|h2| {
        h2.class("text-sm font-semibold text-fg-muted uppercase tracking-wide mb-3")
            .text("Functions")
    });

    let mut ul = UnorderedList::builder();
    ul.class("space-y-0.5");
    for func in functions {
        ul.push(render_function_row(func));
    }
    div.push(ul.build());
    div.build()
}

/// Render a single function row: linked name + doc excerpt.
fn render_function_row(func: &FunctionDoc) -> ListItem {
    // Color for functions: use a teal/cyan hue
    let color_class = "text-wit-func";

    let mut li = ListItem::builder();
    li.class("py-3 flex gap-6");

    // Left: function name
    li.division(|left| {
        left.class("shrink-0 w-52").anchor(|a| {
            a.href(func.url.clone())
                .class(format!(
                    "font-mono text-sm font-semibold hover:underline {color_class}"
                ))
                .text(func.name.clone())
        })
    });

    // Right: doc excerpt
    if let Some(docs) = &func.docs {
        li.division(|right| {
            right
                .class("text-sm leading-relaxed text-fg-secondary line-clamp-2 min-w-0")
                .text(first_sentence(docs))
        });
    }

    li.build()
}

/// Get the CSS color class for a type kind.
///
/// Palette (OKLCH-based, same hue family as the design system):
/// - Records/Variants: blue-violet (hue 260) — structural data types
/// - Enums/Flags: teal (hue 180) — enumerable values
/// - Resources: amber (hue 70) — managed handles
/// - Aliases: default accent — pass-through types
/// - Functions: indigo (hue 240) — callable items
fn kind_color_class(kind: &TypeKind) -> &'static str {
    match kind {
        TypeKind::Record { .. } | TypeKind::Variant { .. } => "text-wit-struct",
        TypeKind::Enum { .. } | TypeKind::Flags { .. } => "text-wit-enum",
        TypeKind::Resource { .. } => "text-wit-resource",
        TypeKind::Alias(_) => "text-accent",
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
