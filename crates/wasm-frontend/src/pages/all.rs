//! All packages listing page.

// r[impl frontend.pages.all]

use html::text_content::Division;
use wasm_meta_registry_client::KnownPackage;

use crate::api_client::{ApiClient, ApiError};
use crate::layout;

/// Fetch all packages and render a paginated list.
pub(crate) async fn render(client: &ApiClient, offset: u32, limit: u32) -> String {
    match client.fetch_all_packages(offset, limit).await {
        Ok(packages) => render_packages(&packages, offset, limit),
        Err(err) => render_error(&err, offset, limit),
    }
}

/// Render the package listing page.
fn render_packages(packages: &[KnownPackage], offset: u32, limit: u32) -> String {
    let mut body = Division::builder();

    // Page header with count
    body.division(|div| {
        div.class("flex items-baseline justify-between pb-6 border-b border-border mb-6")
            .heading_1(|h1| {
                h1.class("text-3xl font-bold tracking-tight")
                    .text("All Packages")
            })
            .span(|s| {
                s.class("text-sm text-fg-faint")
                    .text(format!("showing {} packages", packages.len()))
            })
    });

    if packages.is_empty() {
        body.division(|div| {
            div.class("py-16 text-center").paragraph(|p| {
                p.class("text-fg-muted")
                    .text("No packages found. The registry may still be syncing.")
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

        body.push(render_pagination(packages, offset, limit));
    }

    layout::document("All Packages", &body.build().to_string())
}

/// Render the page with an API error message.
fn render_error(err: &ApiError, offset: u32, limit: u32) -> String {
    let mut body = Division::builder();

    body.division(|div| {
        div.class("pb-6 border-b border-border mb-6")
            .heading_1(|h1| {
                h1.class("text-3xl font-bold tracking-tight")
                    .text("All Packages")
            })
    });

    body.division(|div| {
        div.class("py-16 text-center")
            .paragraph(|p| {
                p.class("text-fg font-semibold")
                    .text("Unable to load packages")
            })
            .paragraph(|p| p.class("text-sm text-fg-muted mt-2").text(err.to_string()))
    });

    body.push(render_pagination(&[], offset, limit));

    layout::document("All Packages", &body.build().to_string())
}

/// Render a single package row.
fn render_row(pkg: &KnownPackage) -> Division {
    let display_name = match (&pkg.wit_namespace, &pkg.wit_name) {
        (Some(ns), Some(name)) => format!("{ns}:{name}"),
        _ => pkg.repository.clone(),
    };

    let description = pkg.description.as_deref().unwrap_or("");

    let version = pkg.tags.first().map_or("—", String::as_str);

    match (&pkg.wit_namespace, &pkg.wit_name) {
        (Some(ns), Some(name)) => Division::builder()
            .anchor(|a| {
                a.href(format!("/{ns}/{name}"))
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
            })
            .build(),
        _ => Division::builder()
            .class("flex items-baseline gap-3 py-3 -mx-2 px-2 rounded")
            .span(|s| {
                s.class("w-48 shrink-0 font-semibold text-fg truncate")
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
            .build(),
    }
}

fn render_pagination(packages: &[KnownPackage], offset: u32, limit: u32) -> Division {
    let effective_limit = limit.max(1);
    let has_prev = offset > 0;
    let has_next = u32::try_from(packages.len()) == Ok(effective_limit);
    let prev_offset = offset.saturating_sub(effective_limit);
    let next_offset = offset.saturating_add(effective_limit);
    let count = u32::try_from(packages.len()).unwrap_or(0);
    let (start, end) = if count == 0 {
        (0, 0)
    } else {
        (offset.saturating_add(1), offset.saturating_add(count))
    };

    let mut controls = Division::builder();
    controls.class("flex items-center gap-2");
    if has_prev {
        controls.anchor(|a| {
            a.href(format!("/all?offset={prev_offset}&limit={effective_limit}"))
                .class(
                    "px-3 py-1.5 rounded border border-border text-sm hover:bg-surface transition-colors",
                )
                .text("Previous")
        });
    } else {
        controls.span(|s| {
            s.class("px-3 py-1.5 rounded border border-border-light text-sm text-fg-faint")
                .text("Previous")
        });
    }
    if has_next {
        controls.anchor(|a| {
            a.href(format!("/all?offset={next_offset}&limit={effective_limit}"))
                .class(
                    "px-3 py-1.5 rounded border border-border text-sm hover:bg-surface transition-colors",
                )
                .text("Next")
        });
    } else {
        controls.span(|s| {
            s.class("px-3 py-1.5 rounded border border-border-light text-sm text-fg-faint")
                .text("Next")
        });
    }

    let mut container = Division::builder();
    container.class("flex items-center justify-between gap-4 mt-8 pt-6 border-t border-border");
    container.span(|s| {
        s.class("text-sm text-fg-faint")
            .text(format!("Showing {start}–{end}"))
    });
    container.push(controls.build());
    container.build()
}
