//! All packages listing page.

// r[impl frontend.pages.all]

use html::text_content::Division;
use wasm_meta_registry_client::KnownPackage;

use crate::layout;
use wasm_meta_registry_client::{ApiError, RegistryClient};

/// Fetch all packages and render a paginated list.
pub(crate) async fn render(client: &RegistryClient, offset: u32, limit: u32) -> String {
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
        div.class("pt-8 flex items-baseline justify-between pb-6 border-b-2 border-fg mb-6")
            .heading_1(|h1| {
                h1.class("text-3xl font-light tracking-display")
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
            div.class("hidden sm:flex items-baseline gap-3 px-2 pb-2 text-sm text-fg-faint")
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

    layout::document_with_nav("All Packages", &body.build().to_string())
}

/// Render the page with an API error message.
fn render_error(err: &ApiError, offset: u32, limit: u32) -> String {
    let mut body = Division::builder();

    body.division(|div| {
        div.class("pt-8 pb-6 border-b-2 border-fg mb-6")
            .heading_1(|h1| {
                h1.class("text-3xl font-light tracking-display")
                    .text("All Packages")
            })
    });

    body.division(|div| {
        div.class("py-16 text-center")
            .paragraph(|p| {
                p.class("text-fg font-medium")
                    .text("Unable to load packages")
            })
            .paragraph(|p| p.class("text-sm text-fg-muted mt-2").text(err.to_string()))
    });

    body.push(render_pagination(&[], offset, limit));

    layout::document_with_nav("All Packages", &body.build().to_string())
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
                        "flex items-baseline gap-3 py-3 hover:bg-surface -mx-2 px-2 transition-colors",
                    )
                    .span(|s| {
                        s.class("w-48 shrink-0 font-medium text-accent truncate")
                            .text(display_name)
                    })
                    .span(|s| {
                        s.class("w-20 shrink-0 text-sm text-fg-faint")
                            .text(version.to_owned())
                    })
                    .span(|s| {
                        s.class("text-sm text-fg-muted truncate")
                            .text(crate::markdown::render_inline(description))
                    })
            })
            .build(),
        _ => Division::builder()
            .class("flex items-baseline gap-3 py-3 -mx-2 px-2 ")
            .span(|s| {
                s.class("w-48 shrink-0 font-medium text-fg truncate")
                    .text(display_name)
            })
            .span(|s| {
                s.class("w-20 shrink-0 text-sm text-fg-faint")
                    .text(version.to_owned())
            })
            .span(|s| {
                s.class("text-sm text-fg-muted truncate")
                    .text(crate::markdown::render_inline(description))
            })
            .build(),
    }
}

fn render_pagination(packages: &[KnownPackage], offset: u32, limit: u32) -> Division {
    let state = PaginationState::new(packages.len(), offset, limit);
    let mut container = Division::builder();
    container.class("flex items-center justify-between gap-4 mt-8 pt-6 border-t-2 border-fg");
    container.span(|s| {
        s.class("text-sm text-fg-faint")
            .text(format!("Showing {}–{}", state.start, state.end))
    });
    container.push(render_pagination_controls(&state));
    container.build()
}

#[derive(Debug)]
struct PaginationState {
    effective_limit: u32,
    prev_offset: u32,
    next_offset: u32,
    has_prev: bool,
    has_next: bool,
    start: u32,
    end: u32,
}

impl PaginationState {
    #[must_use]
    fn new(package_count: usize, offset: u32, limit: u32) -> Self {
        let effective_limit = limit.max(1);
        let has_prev = offset > 0;
        let has_next = u32::try_from(package_count) == Ok(effective_limit);
        let prev_offset = offset.saturating_sub(effective_limit);
        let next_offset = offset.saturating_add(effective_limit);
        let count = u32::try_from(package_count).unwrap_or(0);
        let (start, end) = if count == 0 {
            (0, 0)
        } else {
            (offset.saturating_add(1), offset.saturating_add(count))
        };

        Self {
            effective_limit,
            prev_offset,
            next_offset,
            has_prev,
            has_next,
            start,
            end,
        }
    }
}

fn render_pagination_controls(state: &PaginationState) -> Division {
    let mut controls = Division::builder();
    controls.class("flex items-center gap-2");
    if state.has_prev {
        controls.anchor(|a| {
            a.href(format!(
                "/all?offset={}&limit={}",
                state.prev_offset, state.effective_limit
            ))
            .class("px-3 py-1.5 border-2 border-fg text-sm hover:bg-surface transition-colors")
            .text("Previous")
        });
    } else {
        controls.span(|s| {
            s.class("px-3 py-1.5 border-2 border-fg-light text-sm text-fg-faint")
                .text("Previous")
        });
    }
    if state.has_next {
        controls.anchor(|a| {
            a.href(format!(
                "/all?offset={}&limit={}",
                state.next_offset, state.effective_limit
            ))
            .class("px-3 py-1.5 border-2 border-fg text-sm hover:bg-surface transition-colors")
            .text("Next")
        });
    } else {
        controls.span(|s| {
            s.class("px-3 py-1.5 border-2 border-fg-light text-sm text-fg-faint")
                .text("Next")
        });
    }
    controls.build()
}

#[cfg(test)]
mod tests {
    use super::*;

    // r[verify frontend.pages.all]
    #[test]
    fn pagination_state_calculates_prev_and_next_offsets() {
        const PAGE_SIZE: u32 = 100;
        const SECOND_PAGE_OFFSET: u32 = 100;
        let state = PaginationState::new(100, SECOND_PAGE_OFFSET, PAGE_SIZE);
        assert_eq!(state.prev_offset, 0);
        assert_eq!(state.next_offset, 200);
        assert!(state.has_prev);
        assert!(state.has_next);
        assert_eq!(state.start, 101);
        assert_eq!(state.end, 200);
    }
}
