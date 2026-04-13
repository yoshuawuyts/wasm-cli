//! About page (placeholder).

use html::text_content::Division;

use crate::layout;

/// Render a simple about page.
#[must_use]
pub(crate) fn render() -> String {
    let body = Division::builder()
        .class("pt-8 max-w-[65ch]")
        .heading_1(|h1| h1.class("text-3xl font-light tracking-display mb-6").text("About"))
        .paragraph(|p| {
            p.class("text-fg-secondary leading-relaxed")
                .text("The WebAssembly Package Registry is a discovery service for WebAssembly components and interfaces. It indexes packages from OCI registries and provides a browsable frontend for exploring the ecosystem.")
        })
        .paragraph(|p| {
            p.class("text-fg-secondary leading-relaxed mt-4")
                .text("This frontend is itself a WebAssembly component, compiled to ")
                .code(|c| {
                    c.class("bg-surface-muted px-1.5 py-0.5 text-sm")
                        .text("wasm32-wasip2")
                })
                .text(" and served via ")
                .code(|c| {
                    c.class("bg-surface-muted px-1.5 py-0.5 text-sm")
                        .text("wasi:http")
                })
                .text(".")
        })
        .build();

    layout::document("About", &body.to_string())
}
