//! Detail page for a child module or component inside a Wasm component.

use html::text_content::{Division, UnorderedList};
use wasm_meta_registry_client::{ComponentSummary, KnownPackage, PackageVersion};

use super::package_shell;

/// Render the detail page for a child module or component.
#[must_use]
pub(crate) fn render(
    pkg: &KnownPackage,
    version: &str,
    version_detail: Option<&PackageVersion>,
    child: &ComponentSummary,
    display_name: &str,
) -> String {
    let pkg_display = package_shell::display_name_for(pkg);
    let kind = child.kind.as_deref().unwrap_or("module");
    let title = format!("{pkg_display} \u{2014} {display_name}");

    // Header
    let kind_color = if kind == "component" {
        "text-wit-world"
    } else {
        "text-wit-module"
    };

    let size_text = child
        .size_bytes
        .map(super::package::format_size)
        .unwrap_or_default();

    let lang_text = if child.languages.is_empty() {
        String::new()
    } else {
        child.languages.join(", ")
    };

    let mut subtitle_parts = vec![kind.to_owned()];
    if !size_text.is_empty() {
        subtitle_parts.push(size_text);
    }
    if !lang_text.is_empty() {
        subtitle_parts.push(lang_text);
    }
    let subtitle = subtitle_parts.join(" \u{2014} ");

    let header = format!(
        r#"<div class="max-w-3xl mb-6">
  <h2 class="text-3xl font-light tracking-display font-display">
    <span class="{kind_color}">{display_name}</span>
  </h2>
  <span class="text-sm text-fg-muted mt-1 block">{subtitle}</span>
</div>"#,
    );

    let mut body = format!("{header}<div class=\"space-y-10 max-w-3xl pt-4 pb-12\">");

    // WIT imports
    if !child.imports.is_empty() {
        let entries: Vec<package_shell::ImportExportEntry> = child
            .imports
            .iter()
            .map(package_shell::iface_ref_to_entry)
            .collect();
        body.push_str(
            &package_shell::render_import_export_section("Imports", &entries).to_string(),
        );
    }

    // WIT exports
    if !child.exports.is_empty() {
        let entries: Vec<package_shell::ImportExportEntry> = child
            .exports
            .iter()
            .map(package_shell::iface_ref_to_entry)
            .collect();
        body.push_str(
            &package_shell::render_import_export_section("Exports", &entries).to_string(),
        );
    }

    // Producers
    if !child.producers.is_empty() {
        body.push_str(&render_producers_section(&child.producers));
    }

    // Dependencies
    if !child.bill_of_materials.is_empty() {
        body.push_str(&render_bom_section(&child.bill_of_materials));
    }

    body.push_str("</div>");

    let ctx = package_shell::SidebarContext {
        pkg,
        version,
        version_detail,
        importers: &[],
        exporters: &[],
    };
    package_shell::render_page_with_crumbs(&ctx, &title, &body, &[])
}

/// Render producers as a list, excluding language entries (shown in subtitle).
fn render_producers_section(producers: &[wasm_meta_registry_client::ProducerEntry]) -> String {
    // Filter out language entries — those are shown in the subtitle.
    let filtered: Vec<_> = producers.iter().filter(|e| e.field != "language").collect();
    if filtered.is_empty() {
        return String::new();
    }

    let mut div = Division::builder();
    div.heading_2(|h2| {
        h2.class("text-lg font-medium text-fg-muted mb-3 pb-2 border-b border-border")
            .text("Producers")
    });

    let mut ul = UnorderedList::builder();
    for entry in &filtered {
        let name = entry.name.clone();
        let version = entry.version.clone();
        // Strip parenthesized info from display, keep in tooltip.
        let display_version = version
            .split_once(" (")
            .map_or_else(|| version.clone(), |(before, _)| before.to_owned());
        let tooltip = if version.is_empty() {
            name.clone()
        } else {
            format!("{name} {version}")
        };
        ul.list_item(|li| {
            li.class("py-1");
            li.span(|s| {
                s.class("font-mono text-base min-w-0 truncate")
                    .title(tooltip);
                s.span(|n| n.class("text-accent").text(name));
                if !display_version.is_empty() {
                    s.span(|v| {
                        v.class("text-fg-faint ml-1")
                            .text(format!("@{display_version}"))
                    });
                }
                s
            });
            li
        });
    }
    div.push(ul.build());
    div.build().to_string()
}

/// Render dependencies as package URLs with links to crates.io.
fn render_bom_section(deps: &[wasm_meta_registry_client::BomEntry]) -> String {
    let mut div = Division::builder();
    div.heading_2(|h2| {
        h2.class("text-lg font-medium text-fg-muted mb-3 pb-2 border-b border-border")
            .text("Dependencies")
    });

    let mut ul = UnorderedList::builder();
    for dep in deps {
        let name = dep.name.clone();
        let version = dep.version.clone();
        let source = dep.source.as_deref().unwrap_or("crates.io");
        let (purl_type, href) = match source {
            "crates.io" | "registry" => (
                "cargo",
                Some(format!("https://crates.io/crates/{name}/{version}")),
            ),
            _ => ("generic", None),
        };
        let purl = format!("pkg:{purl_type}/{name}@{version}");
        ul.list_item(|li| {
            li.class("py-1");
            if let Some(url) = href {
                li.anchor(|a| {
                    a.href(url).class("font-mono text-base hover:underline");
                    a.span(|s| s.class("text-fg-muted").text(format!("pkg:{purl_type}/")));
                    a.span(|s| s.class("text-accent").text(name));
                    a.span(|s| s.class("text-fg-faint ml-1").text(format!("@{version}")));
                    a
                })
                .title(purl);
            } else {
                li.span(|s| {
                    s.class("font-mono text-base");
                    s.span(|ps| ps.class("text-fg-muted").text(format!("pkg:{purl_type}/")));
                    s.span(|ns| ns.class("text-fg").text(name));
                    s.span(|vs| vs.class("text-fg-faint ml-1").text(format!("@{version}")));
                    s
                })
                .title(purl);
            }
            li
        });
    }
    div.push(ul.build());
    div.build().to_string()
}
