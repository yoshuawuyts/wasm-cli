//! Interface detail page.

use crate::wit_doc::{FunctionDoc, InterfaceDoc, TypeDoc, TypeKind, WitDocument};
use html::text_content::{Division, ListItem, UnorderedList};
use wasm_meta_registry_client::{KnownPackage, PackageVersion};

use super::package_shell;

/// Render the interface detail page.
#[must_use]
pub(crate) fn render(
    pkg: &KnownPackage,
    version: &str,
    version_detail: Option<&PackageVersion>,
    iface: &InterfaceDoc,
    _doc: &WitDocument,
) -> String {
    let display_name = package_shell::display_name_for(pkg);
    let title = format!("{display_name} — {}", iface.name);

    // Interface content — heading + docs in a two-column row
    let docs_md = iface
        .docs
        .as_deref()
        .map(|docs| crate::markdown::render_block(docs, crate::markdown::DOC_CLASS))
        .unwrap_or_default();

    let fqn = format!("{display_name}/{}", iface.name);

    let copy_icon = "<svg xmlns='http://www.w3.org/2000/svg' width='14' height='14' viewBox='0 0 24 24' fill='none' stroke='currentColor' stroke-width='2' stroke-linecap='round' stroke-linejoin='round'><rect x='9' y='9' width='13' height='13' rx='2' ry='2'/><path d='M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1'/></svg>";
    let check_icon = "<svg xmlns='http://www.w3.org/2000/svg' width='14' height='14' viewBox='0 0 24 24' fill='none' stroke='currentColor' stroke-width='2' stroke-linecap='round' stroke-linejoin='round'><polyline points='20 6 9 17 4 12'/></svg>";

    let header_row = format!(
        r#"<div class="flex gap-6 max-w-3xl mb-6">
  <div class="shrink-0 w-52">
    <h2 class="text-3xl font-light tracking-display flex items-baseline gap-2 group">
      <span class="text-wit-iface">{iface_name}</span>
      <button id="copy-fqn-btn" class="text-fg-faint hover:text-fg transition-opacity cursor-pointer opacity-0 group-hover:opacity-100" style="font-size:0.5em;vertical-align:middle" title="Copy item path to clipboard">{copy_icon}</button>
    </h2>
    <span class="text-sm text-fg-muted mt-2 block">Interface</span>
  </div>
  <div class="min-w-0 pt-1">{docs_md}</div>
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
        iface_name = iface.name,
    );

    // Grouped type and function sections
    let mut content = Division::builder();
    content.class("space-y-6 max-w-3xl");
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

    let body_html = format!("{header_row}{}", content.build());

    let ctx = package_shell::SidebarContext {
        pkg,
        version,
        version_detail,
        importers: &[],
        exporters: &[],
    };
    package_shell::render_page_with_crumbs(&ctx, &title, &body_html, &[])
}

/// Render a section of types grouped by kind.
fn render_type_section(heading: &str, types: &[&TypeDoc]) -> Division {
    let mut div = Division::builder();
    div.class("pt-6 first:pt-0");
    div.heading_2(|h2| {
        h2.class("text-base font-medium text-fg-muted mb-3 pb-2 border-b border-border")
            .text(heading.to_owned())
    });

    let mut ul = UnorderedList::builder();
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
    li.class("py-1 flex gap-6");

    // Left: kind-colored name
    li.division(|left| {
        left.class("shrink-0 w-52").anchor(|a| {
            a.href(ty.url.clone())
                .class(format!(
                    "font-mono text-sm font-medium hover:underline {color_class}"
                ))
                .text(ty.name.clone())
        })
    });

    // Right: doc excerpt
    if let Some(docs) = &ty.docs {
        li.division(|right| {
            right
                .class("text-sm leading-snug text-fg-secondary line-clamp-2 min-w-0")
                .text(crate::markdown::render_inline(&first_sentence(docs)))
        });
    }

    li.build()
}

/// Render the freestanding functions section.
fn render_function_section(functions: &[FunctionDoc]) -> Division {
    let mut div = Division::builder();
    div.class("pt-6 first:pt-0");
    div.heading_2(|h2| {
        h2.class("text-base font-medium text-fg-muted mb-3 pb-2 border-b border-border")
            .text("Functions")
    });

    let mut ul = UnorderedList::builder();
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
    li.class("py-1 flex gap-6");

    // Left: function name
    li.division(|left| {
        left.class("shrink-0 w-52").anchor(|a| {
            a.href(func.url.clone())
                .class(format!(
                    "font-mono text-sm font-medium hover:underline {color_class}"
                ))
                .text(func.name.clone())
        })
    });

    // Right: doc excerpt
    if let Some(docs) = &func.docs {
        li.division(|right| {
            right
                .class("text-sm leading-snug text-fg-secondary line-clamp-2 min-w-0")
                .text(crate::markdown::render_inline(&first_sentence(docs)))
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

/// Render the full interface definition as a WIT code block.
#[allow(dead_code)]
fn render_interface_definition(iface: &InterfaceDoc) -> Division {
    use super::wit_render::{self, CODE_BLOCK_CLASS};

    Division::builder()
        .class("mb-8")
        .push(
            html::text_content::PreformattedText::builder()
                .class(CODE_BLOCK_CLASS)
                .code(|c| {
                    c.span(|s| s.class("text-fg-muted").text("interface "))
                        .span(|s| {
                            s.class("text-wit-iface font-medium")
                                .text(iface.name.clone())
                        })
                        .text(" {\n".to_owned());

                    for ty in &iface.types {
                        wit_render::render_type_in_code(c, ty, "  ");
                        c.text("\n\n".to_owned());
                    }

                    for func in &iface.functions {
                        wit_render::render_func_in_code(c, func, "  ");
                        c.text("\n".to_owned());
                    }

                    c.text("}".to_owned())
                })
                .build(),
        )
        .build()
}
