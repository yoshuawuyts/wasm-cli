//! Package detail page.

// r[impl frontend.pages.package-detail]

use html::content::{Aside, Navigation, Section};
use html::inline_text::Span;
use html::text_content::{Division, ListItem, UnorderedList};
use wasm_meta_registry_client::{KnownPackage, PackageVersion};
use wasm_wit_doc::WitDocument;

use crate::layout;

/// Which tab is currently active on the package detail page.
pub(crate) enum ActiveTab<'a> {
    /// WIT definition and worlds.
    Docs {
        version_detail: Option<&'a PackageVersion>,
    },
    /// Forward dependencies of this package.
    Dependencies,
    /// Reverse dependencies: packages that import or export this interface.
    Dependents {
        importers: &'a [KnownPackage],
        exporters: &'a [KnownPackage],
    },
}

/// Render the package detail page for a given package and version.
#[must_use]
pub(crate) fn render(pkg: &KnownPackage, version: &str, tab: &ActiveTab<'_>) -> String {
    let display_name = match (&pkg.wit_namespace, &pkg.wit_name) {
        (Some(ns), Some(name)) => format!("{ns}:{name}"),
        _ => pkg.repository.clone(),
    };

    let description = pkg
        .description
        .as_deref()
        .unwrap_or("No description available");

    let mut body = Division::builder();

    body.class("pt-8");

    // Breadcrumb
    body.push(render_breadcrumb(&display_name));

    // Title
    body.division(|div| {
        div.class("mb-8")
            .heading_1(|h1| {
                h1.class("text-3xl font-bold tracking-tight text-accent")
                    .text(display_name.clone())
            })
            .paragraph(|p| {
                p.class("text-lg text-fg-secondary mt-2")
                    .text(description.to_owned())
            })
    });

    // Tab bar + active panel
    let url_base = match (&pkg.wit_namespace, &pkg.wit_name) {
        (Some(ns), Some(name)) => format!("/{ns}/{name}/{version}"),
        _ => format!("/{}/{version}", pkg.repository),
    };
    body.push(render_tab_bar(&url_base, tab));

    // Grid layout: main content + sidebar (shared across all tabs)
    let mut grid = Division::builder();
    grid.class("grid grid-cols-1 md:grid-cols-3 gap-12");

    // Main content column (varies by tab)
    let mut main_col = Division::builder();
    main_col.class("md:col-span-2 space-y-8");
    match tab {
        ActiveTab::Docs { version_detail } => {
            if let Some(detail) = version_detail {
                main_col.push(render_wit_content(detail, &url_base));
            }
        }
        ActiveTab::Dependencies => {
            main_col.push(render_dependencies_panel(pkg));
        }
        ActiveTab::Dependents {
            importers,
            exporters,
        } => {
            main_col.push(render_dependents_panel(importers, exporters));
        }
    }
    grid.push(main_col.build());

    // Sidebar
    let version_detail = match tab {
        ActiveTab::Docs { version_detail } => *version_detail,
        _ => None,
    };
    grid.push(render_sidebar(pkg, version, &display_name, version_detail));

    body.push(grid.build());

    layout::document(&display_name, &body.build().to_string())
}

