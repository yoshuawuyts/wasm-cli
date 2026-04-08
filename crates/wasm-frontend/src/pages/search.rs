//! Search results page.

// r[impl frontend.pages.search]

use html::inline_text::Anchor;
use html::text_content::Division;
use wasm_meta_registry_client::KnownPackage;

use crate::layout;
use wasm_meta_registry_client::{ApiClient, ApiError};

/// Fetch matching packages and render the search results page.
pub(crate) async fn render(client: &ApiClient, query: &str) -> String {
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
                        .class("flex-1 px-3 py-2 rounded border border-border bg-white text-fg text-sm placeholder:text-fg-faint focus:border-accent focus:ring-1 focus:ring-accent outline-none")
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
fn render_row(pkg: &KnownPackage) -> Anchor {
    let display_name = match (&pkg.wit_namespace, &pkg.wit_name) {
        (Some(ns), Some(name)) => format!("{ns}:{name}"),
        _ => pkg.repository.clone(),
    };

    let href = match (&pkg.wit_namespace, &pkg.wit_name) {
        (Some(ns), Some(name)) => format!("/{ns}/{name}"),
        _ => "#".to_string(),
    };

    let description = pkg.description.as_deref().unwrap_or("");
    let version = pkg.tags.first().map_or("—", String::as_str);

    Anchor::builder()
        .href(href)
        .class(
            "flex items-baseline gap-3 py-3 hover:bg-surface -mx-2 px-2 rounded transition-colors",
        )
        .span(|s| {
            s.class("w-48 shrink-0 font-semibold text-accent truncate")
                .text(display_name)
        })
        .span(|s| {
            s.class("w-20 shrink-0 text-sm text-fg-faint")
                .text(version.to_owned())
        })
        .span(|s| {
            s.class("text-sm text-fg-muted truncate")
                .text(description.to_owned())
        })
        .build()
}
