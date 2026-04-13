//! 404 Not Found page.

// r[impl frontend.pages.not-found]

use html::text_content::Division;

use crate::layout;

/// Render a user-friendly 404 page.
#[must_use]
pub(crate) fn render() -> String {
    let body = Division::builder()
        .class("pt-16 pb-20 max-w-lg")
        .heading_1(|h1| {
            h1.class("text-4xl font-light tracking-display text-accent")
                .text("Page not found")
        })
        .paragraph(|p| {
            p.class("text-fg-secondary mt-3").text(
                "The package, interface, or item you're looking for \
                     doesn't exist — or it may have been published under \
                     a different version.",
            )
        })
        .division(|actions| {
            actions
                .class("mt-8 flex flex-wrap gap-3 text-sm")
                .anchor(|a| {
                    a.href("/")
                        .class(
                            "px-4 py-2 bg-accent text-white \
                             font-medium hover:bg-accent-hover transition-colors",
                        )
                        .text("Browse packages")
                })
                .anchor(|a| {
                    a.href("/search")
                        .class(
                            "px-4 py-2 border-2 border-fg \
                             text-fg hover:border-accent/50 transition-colors",
                        )
                        .text("Search")
                })
        })
        .build();

    layout::document_with_nav("Not Found", &body.to_string())
}