/// Render the install command section with a copy button.
fn render_install_command(display_name: &str, version: &str) -> Division {
    let command = format!("wasm install {display_name}@{version}");

    let copy_icon = "<svg xmlns='http://www.w3.org/2000/svg' width='16' height='16' viewBox='0 0 24 24' fill='none' stroke='currentColor' stroke-width='2' stroke-linecap='round' stroke-linejoin='round'><rect x='9' y='9' width='13' height='13' rx='2' ry='2'/><path d='M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1'/></svg>";
    let check_icon = "<svg xmlns='http://www.w3.org/2000/svg' width='16' height='16' viewBox='0 0 24 24' fill='none' stroke='currentColor' stroke-width='2' stroke-linecap='round' stroke-linejoin='round'><polyline points='20 6 9 17 4 12'/></svg>";

    let script = format!(
        "(function(){{\
        var btn=document.getElementById('copy-install-btn');\
        var copyIcon=\"{copy_icon}\";\
        var checkIcon=\"{check_icon}\";\
        btn.innerHTML=copyIcon;\
        btn.addEventListener('click',function(){{\
        navigator.clipboard.writeText('{command}').then(function(){{\
        btn.innerHTML=checkIcon;\
        setTimeout(function(){{btn.innerHTML=copyIcon}},2000)\
        }})}})}})()",
    );

    Division::builder()
        .class("bg-surface border border-border rounded-lg p-5 space-y-2 text-sm group/install")
        .division(|div| {
            div.class(
                "flex items-center gap-2 bg-surface-muted border border-border \
                 rounded-md px-3 py-2 font-mono text-xs text-fg",
            )
            .code(|code| {
                code.class("flex-1 select-all overflow-hidden whitespace-nowrap text-ellipsis")
                    .text(command)
            })
            .button(|btn| {
                btn.id("copy-install-btn").class(
                    "shrink-0 text-fg-muted opacity-0 group-hover/install:opacity-100 \
                     hover:text-fg transition-opacity cursor-pointer",
                )
            })
            .script(|s| s.text(script))
        })
        .build()
}

/// Render the breadcrumb navigation.
fn render_breadcrumb(display_name: &str) -> Navigation {
    Navigation::builder()
        .class("text-sm text-fg-muted mb-4")
        .anchor(|a| {
            a.href("/")
                .class("hover:text-accent transition-colors")
                .text("Home")
        })
        .span(|s| s.class("mx-1").text("/"))
        .span(|s| s.text(display_name.to_owned()))
        .build()
}

/// Render the WIT content section for a package version.
///
/// For components, displays the world imports and exports.
fn render_wit_content(detail: &PackageVersion, url_base: &str) -> Section {
    let mut section = Section::builder();

    if let Some(doc) = try_parse_wit(detail, url_base) {
        if !doc.interfaces.is_empty() {
            section.push(render_interface_overview(&doc));
        }
        if !doc.worlds.is_empty() {
            section.push(render_world_overview(&doc));
        }
    } else if let Some(wit_text) = &detail.wit_text {
        section.push(render_raw_wit(wit_text));
    }

    section.build()
}

/// Try parsing the WIT text into a rich document model.
fn try_parse_wit(detail: &PackageVersion, url_base: &str) -> Option<WitDocument> {
    let wit_text = detail.wit_text.as_deref()?;
    let dep_urls = build_dep_urls(&detail.dependencies);
    wasm_wit_doc::parse_wit_doc(wit_text, url_base, &dep_urls).ok()
}

/// Build the `dep_urls` mapping from a package's declared dependencies.
///
/// Maps `"namespace:name"` → `"/namespace/name/version"` for each
/// dependency that has a version.
fn build_dep_urls(
    deps: &[wasm_meta_registry_client::PackageDependencyRef],
) -> std::collections::HashMap<String, String> {
    deps.iter()
        .filter_map(|dep| {
            let version = dep.version.as_deref()?;
            let url = format!("/{}/{version}", dep.package.replace(':', "/"));
            Some((dep.package.clone(), url))
        })
        .collect()
}

/// Render the interfaces overview section.
fn render_interface_overview(doc: &WitDocument) -> Division {
    let mut container = Division::builder();
    container.class("space-y-4");
    container.heading_2(|h2| {
        h2.class("text-lg font-semibold mb-1").text("Interfaces")
    });

    let mut ul = UnorderedList::builder();
    ul.class("space-y-3");
    for iface in &doc.interfaces {
        ul.push(render_interface_row(iface));
    }
    container.push(ul.build());
    container.build()
}

