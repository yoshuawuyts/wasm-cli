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
    if let Some(section) = render_section("Interfaces", &interfaces) {
        body.push(section);
    }
    if let Some(section) = render_section("Components", &components) {
        body.push(section);
    }

    if packages.is_empty() {
        body.division(|div| {
            div.class("py-16 text-center").paragraph(|p| {
                p.class("text-fg-muted")
                    .text("No packages found. The registry may still be syncing.")
            })
        });
    }

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

/// Render the hero area with title, subtitle, and package count.
fn render_hero(total: usize) -> Division {
    let mut hero = Division::builder();
    hero.class("pb-12 border-b border-border mb-12");
    hero.heading_1(|h1| {
        h1.class("text-3xl font-bold tracking-tight")
            .text("WebAssembly Package Registry")
    });
    hero.paragraph(|p| {
        p.class("text-fg-secondary mt-3 max-w-[50ch]")
            .text("Browse WebAssembly components and WIT interfaces published to OCI registries.")
    });
    if total > 0 {
        hero.paragraph(|p| {
            p.class("text-sm text-fg-faint mt-4")
                .text(format!("{total} packages indexed"))
        });
    }
    hero.build()
}

/// Split packages into (components, interfaces) based on WIT metadata.
fn split_by_kind(packages: &[KnownPackage]) -> (Vec<&KnownPackage>, Vec<&KnownPackage>) {
    let mut components = Vec::new();
    let mut interfaces = Vec::new();

    for pkg in packages {
        // Packages without WIT metadata go into components as a fallback
        if pkg.wit_namespace.is_none() {
            components.push(pkg);
        } else {
            interfaces.push(pkg);
        }
    }

    (components, interfaces)
}

/// Render a section with a heading, a grid of package rows, and a "view all" link.
fn render_section(heading: &str, packages: &[&KnownPackage]) -> Option<Section> {
    if packages.is_empty() {
        return None;
    }

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

    // Package list — compact rows instead of card grid
    let mut list = Division::builder();
    list.class("divide-y divide-border-light");
    for pkg in visible {
        list.push(render_row(pkg));
    }
    section.push(list.build());

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

    Some(section.build())
}

/// Render a single package as a compact row.
fn render_row(pkg: &KnownPackage) -> Division {
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
                        "flex items-baseline gap-3 py-3 hover:bg-surface -mx-2 px-2 rounded transition-colors",
                    )
                    .span(|s| {
                        s.class("font-semibold text-accent shrink-0")
                            .text(display_name)
                    })
                    .span(|s| {
                        s.class("text-sm text-fg-faint shrink-0")
                            .text(version.to_owned())
                    })
                    .span(|s| {
                        s.class("text-sm text-fg-muted truncate")
                            .text(description.to_owned())
                    })
            })
            .build(),
        _ => Division::builder()
            .class("flex items-baseline gap-3 py-3 -mx-2 px-2 rounded")
            .span(|s| {
                s.class("font-semibold text-fg shrink-0")
                    .text(display_name)
            })
            .span(|s| {
                s.class("text-sm text-fg-faint shrink-0")
                    .text(version.to_owned())
            })
            .span(|s| {
                s.class("text-sm text-fg-muted truncate")
                    .text(description.to_owned())
            })
            .build(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn package(wit_namespace: Option<&str>) -> KnownPackage {
        KnownPackage {
            registry: "ghcr.io".to_string(),
            repository: "example/pkg".to_string(),
            description: None,
            tags: vec!["1.0.0".to_string()],
            signature_tags: vec![],
            attestation_tags: vec![],
            last_seen_at: "2026-01-01T00:00:00Z".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            wit_namespace: wit_namespace.map(ToOwned::to_owned),
            wit_name: Some("demo".to_string()),
            dependencies: vec![],
        }
    }

    // r[verify frontend.pages.home]
    #[test]
    fn split_by_kind_uses_wit_namespace_presence() {
        let interface = package(Some("wasi"));
        let component = package(None);
        let input = vec![interface, component];

        let (components, interfaces) = split_by_kind(&input);
        assert_eq!(components.len(), 1);
        assert_eq!(interfaces.len(), 1);
        assert!(components[0].wit_namespace.is_none());
        assert!(interfaces[0].wit_namespace.is_some());
    }
}
