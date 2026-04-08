//! Front page — recently updated components and interfaces.

// r[impl frontend.pages.home]

use html::content::Section;
use html::text_content::Division;
use wasm_meta_registry_client::KnownPackage;

use crate::layout;
use wasm_meta_registry_client::{ApiError, RegistryClient};

/// Maximum number of packages to show per section on the home page.
const HOME_SECTION_LIMIT: usize = 6;

/// Fetch recent packages and render the home page.
pub(crate) async fn render(client: &RegistryClient) -> String {
    match client.fetch_recent_packages(50).await {
        Ok(packages) => render_packages(&packages),
        Err(err) => render_error(&err),
    }
}

/// Render the home page with a list of packages.
fn render_packages(packages: &[KnownPackage]) -> String {
    let (components, interfaces) = split_by_kind(packages);

    let mut body = Division::builder();

    // Hero area
    body.push(render_hero(packages.len()));

    // Package sections with generous separation
    body.push(render_section("Interfaces", &interfaces));
    body.push(render_section("Components", &components));

    layout::document("Home", &body.build().to_string())
}

/// Render the home page with an API error message.
fn render_error(err: &ApiError) -> String {
    let mut body = Division::builder();
    body.push(render_hero(0));
    body.division(|div| {
        div.class("py-16 text-center")
            .paragraph(|p| {
                p.class("text-fg font-semibold")
                    .text("Unable to load packages")
            })
            .paragraph(|p| p.class("text-sm text-fg-muted mt-2").text(err.to_string()))
    });
    layout::document("Home", &body.build().to_string())
}

/// Render the hero area with heading, search form, and quick-install hint.
fn render_hero(total: usize) -> Division {
    let placeholder = if total > 0 {
        format!("Search {total} packages\u{2026}")
    } else {
        "Search packages\u{2026}".to_owned()
    };

    let mut hero = Division::builder();
    hero.class("pb-12 border-b border-border mb-12");
    hero.heading_1(|h1| {
        h1.class("text-3xl font-bold tracking-tight")
            .text("WebAssembly Package Registry")
    });

    // Search — the primary action
    hero.form(|form| {
        form.action("/search")
            .method("get")
            .class("mt-6 flex max-w-lg")
            .input(|input| {
                input
                    .type_("search")
                    .name("q")
                    .placeholder(placeholder)
                    .aria_label("Search packages")
                    .autofocus(true)
                    .class("flex-1 px-4 py-2.5 rounded-l-md text-base border border-border bg-surface text-fg placeholder:text-fg-faint focus:border-accent focus:outline-none transition-colors")
            })
            .button(|btn| {
                btn.type_("submit")
                    .class("px-5 py-2.5 rounded-r-md text-sm font-medium bg-accent text-white hover:bg-accent-hover border border-accent transition-colors")
                    .text("Search")
            })
    });

    // Quick-install hint — communicates what this tool does
    hero.paragraph(|p| {
        p.class("mt-4 text-sm text-fg-muted")
            .text("Get started: ")
            .code(|code| {
                code.class(
                    "font-mono text-fg-secondary bg-surface-muted px-1.5 py-0.5 rounded text-xs",
                )
                .text("wasm install wasi:http")
            })
    });

    hero.build()
}

/// Split packages into (components, interfaces) based on package kind.
fn split_by_kind(packages: &[KnownPackage]) -> (Vec<&KnownPackage>, Vec<&KnownPackage>) {
    let mut components = Vec::new();
    let mut interfaces = Vec::new();

    for pkg in packages {
        match pkg.kind {
            Some(wasm_meta_registry_client::PackageKind::Interface) => interfaces.push(pkg),
            _ => components.push(pkg),
        }
    }

    (components, interfaces)
}