/// Render a single interface row in the overview list.
fn render_interface_row(iface: &wasm_wit_doc::InterfaceDoc) -> ListItem {
    let type_count = iface.types.len();
    let func_count = iface.functions.len();

    let mut li = ListItem::builder();
    li.class(
        "border border-border rounded-lg p-4 \
         hover:border-accent/50 transition-colors",
    );

    li.anchor(|a| {
        a.href(iface.url.clone())
            .class("block group")
            .division(|div| {
                div.class("flex items-baseline gap-2")
                    .span(|s| {
                        s.class(
                            "font-mono font-semibold text-accent \
                             group-hover:underline",
                        )
                        .text(iface.name.clone())
                    })
                    .span(|s| {
                        s.class("text-xs text-fg-muted")
                            .text(item_counts_label(type_count, func_count))
                    })
            });
        if let Some(docs) = &iface.docs {
            a.paragraph(|p| {
                p.class("text-sm text-fg-secondary mt-1 line-clamp-2")
                    .text(first_sentence(docs))
            });
        }
        a
    });

    li.build()
}

/// Render the worlds overview section.
fn render_world_overview(doc: &WitDocument) -> Division {
    let mut container = Division::builder();
    container.class("space-y-4 mt-8");
    container.heading_2(|h2| {
        h2.class("text-lg font-semibold mb-1").text("Worlds")
    });

    let mut ul = UnorderedList::builder();
    ul.class("space-y-3");
    for world in &doc.worlds {
        ul.push(render_world_row(world));
    }
    container.push(ul.build());
    container.build()
}

/// Render a single world row in the overview list.
fn render_world_row(world: &wasm_wit_doc::WorldDoc) -> ListItem {
    let import_count = world.imports.len();
    let export_count = world.exports.len();

    let mut li = ListItem::builder();
    li.class(
        "border border-border rounded-lg p-4 \
         hover:border-accent/50 transition-colors",
    );

    li.anchor(|a| {
        a.href(world.url.clone())
            .class("block group")
            .division(|div| {
                div.class("flex items-baseline gap-2")
                    .span(|s| {
                        s.class(
                            "font-mono font-semibold text-accent \
                             group-hover:underline",
                        )
                        .text(world.name.clone())
                    })
                    .span(|s| {
                        s.class("text-xs text-fg-muted")
                            .text(world_counts_label(import_count, export_count))
                    })
            });
        if let Some(docs) = &world.docs {
            a.paragraph(|p| {
                p.class("text-sm text-fg-secondary mt-1 line-clamp-2")
                    .text(first_sentence(docs))
            });
        }
        a
    });

    li.build()
}

/// Render raw WIT text in a pre-formatted code block (fallback).
fn render_raw_wit(wit_text: &str) -> Division {
    Division::builder()
        .heading_2(|h2| {
            h2.class("text-lg font-semibold mb-3")
                .text("WIT Definition")
        })
        .push(
            html::text_content::PreformattedText::builder()
                .class("bg-surface-muted border border-border rounded-lg p-4 overflow-x-auto text-sm leading-relaxed")
                .code(|code| code.class("text-fg").text(wit_text.to_owned()))
                .build(),
        )
        .build()
}

/// Format a counts label like "3 types, 2 functions".
fn item_counts_label(types: usize, funcs: usize) -> String {
    let mut parts = Vec::new();
    if types > 0 {
        parts.push(format!(
            "{types} {}",
            if types == 1 { "type" } else { "types" }
        ));
    }
    if funcs > 0 {
        parts.push(format!(
            "{funcs} {}",
            if funcs == 1 { "function" } else { "functions" }
        ));
    }
    if parts.is_empty() {
        "empty".to_owned()
    } else {
        parts.join(", ")
    }
}

/// Format a counts label like "2 imports, 1 export".
fn world_counts_label(imports: usize, exports: usize) -> String {
    let mut parts = Vec::new();
    if imports > 0 {
        parts.push(format!(
            "{imports} {}",
            if imports == 1 { "import" } else { "imports" }
        ));
    }
    if exports > 0 {
        parts.push(format!(
            "{exports} {}",
            if exports == 1 { "export" } else { "exports" }
        ));
    }
    if parts.is_empty() {
        "empty".to_owned()
    } else {
        parts.join(", ")
    }
}

