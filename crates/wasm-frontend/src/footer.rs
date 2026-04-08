//! Footer component.

use html::content::Footer;

/// Render the site footer.
#[must_use]
pub(crate) fn render() -> String {
    Footer::builder()
        .class("border-t border-gray-200 mt-12")
        .division(|div| {
            div.class("max-w-5xl mx-auto px-4 py-6 text-center text-sm text-gray-500")
                .paragraph(|p| {
                    p.text("wasm registry — a ")
                        .anchor(|a| {
                            a.href("https://bytecodealliance.org")
                                .class("text-accent hover:underline")
                                .text("Bytecode Alliance")
                        })
                        .text(" project")
                })
        })
        .build()
        .to_string()
}
