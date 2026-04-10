//! Error page shown when the registry API is unreachable.

use html::text_content::Division;

use crate::layout;

/// Render an error page with a description of what went wrong.
#[must_use]
pub(crate) fn render(message: &str) -> String {
    let body = Division::builder()
        .class("text-center py-20")
        .heading_1(|h1| {
            h1.class("text-3xl font-normal tracking-display text-fg")
                .text("Something went wrong")
        })
        .paragraph(|p| {
            p.class("text-sm text-fg-muted mt-4")
                .text(message.to_owned())
        })
        .anchor(|a| {
            a.href("/")
                .class("inline-block mt-8 px-6 py-3 bg-fg text-page rounded-lg font-medium hover:bg-fg-secondary transition-colors")
                .text("Go to Home")
        })
        .build();

    layout::document("Error", &body.to_string())
}
