//! 404 Not Found page.

// r[impl frontend.pages.not-found]

use html::text_content::Division;

use crate::layout;

/// Render a user-friendly 404 page.
#[must_use]
pub(crate) fn render() -> String {
    let body = Division::builder()
        .class("text-center py-20")
        .heading_1(|h1| h1.class("text-6xl font-bold text-accent").text("404"))
        .paragraph(|p| p.class("text-xl text-gray-600 mt-4").text("Page not found"))
        .paragraph(|p| {
            p.class("text-gray-500 mt-2")
                .text("The page you're looking for doesn't exist or has been moved.")
        })
        .anchor(|a| {
            a.href("/")
                .class("inline-block mt-8 px-6 py-3 bg-accent text-white rounded-lg font-medium hover:opacity-90 transition-opacity")
                .text("Go to Home")
        })
        .build();

    layout::document("Not Found", &body.to_string())
}