/// Extract the first sentence from a doc comment for summary display.
fn first_sentence(text: &str) -> String {
    text.split_once(". ")
        .map_or_else(|| text.to_owned(), |(first, _)| format!("{first}."))
}

/// Render the tab bar with links to each tab route.
fn render_tab_bar(url_base: &str, active: &ActiveTab<'_>) -> Division {
    let active_class = "text-accent border-b-2 border-accent font-semibold";
    let inactive_class = "text-fg-muted hover:text-fg";
    let tab_base = "px-4 py-2 text-sm transition-colors inline-block";

    let tabs: &[(&str, &str, bool)] = &[
        ("Docs", url_base, matches!(active, ActiveTab::Docs { .. })),
        (
            "Dependencies",
            &format!("{url_base}/dependencies"),
            matches!(active, ActiveTab::Dependencies),
        ),
        (
            "Dependents",
            &format!("{url_base}/dependents"),
            matches!(active, ActiveTab::Dependents { .. }),
        ),
    ];

    Division::builder()
        .class("flex border-b border-border mb-8")
        .push({
            let mut nav = Division::builder();
            nav.class("flex");
            for &(label, href, is_active) in tabs {
                let style = if is_active {
                    active_class
                } else {
                    inactive_class
                };
                nav.anchor(|a| {
                    a.href(href.to_owned())
                        .class(format!("{tab_base} {style}"))
                        .text(label.to_owned())
                });
            }
            nav.build()
        })
        .build()
}

/// Render the dependencies panel showing forward dependencies.
fn render_dependencies_panel(pkg: &KnownPackage) -> Division {
    let mut div = Division::builder();
    div.paragraph(|p| {
        p.class("text-fg-muted text-sm mb-4")
            .text("Interfaces this component depends on.")
    });

    if pkg.dependencies.is_empty() {
        div.paragraph(|p| {
            p.class("text-fg-muted text-sm italic")
                .text("No dependencies")
        });
        return div.build();
    }

    let mut ul = UnorderedList::builder();
    ul.class("space-y-2");
    for dep in &pkg.dependencies {
        let mut li = ListItem::builder();
        li.class("text-sm font-mono");
        li.push(
            Span::builder()
                .class("text-accent")
                .push(
                    html::inline_text::Anchor::builder()
                        .href(format!("/{}", dep.package.replace(':', "/")))
                        .class("text-accent hover:underline font-medium")
                        .text(dep.package.clone())
                        .build(),
                )
                .build(),
        );
        if let Some(v) = &dep.version {
            li.push(
                Span::builder()
                    .class("text-fg-faint ml-1")
                    .text(format!("@ {v}"))
                    .build(),
            );
        }
        ul.push(li.build());
    }
    div.push(ul.build());
    div.build()
}

