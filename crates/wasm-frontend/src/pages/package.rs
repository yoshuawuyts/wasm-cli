//! Package detail page.

// r[impl frontend.pages.package-detail]

use html::content::{Aside, Navigation, Section};
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

    match tab {
        ActiveTab::Docs { version_detail } => {
            body.push(render_docs_panel(pkg, version, &display_name, *version_detail));
        }
        ActiveTab::Providers { exporters } => {
            body.push(render_package_list(exporters));
        }
        ActiveTab::Dependents { importers } => {
            body.push(render_package_list(importers));
        }
    }

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
/// For interfaces, displays the full WIT source text.
/// For components, displays the world imports and exports.
fn render_wit_content(detail: &PackageVersion) -> Section {
    let mut section = Section::builder();

    if !detail.worlds.is_empty() {
        section.push(render_worlds(detail));
    }

    if let Some(wit_text) = &detail.wit_text {
        section.heading_2(|h2| {
            h2.class("text-lg font-semibold mb-3")
                .text("WIT Definition")
        });
        section.push(
            html::text_content::PreformattedText::builder()
                .class("bg-surface-muted border border-border rounded-lg p-4 overflow-x-auto text-sm leading-relaxed")
                .code(|code| code.class("text-fg").text(wit_text.clone()))
                .build(),
        );
    }

    section.build()
}

/// Render the worlds section showing imports and exports.
fn render_worlds(detail: &PackageVersion) -> Division {
    let mut container = Division::builder();
    container.class("space-y-6");

    for world in &detail.worlds {
        let mut world_div = Division::builder();
        world_div.class("space-y-3");
        world_div.heading_2(|h2| {
            h2.class("text-lg font-semibold")
                .text(format!("world {}", world.name))
        });

        if let Some(desc) = &world.description {
            world_div.paragraph(|p| p.class("text-fg-secondary text-sm").text(desc.clone()));
        }

        if !world.imports.is_empty() {
            world_div.push(render_interface_list("Imports", &world.imports));
        }
        if !world.exports.is_empty() {
            world_div.push(render_interface_list("Exports", &world.exports));
        }
        container.push(world_div.build());
    }

    container.build()
}

/// Render a list of WIT interface references (imports or exports).
fn render_interface_list(
    label: &str,
    interfaces: &[wasm_meta_registry_client::WitInterfaceRef],
) -> Division {
    let mut div = Division::builder();
    div.heading_3(|h3| {
        h3.class("text-sm font-semibold text-fg-muted uppercase tracking-wide mb-2")
            .text(label.to_owned())
    });

    let mut ul = UnorderedList::builder();
    ul.class("space-y-1 ml-1");
    for iface in interfaces {
        let display = format_interface_ref(iface);
        ul.list_item(|li| {
            li.class("text-sm font-mono")
                .span(|s| s.class("text-accent").text(display))
        });
    }
    div.push(ul.build());
    div.build()
}

/// Format a WIT interface reference as a display string.
fn format_interface_ref(iface: &wasm_meta_registry_client::WitInterfaceRef) -> String {
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

/// Render the dependencies section.
fn render_dependencies(pkg: &KnownPackage) -> Option<Section> {
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

    Some(section.build())
}

/// Render the tab bar with links to each tab route.
fn render_tab_bar(url_base: &str, active: &ActiveTab<'_>) -> Division {
    let active_class = "text-accent border-b-2 border-accent font-semibold";
    let inactive_class = "text-fg-muted hover:text-fg";
    let tab_base = "px-4 py-2 text-sm transition-colors inline-block";

    let tabs: &[(&str, &str, bool)] = &[
        ("Docs", url_base, matches!(active, ActiveTab::Docs { .. })),
        (
            "Providers",
            &format!("{url_base}/providers"),
            matches!(active, ActiveTab::Providers { .. }),
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
                let style = if is_active { active_class } else { inactive_class };
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

/// Render the docs panel containing WIT content, dependencies, and sidebar.
fn render_docs_panel(
    pkg: &KnownPackage,
    version: &str,
    display_name: &str,
    version_detail: Option<&PackageVersion>,
) -> Division {
    let mut panel = Division::builder();
    panel.id("panel-docs");

    let mut grid = Division::builder();
    grid.class("grid grid-cols-1 md:grid-cols-3 gap-12");

    // Main content column
    let mut main_col = Division::builder();
    main_col.class("md:col-span-2 space-y-8");
    if let Some(detail) = version_detail {
        main_col.push(render_wit_content(detail));
    }
    if let Some(deps) = render_dependencies(pkg) {
        main_col.push(deps);
    }
    grid.push(main_col.build());

    // Sidebar
    grid.push(render_sidebar(pkg, version, display_name, version_detail));

    panel.push(grid.build());
    panel.build()
}

/// Render a list of packages for the providers/dependents tabs.
fn render_package_list(packages: &[KnownPackage]) -> Division {
    let mut div = Division::builder();

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
    card.push(sidebar_row("Created", &pkg.created_at));
    card.push(sidebar_row("Last updated", &pkg.last_seen_at));
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

    Division::builder()
        .division(|dt| {
            dt.class("text-fg-muted text-xs uppercase tracking-wide")
                .text("Version")
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

        let html = render_dependencies(&pkg)
            .expect("dependencies section should render")
            .to_string();
        assert!(html.contains("wasi:io"));
        assert!(html.contains(" @ "));
        assert!(html.contains("0.2.0"));
    }
}
