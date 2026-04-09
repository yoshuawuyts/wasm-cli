//! Front page — recently updated components and interfaces.

// r[impl frontend.pages.home]

use html::text_content::Division;
use html::text_content::builders::DivisionBuilder;
use wasm_meta_registry_client::KnownPackage;

use crate::layout;
use wasm_meta_registry_client::{ApiError, RegistryClient};

/// Maximum number of packages to show per tab on the home page (4 cols × 10 rows).
const HOME_SECTION_LIMIT: usize = 40;

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

    // Tabbed package listing
    body.push(render_tabs(packages, &interfaces, &components));

    layout::document("Home", &body.build().to_string())
}

/// Render the home page with an API error message.
fn render_error(_err: &ApiError) -> String {
    let mut body = Division::builder();
    body.push(render_hero(0));
    body.division(|div| {
        div.class("py-16 text-center")
            .paragraph(|p| {
                p.class("text-fg font-semibold")
                    .text("Could not load components")
            })
            .paragraph(|p| {
                p.class("text-sm text-fg-muted mt-2")
                    .text("The registry may be temporarily unavailable. Try refreshing the page.")
            })
    });
    layout::document("Home", &body.build().to_string())
}

/// Render the hero area with heading, search form, CTA, and quick-install hint.
fn render_hero(_total: usize) -> Division {
    let mut hero = Division::builder();
    hero.class("pt-8 pb-6");

    // Title and hint — grouped tightly
    hero.heading_1(|h1| {
        h1.class("text-3xl font-bold tracking-tight")
            .text("WebAssembly Component Registry")
    });
    hero.paragraph(|p| {
        p.class("mt-2 text-sm text-fg-muted flex items-center gap-2 flex-wrap")
            .text("Search for programs, libraries, and interfaces across all OCI registries.")
    });

    // Search and CTA — grouped below with generous separation from title
    hero.division(|row| {
        row.class("mt-8 flex flex-col sm:flex-row gap-3 sm:items-center")
            .form(|form| {
                form.action("/search")
                    .method("get")
                    .class("flex flex-1 max-w-lg search-form")
                    .division(|wrapper| {
                        wrapper.class("flex-1 relative")
                            .input(|input| {
                                input
                                    .type_("search")
                                    .name("q")
                                    .id("search-input")
                                    .aria_label("Search components and interfaces")
                                    .autofocus(true)
                                    .class("w-full px-4 pr-8 py-2.5 rounded-l-md text-base border border-border bg-surface text-fg focus:border-accent focus:outline-none search-glow transition-colors")
                            })
                            // Carousel placeholder overlay
                            .span(|overlay| {
                                overlay
                                    .id("search-carousel")
                                    .class("search-carousel")
                                    .aria_hidden(true)
                                    .span(|prefix| {
                                        prefix.text("Search ".to_owned())
                                    })
                                    .span(|word| {
                                        word.id("carousel-word")
                                            .class("carousel-word")
                                            .text("components\u{2026}")
                                    })
                            })
                            .span(|kbd| {
                                kbd.class("search-kbd")
                                    .aria_hidden(true)
                                    .text("/")
                            })
                    })
                    .button(|btn| {
                        btn.type_("submit")
                            .class("px-5 py-2.5 rounded-r-md text-sm font-medium bg-accent text-white hover:bg-accent-hover border border-accent btn-press transition-colors")
                            .text("Search")
                    })
            })
            .anchor(|a| {
                a.href("/docs")
                    .class("text-sm text-fg-muted hover:text-accent transition-colors shrink-0")
                    .text("Publish a component \u{2192}")
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

/// Render the tabbed package listing with All / Interfaces / Components tabs.
fn render_tabs(
    all: &[KnownPackage],
    interfaces: &[&KnownPackage],
    components: &[&KnownPackage],
) -> Division {
    let all_refs: Vec<&KnownPackage> = all.iter().collect();

    let tabs: &[(&str, &str, &[&KnownPackage])] = &[
        ("all", "All", &all_refs),
        ("interfaces", "Interfaces", interfaces),
        ("components", "Components", components),
    ];

    let mut wrapper = Division::builder();
    wrapper.class("tab-group");

    // Tab bar
    let mut bar = Division::builder();
    bar.class("flex gap-1 border-b border-border mb-6");
    bar.role("tablist");
    for (i, &(id, label, pkgs)) in tabs.iter().enumerate() {
        let count = pkgs.len();
        let selected = i == 0;
        bar.button(|btn| {
            btn.type_("button")
                .role("tab")
                .class("tab-btn")
                .data("tab", id)
                .aria_selected(selected)
                .aria_controls_elements(format!("panel-{id}"))
                .span(|s: &mut html::inline_text::builders::SpanBuilder| s.text(label.to_owned()))
                .span(|s: &mut html::inline_text::builders::SpanBuilder| {
                    s.class("ml-1.5 text-xs text-fg-faint")
                        .text(format!("{count}"))
                })
        });
    }
    wrapper.push(bar.build());

    // Panels
    for (i, &(id, _label, pkgs)) in tabs.iter().enumerate() {
        let mut panel = Division::builder();
        panel
            .id(format!("panel-{id}"))
            .role("tabpanel")
            .class("tab-panel");
        if i != 0 {
            panel.style("display:none");
        }
        render_card_grid(&mut panel, pkgs);
        wrapper.push(panel.build());
    }

    wrapper.build()
}

/// Render a grid of package cards into a container, with a "view all" link
/// when the list is truncated.
fn render_card_grid(container: &mut DivisionBuilder, packages: &[&KnownPackage]) {
    if packages.is_empty() {
        container.paragraph(|p| {
            p.class("py-8 text-sm text-fg-faint")
                .text("Nothing published yet. ")
                .anchor(|a| {
                    a.href("/docs")
                        .class("text-accent hover:underline")
                        .text("Learn how to publish")
                })
        });
        return;
    }

    let has_more = packages.len() > HOME_SECTION_LIMIT;
    let visible = packages.get(..HOME_SECTION_LIMIT).unwrap_or(packages);

    let mut grid = Division::builder();
    grid.class("grid grid-cols-1 sm:grid-cols-2 md:grid-cols-3 lg:grid-cols-4 gap-3");
    for (i, pkg) in visible.iter().enumerate() {
        grid.push(render_card(pkg, i));
    }
    container.push(grid.build());

    if has_more {
        container.paragraph(|p| {
            p.class("mt-4").anchor(|a| {
                a.href("/all")
                    .class("text-sm text-accent hover:underline")
                    .text("View all \u{2192}")
            })
        });
    }
}

/// Icon for a package kind.
fn kind_icon(kind: Option<wasm_meta_registry_client::PackageKind>) -> &'static str {
    match kind {
        Some(wasm_meta_registry_client::PackageKind::Interface) => "\u{2b21}",
        _ => "\u{25c8}",
    }
}

/// CSS class for the kind-colored left border.
fn kind_card_class(kind: Option<wasm_meta_registry_client::PackageKind>) -> &'static str {
    match kind {
        Some(wasm_meta_registry_client::PackageKind::Interface) => "card-interface",
        _ => "card-component",
    }
}

/// Tailwind color class for the kind badge icon.
fn kind_icon_color(kind: Option<wasm_meta_registry_client::PackageKind>) -> &'static str {
    match kind {
        Some(wasm_meta_registry_client::PackageKind::Interface) => "text-wit-iface",
        _ => "text-accent",
    }
}

/// Render a single package as a card.
fn render_card(pkg: &KnownPackage, index: usize) -> Division {
    let delay = format!("animation-delay:{}ms", index * 30);
    let display_name = match (&pkg.wit_namespace, &pkg.wit_name) {
        (Some(ns), Some(name)) => format!("{ns}:{name}"),
        _ => pkg.repository.clone(),
    };

    let description = pkg.description.as_deref().unwrap_or("No description");
    let version = crate::pick_redirect_version(&pkg.tags).unwrap_or_else(|| {
        pkg.tags
            .first()
            .cloned()
            .unwrap_or_else(|| "\u{2014}".to_owned())
    });
    let icon = kind_icon(pkg.kind);

    match (&pkg.wit_namespace, &pkg.wit_name) {
        (Some(ns), Some(name)) => Division::builder()
            .class("card-enter")
            .style(delay.clone())
            .anchor(|a| {
                a.href(format!("/{ns}/{name}"))
                    .class(
                        "flex flex-col h-full border border-border rounded-lg p-3.5 hover:border-accent/40 hover:bg-surface card-lift",
                    )
                    .span(|s| {
                        s.class("flex items-start justify-between gap-2")
                            .span(|name_span| {
                                name_span
                                    .class("font-semibold truncate")
                                    .span(|ns_span| {
                                        ns_span
                                            .class("text-fg-muted")
                                            .text(format!("{ns}:"))
                                    })
                                    .span(|pkg_span| {
                                        pkg_span.class("text-accent").text(name.clone())
                                    })
                            })
                            .span(|badge| {
                                badge
                                    .class("text-xs text-fg-faint shrink-0")
                                    .text(icon.to_owned())
                            })
                    })
                    .span(|s| {
                        s.class("block text-sm text-fg-muted mt-1 line-clamp-2 flex-1")
                            .text(description.to_owned())
                    })
                    .span(|s| {
                        s.class("block text-xs text-fg-faint mt-3 font-mono")
                            .text(version.to_owned())
                    })
            })
            .build(),
        _ => Division::builder()
            .class("card-enter")
            .style(delay)
            .class("flex flex-col h-full border border-border rounded-lg p-3.5 card-lift")
            .span(|s| {
                s.class("flex items-start justify-between gap-2")
                    .span(|name_span| {
                        name_span
                            .class("font-semibold text-fg truncate")
                            .text(display_name)
                    })
                    .span(|badge| {
                        badge
                            .class("text-xs text-fg-faint shrink-0")
                            .text(icon.to_owned())
                    })
            })
            .span(|s| {
                s.class("block text-sm text-fg-muted mt-1 line-clamp-2 flex-1")
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