/// Render a section with a heading, a grid of package rows, and a "view all" link.
fn render_section(heading: &str, packages: &[&KnownPackage]) -> Section {
    let has_more = packages.len() > HOME_SECTION_LIMIT;
    let visible = packages.get(..HOME_SECTION_LIMIT).unwrap_or(packages);

    let (icon, subtitle) = match heading {
        "Interfaces" => (
            "⬡",
            "WIT interface definitions for composable WebAssembly modules",
        ),
        "Components" => ("◈", "Standalone WebAssembly components ready to use"),
        _ => ("", ""),
    };

    let mut section = Section::builder();
    section.class("mb-16");

    // Section header with icon, description, and count
    section.division(|div| {
        div.class("mb-4")
            .division(|row| {
                row.class("flex items-baseline justify-between")
                    .heading_2(|h2| {
                        h2.class("text-lg font-semibold")
                            .text(format!("{icon} {heading}"))
                    })
                    .span(|s| {
                        s.class("text-sm text-fg-faint")
                            .text(format!("{}", packages.len()))
                    })
            })
            .paragraph(|p| {
                p.class("text-sm text-fg-muted mt-1")
                    .text(subtitle.to_owned())
            })
    });

    if packages.is_empty() {
        // Empty state
        section.paragraph(|p| {
            p.class("py-8 text-sm text-fg-faint")
                .text(format!("No {heading} found yet."))
        });
    } else {
        // Package grid — card layout
        let mut grid = Division::builder();
        grid.class("grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4");
        for pkg in visible {
            grid.push(render_card(pkg));
        }
        section.push(grid.build());

        // "View all" link
        if has_more {
            section.paragraph(|p| {
                p.class("mt-4").anchor(|a| {
                    a.href("/all")
                        .class("text-sm text-accent hover:underline")
                        .text(format!("View all {heading} →"))
                })
            });
        }
    }

    section.build()
}

/// Render a single package as a card.
fn render_card(pkg: &KnownPackage) -> Division {
    let display_name = match (&pkg.wit_namespace, &pkg.wit_name) {
        (Some(ns), Some(name)) => format!("{ns}:{name}"),
        _ => pkg.repository.clone(),
    };

    let description = pkg
        .description
        .as_deref()
        .unwrap_or("No description available");

    let version = pkg.tags.first().map_or("—", String::as_str);

    match (&pkg.wit_namespace, &pkg.wit_name) {
        (Some(ns), Some(name)) => Division::builder()
            .anchor(|a| {
                a.href(format!("/{ns}/{name}"))
                    .class(
                        "block border border-border rounded-lg p-4 hover:border-accent/40 hover:bg-surface transition-colors",
                    )
                    .span(|s| {
                        s.class("block font-semibold text-accent truncate")
                            .text(display_name)
                    })
                    .span(|s| {
                        s.class("block text-sm text-fg-muted mt-1 line-clamp-2")
                            .text(description.to_owned())
                    })
                    .span(|s| {
                        s.class("block text-xs text-fg-faint mt-3 font-mono")
                            .text(version.to_owned())
                    })
            })
            .build(),
        _ => Division::builder()
            .class("border border-border rounded-lg p-4")
            .span(|s| {
                s.class("block font-semibold text-fg truncate")
                    .text(display_name)
            })
            .span(|s| {
                s.class("block text-sm text-fg-muted mt-1 line-clamp-2")
                    .text(description.to_owned())
            })
            .span(|s| {
                s.class("block text-xs text-fg-faint mt-3 font-mono")
                    .text(version.to_owned())
            })
            .build(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn package(kind: Option<wasm_meta_registry_client::PackageKind>) -> KnownPackage {
        KnownPackage {
            registry: "ghcr.io".to_string(),
            repository: "example/pkg".to_string(),
            kind,
            description: None,
            tags: vec!["1.0.0".to_string()],
            signature_tags: vec![],
            attestation_tags: vec![],
            last_seen_at: "2026-01-01T00:00:00Z".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            wit_namespace: Some("test".to_string()),
            wit_name: Some("demo".to_string()),
            dependencies: vec![],
        }
    }

    // r[verify frontend.pages.home]
    #[test]
    fn split_by_kind_uses_package_kind() {
        use wasm_meta_registry_client::PackageKind;

        let interface = package(Some(PackageKind::Interface));
        let component = package(Some(PackageKind::Component));
        let unknown = package(None);
        let input = vec![interface, component, unknown];

        let (components, interfaces) = split_by_kind(&input);
        assert_eq!(interfaces.len(), 1);
        assert_eq!(components.len(), 2);
        assert_eq!(interfaces[0].kind, Some(PackageKind::Interface));
        assert_eq!(components[0].kind, Some(PackageKind::Component));
        assert_eq!(components[1].kind, None);
    }
}