/// Render the dependents panel with All / Importers / Exporters filter.
fn render_dependents_panel(importers: &[KnownPackage], exporters: &[KnownPackage]) -> Division {
    let active_class = "text-accent border-b-2 border-accent font-semibold";
    let inactive_class = "text-fg-muted hover:text-fg";
    let filter_base = "px-3 py-1.5 text-xs cursor-pointer transition-colors";

    let mut container = Division::builder();
    container.paragraph(|p| {
        p.class("text-fg-muted text-sm mb-4").text(
            "Importers consume this interface. \
             Exporters implement it.",
        )
    });

    // Sub-filter bar
    container.division(|div| {
        div.class("flex border-b border-border mb-4")
            .button(|btn| {
                btn.id("filter-all")
                    .class(format!("{filter_base} {active_class}"))
                    .text(format!("All ({})", importers.len() + exporters.len()))
            })
            .button(|btn| {
                btn.id("filter-importers")
                    .class(format!("{filter_base} {inactive_class}"))
                    .text(format!("Importers ({})", importers.len()))
            })
            .button(|btn| {
                btn.id("filter-exporters")
                    .class(format!("{filter_base} {inactive_class}"))
                    .text(format!("Exporters ({})", exporters.len()))
            })
    });

    // All panel
    let mut all: Vec<&KnownPackage> = importers.iter().chain(exporters.iter()).collect();
    all.sort_by(|a, b| a.repository.cmp(&b.repository));
    all.dedup_by(|a, b| a.repository == b.repository);
    container.push(render_filterable_package_list("list-all", &all, true));

    // Importers panel
    let importer_refs: Vec<&KnownPackage> = importers.iter().collect();
    container.push(render_filterable_package_list(
        "list-importers",
        &importer_refs,
        false,
    ));

    // Exporters panel
    let exporter_refs: Vec<&KnownPackage> = exporters.iter().collect();
    container.push(render_filterable_package_list(
        "list-exporters",
        &exporter_refs,
        false,
    ));

    // Filter switching script
    let script = format!(
        "(function(){{\
        var filters=[['filter-all','list-all'],['filter-importers','list-importers'],['filter-exporters','list-exporters']];\
        var active='{active_class}',inactive='{inactive_class}',base='{filter_base}';\
        filters.forEach(function(f){{\
        document.getElementById(f[0]).addEventListener('click',function(){{\
        filters.forEach(function(o){{\
        document.getElementById(o[0]).className=base+' '+(o[0]===f[0]?active:inactive);\
        document.getElementById(o[1]).style.display=o[0]===f[0]?'':'none'\
        }})}})}})\
        }})()"
    );
    container.script(|s| s.text(script));

    container.build()
}

/// Render a filterable package list panel.
fn render_filterable_package_list(id: &str, packages: &[&KnownPackage], visible: bool) -> Division {
    let mut div = Division::builder();
    div.id(id.to_owned());
    if !visible {
        div.style("display:none");
    }

    if packages.is_empty() {
        div.paragraph(|p| p.class("text-fg-muted text-sm italic").text("None found"));
        return div.build();
    }

    let mut ul = UnorderedList::builder();
    ul.class("space-y-2");
    for pkg in packages {
        let name = match (&pkg.wit_namespace, &pkg.wit_name) {
            (Some(ns), Some(n)) => format!("{ns}:{n}"),
            _ => pkg.repository.clone(),
        };
        let href = match (&pkg.wit_namespace, &pkg.wit_name) {
            (Some(ns), Some(n)) => format!("/{ns}/{n}"),
            _ => "#".to_string(),
        };
        let desc = pkg
            .description
            .as_deref()
            .unwrap_or("No description available");

        ul.list_item(|li| {
            li.class("text-sm")
                .anchor(|a| {
                    a.href(href)
                        .class("text-accent hover:underline font-medium")
                        .text(name)
                })
                .push(
                    Span::builder()
                        .class("text-fg-secondary ml-2")
                        .text(format!("— {desc}"))
                        .build(),
                )
        });
    }
    div.push(ul.build());
    div.build()
}

/// Render the sidebar with metadata and version selector.
fn render_sidebar(
    pkg: &KnownPackage,
    current_version: &str,
    display_name: &str,
    version_detail: Option<&PackageVersion>,
) -> Aside {
    let url_name = match (&pkg.wit_namespace, &pkg.wit_name) {
        (Some(ns), Some(name)) => format!("{ns}/{name}"),
        _ => pkg.repository.clone(),
    };

    let mut aside = Aside::builder();
    aside.class("space-y-4");

    // Install command
    aside.push(render_install_command(display_name, current_version));

    let mut card = Division::builder();
    card.class("bg-surface border border-border rounded-lg p-5 space-y-4 text-sm");

    // Version selector dropdown
    if !pkg.tags.is_empty() {
        card.push(render_version_select(pkg, current_version, &url_name));
    }

    // Repository: combined registry/repository as a clickable link
    let repo_url = format!("https://{}/{}", pkg.registry, pkg.repository);
    let repo_display = format!("{}/{}", pkg.registry, pkg.repository);
    card.push(sidebar_link_row("Repository", &repo_display, &repo_url));

    if let Some(kind) = &pkg.kind {
        card.push(sidebar_row("Kind", &kind.to_string()));
    }
    if let Some(size) = version_detail.and_then(|d| d.size_bytes) {
        card.push(sidebar_row("Size", &format_size(size)));
    }
    card.push(sidebar_row("Published on", &pkg.created_at));
    aside.push(card.build());

    aside.build()
}

