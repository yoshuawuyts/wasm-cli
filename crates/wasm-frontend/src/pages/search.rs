//! Search results page.

// r[impl frontend.pages.search]

use html::inline_text::Span;
use html::text_content::Division;
use wasm_meta_registry_client::KnownPackage;

use crate::layout;
use wasm_meta_registry_client::{ApiError, RegistryClient};

/// Fetch matching packages and render the search results page.
pub(crate) async fn render(client: &RegistryClient, query: &str) -> String {
    match client.search_packages(query).await {
        Ok(packages) => render_results(query, &packages),
        Err(err) => render_error(query, &err),
    }
}

/// Render the search results.
fn render_results(query: &str, packages: &[KnownPackage]) -> String {
    let mut body = Division::builder();

    // Page header
    body.division(|div| {
        div.class("pb-6 border-b border-border mb-6")
            .heading_1(|h1| {
                h1.class("text-3xl font-bold tracking-tight")
                    .text(format!("Results for \u{201c}{query}\u{201d}"))
            })
            .paragraph(|p| {
                p.class("text-sm text-fg-faint mt-2").text(format!(
                    "{} package{} found",
                    packages.len(),
                    if packages.len() == 1 { "" } else { "s" }
                ))
            })
    });

    // Search box so users can refine
    body.push(render_search_form(query));

    if packages.is_empty() {
        body.division(|div| {
            div.class("py-16 text-center")
                .paragraph(|p| {
                    p.class("text-fg-muted")
                        .text("No packages matched your query.")
                })
                .paragraph(|p| {
                    p.class("mt-4").anchor(|a| {
                        a.href("/all")
                            .class("text-sm text-accent hover:underline")
                            .text("Browse all packages →")
                    })
                })
        });
    } else {
        // Table-style header
        body.division(|div| {
            div.class("hidden sm:flex items-baseline gap-3 px-2 pb-2 text-xs text-fg-faint uppercase tracking-wide")
                .span(|s| s.class("w-48 shrink-0").text("Package"))
                .span(|s| s.class("w-20 shrink-0").text("Version"))
                .span(|s| s.text("Description"))
        });

        let mut list = Division::builder();
        list.class("divide-y divide-border-light");
        for pkg in packages {
            list.push(render_row(pkg));
        }
        body.push(list.build());
    }

    layout::document("Search", &body.build().to_string())
}

/// Render the page with an API error message.
fn render_error(query: &str, err: &ApiError) -> String {
    let mut body = Division::builder();

    body.division(|div| {
        div.class("pb-6 border-b border-border mb-6")
            .heading_1(|h1| {
                h1.class("text-3xl font-bold tracking-tight")
                    .text(format!("Results for \u{201c}{query}\u{201d}"))
            })
    });

    body.push(render_search_form(query));

    body.division(|div| {
        div.class("py-16 text-center")
            .paragraph(|p| {
                p.class("text-fg font-semibold")
                    .text("Unable to search packages")
            })
            .paragraph(|p| p.class("text-sm text-fg-muted mt-2").text(err.to_string()))
    });

    layout::document("Search", &body.build().to_string())
}

/// Inline search form for refining queries.
fn render_search_form(query: &str) -> Division {
    Division::builder()
        .class("mb-8")
        .form(|form| {
            form.class("flex gap-2")
                .method("get")
                .action("/search")
                .input(|input| {
                    input
                        .type_("search")
                        .name("q")
                        .value(query.to_owned())
                        .placeholder("Search packages\u{2026}")
                        .class("flex-1 px-3 py-2 rounded border border-border bg-page text-fg text-sm placeholder:text-fg-faint focus:border-accent focus:ring-1 focus:ring-accent outline-none")
                })
                .button(|btn| {
                    btn.type_("submit")
                        .class("px-4 py-2 rounded bg-accent text-white text-sm font-medium hover:bg-accent-hover transition-colors")
                        .text("Search")
                })
        })
        .build()
}

/// Render a single package row.
fn render_row(pkg: &KnownPackage) -> Division {
    let (display_name, href) = package_identity(pkg);
    let description = pkg.description.as_deref().unwrap_or("");
    let version = pkg.tags.first().map_or("—", String::as_str);
    let [name_span, version_span, description_span] = row_spans(
        &display_name,
        version,
        description,
        if href.is_some() {
            "text-accent"
        } else {
            "text-fg"
        },
    );

    if let Some(href) = href {
        let mut row = Division::builder();
        row.anchor(|a| {
            a.href(href)
                .class(
                    "flex items-baseline gap-3 py-3 hover:bg-surface -mx-2 px-2 rounded transition-colors",
                )
                .push(name_span)
                .push(version_span)
                .push(description_span)
        });
        row.build()
    } else {
        let mut row = Division::builder();
        row.class("flex items-baseline gap-3 py-3 -mx-2 px-2 rounded")
            .push(name_span)
            .push(version_span)
            .push(description_span);
        row.build()
    }
}

fn package_identity(pkg: &KnownPackage) -> (String, Option<String>) {
    match (&pkg.wit_namespace, &pkg.wit_name) {
        (Some(ns), Some(name)) => (format!("{ns}:{name}"), Some(format!("/{ns}/{name}"))),
        _ => (pkg.repository.clone(), None),
    }
}

fn row_spans(
    display_name: &str,
    version: &str,
    description: &str,
    name_color_class: &str,
) -> [Span; 3] {
    [
        Span::builder()
            .class(format!(
                "w-48 shrink-0 font-semibold {name_color_class} truncate"
            ))
            .text(display_name.to_owned())
            .build(),
        Span::builder()
            .class("w-20 shrink-0 text-sm text-fg-faint")
            .text(version.to_owned())
            .build(),
        Span::builder()
            .class("text-sm text-fg-muted truncate")
            .text(description.to_owned())
            .build(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn package_without_wit() -> KnownPackage {
        KnownPackage {
            registry: "ghcr.io".to_string(),
            repository: "example/no-wit".to_string(),
            kind: None,
            description: Some("demo".to_string()),
            tags: vec!["1.0.0".to_string()],
            signature_tags: vec![],
            attestation_tags: vec![],
            last_seen_at: "2026-01-01T00:00:00Z".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            wit_namespace: None,
            wit_name: None,
            dependencies: vec![],
        }
    }

    #[test]
    fn non_wit_rows_render_as_non_links() {
        let html = render_row(&package_without_wit()).to_string();
        assert!(!html.contains("href=\"#\""));
        assert!(!html.contains("<a "));
    }
}
