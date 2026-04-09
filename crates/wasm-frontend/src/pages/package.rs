//! Package detail page.

// r[impl frontend.pages.package-detail]

use crate::wit_doc::WitDocument;
use html::content::Section;
use html::inline_text::Span;
use html::text_content::{Division, ListItem, UnorderedList};
use wasm_meta_registry_client::{KnownPackage, PackageVersion};

use crate::layout;

/// Which tab is currently active on the package detail page.
pub(crate) enum ActiveTab<'a> {
    /// WIT definition, worlds, and dependencies.
    Docs {
        version_detail: Option<&'a PackageVersion>,
    },
    /// Packages that export/implement this interface.
    Providers { exporters: &'a [KnownPackage] },
    /// Packages that import/consume this interface.
    Dependents { importers: &'a [KnownPackage] },
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

    // Header: title + description on left, metadata on right
    let version_detail = match tab {
        ActiveTab::Docs { version_detail } => *version_detail,
        _ => None,
    };
    body.push(render_page_header(
        pkg,
        &display_name,
        description,
        version,
        version_detail,
    ));

    // Tab bar + active panel
    let url_base = match (&pkg.wit_namespace, &pkg.wit_name) {
        (Some(ns), Some(name)) => format!("/{ns}/{name}/{version}"),
        _ => format!("/{}/{version}", pkg.repository),
    };
    body.push(render_tab_bar(&url_base, tab));

    // Parse WIT doc early so we can show the nav sidebar.
    let wit_doc = version_detail.and_then(|d| try_parse_wit(d, &url_base));

    // Grid: main content + optional sidebar
    let mut grid = Division::builder();
    if wit_doc.is_some() {
        grid.class("grid grid-cols-1 md:grid-cols-3 gap-12");
    }

    // Main content column
    let mut main_col = Division::builder();
    if wit_doc.is_some() {
        main_col.class("md:col-span-2 space-y-8");
    } else {
        main_col.class("space-y-8");
    }
    match tab {
        ActiveTab::Docs { version_detail } => {
            if let Some(detail) = version_detail {
                main_col.push(render_wit_content_with_doc(
                    detail,
                    &url_base,
                    wit_doc.as_ref(),
                ));
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

    // Sidebar (only when WIT doc is available)
    if let Some(doc) = &wit_doc {
        let sidebar_ctx = super::sidebar::SidebarContext {
            display_name: &display_name,
            version,
            doc,
            active: super::sidebar::SidebarActive::Interface(""),
        };
        grid.push(super::sidebar::render_sidebar(&sidebar_ctx));
    }

    body.push(grid.build());

    layout::document(&display_name, &body.build().to_string())
}

/// Render the install command section with a copy button.
fn render_install_command(display_name: &str, version: &str) -> Division {
    let command = format!("wasm install {display_name}@{version}");
    let command_js = serde_json::to_string(&command).expect("serialize install command");

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
        .class("max-w-lg group/install")
        .division(|div| {
            div.class(
                "flex items-center gap-2 bg-surface-muted border border-border \
                 rounded-md px-3 py-2 font-mono text-xs text-fg",
            )
            .code(|code| {
                code.class("flex-1 select-all overflow-x-auto whitespace-nowrap")
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

/// Render the WIT content section for a package version.
///
/// When a pre-parsed `WitDocument` is available, show interfaces and worlds
/// as navigable cards.  Otherwise fall back to the world summaries that the
/// registry extracted at index time plus the raw WIT text block.
fn render_wit_content_with_doc(
    detail: &PackageVersion,
    _url_base: &str,
    doc: Option<&WitDocument>,
) -> Section {
    let mut section = Section::builder();

    if let Some(doc) = doc {
        if !doc.worlds.is_empty() {
            section.push(render_world_overview(doc));
        }
        if !doc.interfaces.is_empty() {
            section.push(render_interface_overview(doc));
        }
    } else {
        // Fallback: show pre-extracted world summaries + raw WIT text.
        if !detail.worlds.is_empty() {
            section.push(render_world_summaries(detail));
        }
        // Only show the raw WIT text if it's genuine WIT (not lossy
        // debug output that contains patterns like `type foo: "type"`
        // or `interface-Id { idx: 0 }`).
        if let Some(wit_text) = &detail.wit_text
            && !is_lossy_wit(wit_text)
        {
            section.push(render_raw_wit(wit_text));
        }
    }

    section.build()
}

/// Try parsing the WIT text into a rich document model.
fn try_parse_wit(detail: &PackageVersion, url_base: &str) -> Option<WitDocument> {
    let wit_text = detail.wit_text.as_deref()?;
    let dep_urls = build_dep_urls(&detail.dependencies);
    crate::wit_doc::parse_wit_doc(wit_text, url_base, &dep_urls).ok()
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
    container.class("space-y-3 mt-10");
    container.heading_2(|h2| {
        h2.class("text-sm font-semibold text-fg-muted uppercase tracking-wide mb-2")
            .text("Interfaces")
    });

    let mut ul = UnorderedList::builder();
    ul.class("space-y-0.5");
    for iface in &doc.interfaces {
        ul.push(render_interface_row(iface));
    }
    container.push(ul.build());
    container.build()
}

/// Render a single interface row: linked name + doc excerpt.
fn render_interface_row(iface: &crate::wit_doc::InterfaceDoc) -> ListItem {
    let mut li = ListItem::builder();
    li.class("py-3 flex gap-6");

    li.division(|left| {
        left.class("shrink-0 w-52").anchor(|a| {
            a.href(iface.url.clone())
                .class("font-mono text-sm font-semibold text-wit-iface hover:underline")
                .text(iface.name.clone())
        })
    });

    // Right: doc excerpt
    if let Some(docs) = &iface.docs {
        li.division(|right| {
            right
                .class("text-sm leading-relaxed text-fg-secondary min-w-0")
                .text(first_sentence(docs))
        });
    }

    li.build()
}

/// Render the worlds overview section.
fn render_world_overview(doc: &WitDocument) -> Division {
    let mut container = Division::builder();
    container.class("space-y-3");
    container.heading_2(|h2| {
        h2.class("text-sm font-semibold text-fg-muted uppercase tracking-wide mb-2")
            .text("Worlds")
    });

    let mut ul = UnorderedList::builder();
    ul.class("space-y-0.5");
    for world in &doc.worlds {
        ul.push(render_world_row(world));
    }
    container.push(ul.build());
    container.build()
}

/// Render a single world row: linked name + doc excerpt.
fn render_world_row(world: &crate::wit_doc::WorldDoc) -> ListItem {
    let mut li = ListItem::builder();
    li.class("py-3 flex gap-6");

    li.division(|left| {
        left.class("shrink-0 w-52").anchor(|a| {
            a.href(world.url.clone())
                .class("font-mono text-sm font-semibold text-wit-world hover:underline")
                .text(world.name.clone())
        })
    });

    // Right: doc excerpt
    if let Some(docs) = &world.docs {
        li.division(|right| {
            right
                .class("text-sm leading-relaxed text-fg-secondary min-w-0")
                .text(first_sentence(docs))
        });
    }

    li.build()
}

/// Render raw WIT text in a pre-formatted code block (fallback).
fn render_raw_wit(wit_text: &str) -> Division {
    Division::builder()
        .heading_2(|h2| {
            h2.class("text-sm font-semibold text-fg-muted uppercase tracking-wide mb-3")
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

/// Render world summaries from pre-extracted `PackageVersion` data (fallback
/// when the WIT text cannot be parsed into a rich document).
fn render_world_summaries(detail: &PackageVersion) -> Division {
    let mut container = Division::builder();
    container.class("space-y-8");

    for world in &detail.worlds {
        container.division(|world_div| {
            world_div.heading_2(|h2| {
                h2.class("text-sm font-semibold text-fg-muted uppercase tracking-wide mb-3")
                    .text(format!("world {}", world.name))
            });

            if let Some(desc) = &world.description {
                world_div
                    .paragraph(|p| p.class("text-fg-secondary text-sm mb-3").text(desc.clone()));
            }

            if !world.imports.is_empty() {
                world_div.push(render_iface_ref_list("Imports", &world.imports));
            }
            if !world.exports.is_empty() {
                world_div.push(render_iface_ref_list("Exports", &world.exports));
            }
            world_div
        });
    }

    container.build()
}

/// Render a list of WIT interface references (fallback).
fn render_iface_ref_list(
    label: &str,
    interfaces: &[wasm_meta_registry_client::WitInterfaceRef],
) -> Division {
    let mut div = Division::builder();
    div.class("mb-4");
    div.heading_3(|h3| {
        h3.class("text-xs font-medium text-fg-muted mb-2")
            .text(label.to_owned())
    });

    let mut ul = UnorderedList::builder();
    ul.class("space-y-0.5");
    for iface in interfaces {
        let display = format_iface_ref(iface);
        ul.list_item(|li| {
            li.class("py-1.5")
                .span(|s| s.class("text-sm font-mono text-accent").text(display))
        });
    }
    div.push(ul.build());
    div.build()
}

/// Format a WIT interface reference as a display string.
fn format_iface_ref(iface: &wasm_meta_registry_client::WitInterfaceRef) -> String {
    let mut s = iface.package.clone();
    if let Some(name) = &iface.interface {
        s.push('/');
        s.push_str(name);
    }
    if let Some(v) = &iface.version {
        s.push('@');
        s.push_str(v);
    }
    s
}

/// Extract the first sentence from a doc comment for summary display.
fn first_sentence(text: &str) -> String {
    text.split_once(". ")
        .map_or_else(|| text.to_owned(), |(first, _)| format!("{first}."))
}

/// Detect whether WIT text is the lossy hand-rolled format rather than
/// genuine parseable WIT.  The lossy format contains debug patterns like
/// `type foo: "type"` and `interface-Id { idx: 0 }`.
fn is_lossy_wit(text: &str) -> bool {
    text.contains(": \"type\"")
        || text.contains(": \"record\"")
        || text.contains(": \"variant\"")
        || text.contains("interface-Id {")
}

/// Render the tab bar with links to each tab route.
fn render_tab_bar(url_base: &str, active: &ActiveTab<'_>) -> Division {
    let active_class = "text-accent border-b-2 border-accent font-semibold";
    let inactive_class = "text-fg-muted hover:text-fg";
    let tab_base = "px-4 py-2 text-sm transition-colors inline-block";

    let tabs: &[(&str, &str, bool)] = &[
        (
            "Documentation",
            url_base,
            matches!(active, ActiveTab::Docs { .. }),
        ),
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
        return None;
    }

    let mut section = Section::builder();
    section.heading_2(|h2| h2.class("text-lg font-semibold mb-3").text("Dependencies"));

    let mut ul = UnorderedList::builder();
    ul.class("space-y-1");
    for dep in &pkg.dependencies {
        let mut li = ListItem::builder();
        li.class("text-sm");
        let dep_span = Span::builder()
            .class("text-accent")
            .text(dep.package.clone())
            .build();
        li.push(dep_span);
        if let Some(v) = &dep.version {
            li.push(Span::builder().class("text-fg-faint").text(" @ ").build());
            let version_span = Span::builder()
                .class("text-fg-faint")
                .text(v.clone())
                .build();
            li.push(version_span);
        }
        ul.push(li.build());
    }
    section.push(ul.build());

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
        let desc = pkg
            .description
            .as_deref()
            .unwrap_or("No description available");

        ul.list_item(|li| {
            let li = li.class("text-sm");
            let li = match (&pkg.wit_namespace, &pkg.wit_name) {
                (Some(ns), Some(n)) => li.anchor(|a| {
                    a.href(format!("/{ns}/{n}"))
                        .class("text-accent hover:underline font-medium")
                        .text(name.clone())
                }),
                _ => li.push(
                    Span::builder()
                        .class("text-accent font-medium")
                        .text(name.clone())
                        .build(),
                ),
            };

            li.push(
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

/// Render the page header: title + description on left, metadata on right.
fn render_page_header(
    pkg: &KnownPackage,
    display_name: &str,
    description: &str,
    current_version: &str,
    version_detail: Option<&PackageVersion>,
) -> Division {
    let url_name = match (&pkg.wit_namespace, &pkg.wit_name) {
        (Some(ns), Some(name)) => format!("{ns}/{name}"),
        _ => pkg.repository.clone(),
    };

    let annotations = version_detail.and_then(|d| d.annotations.as_ref());

    let mut header = Division::builder();
    header.class("flex flex-col md:flex-row md:items-start md:justify-between gap-6 mb-6");

    // Left: title with inline version + description + install command
    header.division(|left| {
        left.class("flex-1 min-w-0")
            .division(|title_row| {
                title_row
                    .class("flex items-baseline gap-2 flex-wrap")
                    .heading_1(|h1| {
                        h1.class("text-3xl font-bold tracking-tight text-accent")
                            .text(display_name.to_owned())
                    })
                    .push(render_version_inline(pkg, current_version, &url_name))
            })
            .paragraph(|p| {
                p.class("text-fg-secondary mt-1")
                    .text(description.to_owned())
            })
            .division(|d| {
                d.class("mt-4")
                    .push(render_install_command(display_name, current_version))
            })
    });

    // Right: metadata rows
    header.division(|right| {
        right.class("shrink-0 md:w-72 space-y-3 text-sm");

        // Metadata rows
        let mut meta = Division::builder();
        meta.class("space-y-2 text-xs leading-relaxed");

        // Source/repository
        if let Some(source) = annotations.and_then(|a| a.source.as_deref()) {
            meta.push(meta_link_row("Repository", &abbreviate_url(source), source));
        } else {
            let repo_url = format!("https://{}/{}", pkg.registry, pkg.repository);
            let repo_display = format!("{}/{}", pkg.registry, pkg.repository);
            meta.push(meta_link_row("Repository", &repo_display, &repo_url));
        }

        if let Some(license) = annotations.and_then(|a| a.licenses.as_deref()) {
            meta.push(meta_row("License", license));
        }
        if let Some(kind) = &pkg.kind {
            meta.push(meta_row("Kind", &kind.to_string()));
        }
        if let Some(size) = version_detail.and_then(|d| d.size_bytes) {
            meta.push(meta_row("Size", &format_size(size)));
        }
        if let Some(created) = version_detail.and_then(|d| d.created_at.as_deref()) {
            meta.push(meta_row("Published", &format_date(created)));
        }
        if let Some(docs_url) = annotations.and_then(|a| a.documentation.as_deref()) {
            meta.push(meta_link_row("Docs", &abbreviate_url(docs_url), docs_url));
        }
        let source = annotations.and_then(|a| a.source.as_deref());
        if let Some(url) = annotations.and_then(|a| a.url.as_deref())
            && source != Some(url)
        {
            meta.push(meta_link_row("Homepage", &abbreviate_url(url), url));
        }

        right.push(meta.build());
        right
    });

    header.build()
}

/// Render a label: value metadata row.
fn meta_row(label: &str, value: &str) -> Division {
    Division::builder()
        .class("flex gap-2")
        .span(|s| {
            s.class("text-fg-muted w-20 shrink-0")
                .text(label.to_owned())
        })
        .span(|s| s.class("text-fg truncate").text(value.to_owned()))
        .build()
}

/// Render a label: linked-value metadata row.
fn meta_link_row(label: &str, text: &str, href: &str) -> Division {
    Division::builder()
        .class("flex gap-2")
        .span(|s| {
            s.class("text-fg-muted w-20 shrink-0")
                .text(label.to_owned())
        })
        .anchor(|a| {
            a.href(href.to_owned())
                .class("text-accent hover:underline truncate")
                .text(text.to_owned())
        })
        .build()
}

/// Render the inline version selector: `@ <select>` next to the package title.
fn render_version_inline(pkg: &KnownPackage, current_version: &str, url_name: &str) -> Division {
    let mut select = html::forms::Select::builder();
    select
        .id("version-select")
        .name("version")
        .class("px-1.5 py-0.5 rounded border border-border bg-surface text-fg text-xl font-bold cursor-pointer");

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
        if version_count == 1 { "version" } else { "versions" }
    );

    Division::builder()
        .class("flex items-baseline gap-1")
        .span(|s| s.class("text-xl text-fg-muted font-bold").text("@"))
        .push(select.build())
        .script(|s| s.text(script_body))
        .build()
}

/// Format a byte count as a human-readable size string.
#[allow(clippy::cast_precision_loss)]
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

/// Abbreviate a URL for display (strip scheme and trailing slash).
fn abbreviate_url(url: &str) -> String {
    url.strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(url)
        .trim_end_matches('/')
        .to_owned()
}

/// Format an ISO 8601 timestamp as a short date (YYYY-MM-DD).
fn format_date(iso: &str) -> String {
    // Take just the date portion of "2026-03-05T23:36:11Z"
    iso.split('T').next().unwrap_or(iso).to_owned()
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
    use crate::wit_doc::{InterfaceDoc, WitDocument, WorldDoc};
    use wasm_meta_registry_client::{
        PackageDependencyRef, PackageVersion, WitInterfaceRef, WitWorldSummary,
    };

    fn sample_known_package(wit: bool) -> KnownPackage {
        KnownPackage {
            registry: "ghcr.io".to_string(),
            repository: "example/pkg".to_string(),
            kind: None,
            description: Some("Example package".to_string()),
            tags: vec!["1.0.0".to_string()],
            signature_tags: vec![],
            attestation_tags: vec![],
            last_seen_at: "2026-01-01T00:00:00Z".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            wit_namespace: wit.then(|| "wasi".to_string()),
            wit_name: wit.then(|| "demo".to_string()),
            dependencies: vec![],
        }
    }

    fn sample_version(wit_text: Option<&str>) -> PackageVersion {
        PackageVersion {
            tag: Some("1.0.0".to_string()),
            digest: "sha256:abc".to_string(),
            size_bytes: Some(123),
            created_at: Some("2026-01-01T00:00:00Z".to_string()),
            synced_at: Some("2026-01-02T00:00:00Z".to_string()),
            annotations: None,
            worlds: vec![],
            components: vec![],
            dependencies: vec![],
            referrers: vec![],
            wit_text: wit_text.map(str::to_string),
        }
    }

    #[test]
    fn dependency_versions_include_separator() {
        let mut pkg = sample_known_package(true);
        pkg.dependencies = vec![PackageDependencyRef {
            package: "wasi:io".to_string(),
            version: Some("0.2.0".to_string()),
        }];

        let html = render_dependencies_panel(&pkg).to_string();
        assert!(html.contains("wasi:io"));
        assert!(html.contains("@ 0.2.0"));
    }

    #[test]
    fn docs_tab_renders_world_and_interface_overviews_from_wit_doc() {
        let version = sample_version(None);
        let doc = WitDocument {
            package_name: "wasi:demo".to_string(),
            version: Some("1.0.0".to_string()),
            docs: None,
            interfaces: vec![InterfaceDoc {
                name: "types".to_string(),
                docs: Some("Interface docs. Extra details.".to_string()),
                types: vec![],
                functions: vec![],
                url: "/wasi/demo/1.0.0/interface/types".to_string(),
            }],
            worlds: vec![WorldDoc {
                name: "command".to_string(),
                docs: Some("World docs. More context.".to_string()),
                imports: vec![],
                exports: vec![],
                url: "/wasi/demo/1.0.0/world/command".to_string(),
            }],
        };

        let html =
            render_wit_content_with_doc(&version, "/wasi/demo/1.0.0", Some(&doc)).to_string();
        assert!(html.contains("Worlds"));
        assert!(html.contains("Interfaces"));
        assert!(html.contains("command"));
        assert!(html.contains("types"));
        assert!(html.contains("World docs."));
        assert!(html.contains("Interface docs."));
    }

    #[test]
    fn docs_tab_fallback_renders_world_summary_and_raw_wit_for_parseable_text() {
        let mut version = sample_version(Some("package wasi:demo@1.0.0;\nworld command {}"));
        version.worlds = vec![WitWorldSummary {
            name: "command".to_string(),
            description: Some("Fallback world summary".to_string()),
            imports: vec![WitInterfaceRef {
                package: "wasi:io".to_string(),
                interface: Some("streams".to_string()),
                version: Some("0.2.0".to_string()),
            }],
            exports: vec![],
        }];

        let html = render_wit_content_with_doc(&version, "/wasi/demo/1.0.0", None).to_string();
        assert!(html.contains("world command"));
        assert!(html.contains("Imports"));
        assert!(html.contains("wasi:io/streams@0.2.0"));
        assert!(html.contains("WIT Definition"));
    }

    #[test]
    fn docs_tab_fallback_hides_raw_wit_for_lossy_text() {
        let version = sample_version(Some("type foo: \"type\"\ninterface-Id { idx: 0 }"));
        let html = render_wit_content_with_doc(&version, "/wasi/demo/1.0.0", None).to_string();
        assert!(!html.contains("WIT Definition"));
    }

    #[test]
    fn dependents_list_renders_non_wit_packages_without_anchor() {
        let non_wit = sample_known_package(false);
        let wit = sample_known_package(true);
        let html = render_filterable_package_list("list-all", &[&non_wit, &wit], false).to_string();

        assert!(html.contains("display:none"));
        assert!(html.contains("wasi:demo"));
        assert!(html.contains("href=\"/wasi/demo\""));
        assert!(!html.contains("href=\"#\""));
        assert!(html.contains("example/pkg"));
    }
}