/// Render the version selector dropdown.
fn render_version_select(pkg: &KnownPackage, current_version: &str, url_name: &str) -> Division {
    let mut select = html::forms::Select::builder();
    select
        .id("version-select")
        .name("version")
        .class("w-full px-3 py-2 rounded-md border border-border bg-surface text-fg text-sm");

    for tag in &pkg.tags {
        let is_current = tag == current_version;
        if is_current {
            select.option(|opt| opt.value(tag.clone()).text(tag.clone()).selected(true));
        } else {
            select.option(|opt| opt.value(tag.clone()).text(tag.clone()));
        }
    }

    let script_body = format!(
        "document.getElementById('version-select').addEventListener('change',function(){{window.location.href='/{url_name}/'+this.value}})"
    );

    let version_count = pkg.tags.len();
    let version_label = format!(
        "{version_count} {}",
        if version_count == 1 {
            "version"
        } else {
            "versions"
        }
    );

    Division::builder()
        .division(|dt| {
            dt.class("text-fg-muted text-xs uppercase tracking-wide")
                .text(version_label)
        })
        .division(|dd| {
            dd.class("mt-0.5")
                .push(select.build())
                .script(|s| s.text(script_body))
        })
        .build()
}

/// Render a single sidebar metadata row.
fn sidebar_row(label: &str, value: &str) -> Division {
    Division::builder()
        .division(|dt| {
            dt.class("text-fg-muted text-xs uppercase tracking-wide")
                .text(label.to_owned())
        })
        .division(|dd| dd.class("text-fg mt-0.5 break-all").text(value.to_owned()))
        .build()
}

/// Render a sidebar row where the value is a link.
fn sidebar_link_row(label: &str, text: &str, href: &str) -> Division {
    Division::builder()
        .division(|dt| {
            dt.class("text-fg-muted text-xs uppercase tracking-wide")
                .text(label.to_owned())
        })
        .division(|dd| {
            dd.class("mt-0.5 break-all").anchor(|a| {
                a.href(href.to_owned())
                    .class("text-accent hover:underline")
                    .text(text.to_owned())
            })
        })
        .build()
}

/// Format a byte count as a human-readable size string.
fn format_size(bytes: i64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = KIB * 1024.0;
    const GIB: f64 = MIB * 1024.0;

    let bytes = bytes as f64;
    if bytes < KIB {
        format!("{bytes} B")
    } else if bytes < MIB {
        format!("{:.1} KiB", bytes / KIB)
    } else if bytes < GIB {
        format!("{:.1} MiB", bytes / MIB)
    } else {
        format!("{:.1} GiB", bytes / GIB)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_meta_registry_client::PackageDependencyRef;

    #[test]
    fn dependency_versions_include_separator() {
        let pkg = KnownPackage {
            registry: "ghcr.io".to_string(),
            repository: "example/pkg".to_string(),
            kind: None,
            description: None,
            tags: vec!["1.0.0".to_string()],
            signature_tags: vec![],
            attestation_tags: vec![],
            last_seen_at: "2026-01-01T00:00:00Z".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            wit_namespace: Some("wasi".to_string()),
            wit_name: Some("demo".to_string()),
            dependencies: vec![PackageDependencyRef {
                package: "wasi:io".to_string(),
                version: Some("0.2.0".to_string()),
            }],
        };

        let html = render_dependencies_panel(&pkg).to_string();
        assert!(html.contains("wasi:io"));
        assert!(html.contains("@ 0.2.0"));
    }
}
