//! Namespace (publisher) page — lists all packages under a given namespace.

use html::text_content::Division;
use wasm_meta_registry_client::RegistryClient;

use crate::layout;

/// Render the namespace page listing all packages for a publisher.
pub(crate) async fn render(client: &RegistryClient, namespace: &str) -> String {
    match client.search_packages(namespace).await {
        Ok(packages) => {
            let filtered: Vec<_> = packages
                .iter()
                .filter(|p| p.wit_namespace.as_deref().is_some_and(|ns| ns == namespace))
                .collect();
            render_packages(namespace, &filtered)
        }
        Err(err) => {
            eprintln!("wasm-frontend: namespace page error for {namespace}: {err}");
            render_packages(namespace, &[])
        }
    }
}

/// Render the package listing for a namespace.
fn render_packages(
    namespace: &str,
    packages: &[&wasm_meta_registry_client::KnownPackage],
) -> String {
    let mut body = Division::builder();

    body.division(|div| {
        div.class("pt-8 pb-8")
            .heading_1(|h1| {
                h1.class("text-3xl font-normal tracking-display")
                    .text(namespace.to_owned())
            })
            .paragraph(|p| {
                p.class("text-sm text-fg-muted mt-2").text(format!(
                    "{} package{}",
                    packages.len(),
                    if packages.len() == 1 { "" } else { "s" }
                ))
            })
    });

    if packages.is_empty() {
        body.division(|div| {
            div.class("py-16 text-center").paragraph(|p| {
                p.class("text-fg-muted")
                    .text("No packages found under this namespace.")
            })
        });
    } else {
        let mut grid = Division::builder();
        grid.class(
            "grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 border-t-2 border-l-2 border-fg",
        );
        for pkg in packages {
            grid.push(render_card(pkg));
        }
        body.push(grid.build());
    }

    layout::document_with_nav(namespace, &body.build().to_string())
}

/// Render a single package card.
fn render_card(pkg: &wasm_meta_registry_client::KnownPackage) -> Division {
    let description = pkg.description.as_deref().unwrap_or("No description");
    let version = crate::pick_redirect_version(&pkg.tags).unwrap_or_else(|| {
        pkg.tags
            .first()
            .cloned()
            .unwrap_or_else(|| "\u{2014}".to_owned())
    });

    match (&pkg.wit_namespace, &pkg.wit_name) {
        (Some(ns), Some(name)) => Division::builder()
            .anchor(|a| {
                a.href(format!("/{ns}/{name}"))
                    .class("flex flex-col h-full bg-page p-5 border-r-2 border-b-2 border-fg card-lift")
                    .span(|s| {
                        s.class("flex justify-between items-start")
                            .span(|left| {
                                left.class("text-2xl font-light tracking-display leading-tight truncate")
                                    .text(name.clone())
                            })
                            .span(|right| {
                                right
                                    .class("text-sm text-fg-faint font-mono shrink-0")
                                    .text(version.clone())
                            })
                    })
                    .span(|s| {
                        s.class("block text-sm text-fg-muted mt-6 overflow-hidden")
                            .style("display: -webkit-box; -webkit-line-clamp: 2; -webkit-box-orient: vertical; min-height: 2.75rem")
                            .text(description.to_owned())
                    })
            })
            .build(),
        _ => Division::builder()
            .class("flex flex-col h-full bg-page p-5 border-r-2 border-b-2 border-fg card-lift")
            .span(|s| {
                s.class("flex justify-between items-start")
                    .span(|left| {
                        left.class("text-2xl font-light tracking-display leading-tight truncate")
                            .text(pkg.repository.clone())
                    })
                    .span(|right| {
                        right
                            .class("text-sm text-fg-faint font-mono shrink-0")
                            .text(version.clone())
                    })
            })
            .span(|s| {
                s.class("block text-sm text-fg-muted mt-6 overflow-hidden")
                    .style("display: -webkit-box; -webkit-line-clamp: 2; -webkit-box-orient: vertical; min-height: 2.75rem")
                    .text(description.to_owned())
            })
            .build(),
    }
}
