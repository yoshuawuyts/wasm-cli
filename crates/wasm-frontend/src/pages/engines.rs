//! Host runtime support page.

use html::text_content::Division;
use wasm_meta_registry_client::{ApiError, HostEngine, RegistryClient};

use crate::layout;

/// Fetch engine support metadata and render the engines page.
pub(crate) async fn render(client: &RegistryClient) -> String {
    match client.fetch_engines().await {
        Ok(engines) => render_engines(&engines),
        Err(err) => render_error(&err),
    }
}

fn render_engines(engines: &[HostEngine]) -> String {
    let mut body = Division::builder();
    body.division(|div| {
        div.class("pt-8 pb-6 border-b border-border mb-6")
            .heading_1(|h1| {
                h1.class("text-3xl font-bold tracking-tight")
                    .text("Host Engines")
            })
            .paragraph(|p| {
                p.class("text-fg-secondary mt-2")
                    .text("Runtime support for WIT interfaces across host engines.")
            })
    });

    if engines.is_empty() {
        body.paragraph(|p| {
            p.class("text-fg-muted py-8")
                .text("No engine support data is configured yet.")
        });
    } else {
        let mut list = Division::builder();
        list.class("space-y-4");
        for engine in engines {
            list.push(render_engine(engine));
        }
        body.push(list.build());
    }

    layout::document("Engines", &body.build().to_string())
}

fn render_engine(engine: &HostEngine) -> Division {
    let mut card = Division::builder();
    card.class("rounded-lg border border-border p-4 bg-surface");
    card.division(|div| {
        div.class("flex flex-wrap items-baseline gap-x-3 gap-y-1");
        div.heading_2(|h2| h2.class("text-lg font-semibold").text(engine.name.clone()));
        if let Some(homepage) = &engine.homepage {
            div.anchor(|a| {
                a.href(homepage.clone())
                    .class("text-sm text-accent hover:underline")
                    .text("website")
            });
        }
        div
    });

    if engine.interfaces.is_empty() {
        card.paragraph(|p| {
            p.class("text-sm text-fg-muted mt-2")
                .text("No interface support listed.")
        });
    } else {
        let mut interfaces = Division::builder();
        interfaces.class("mt-3 space-y-2");
        for support in &engine.interfaces {
            let versions = if support.versions.is_empty() {
                "unversioned".to_string()
            } else {
                support.versions.join(", ")
            };
            interfaces.paragraph(|p| {
                p.class("text-sm text-fg-secondary")
                    .code(|c| c.class("font-mono text-fg").text(support.interface.clone()))
                    .text(": ")
                    .span(|s| s.class("text-fg-muted").text(versions))
            });
        }
        card.push(interfaces.build());
    }

    card.build()
}

fn render_error(err: &ApiError) -> String {
    let body = Division::builder()
        .class("pt-8 max-w-[65ch]")
        .heading_1(|h1| {
            h1.class("text-3xl font-bold tracking-tight mb-6")
                .text("Host Engines")
        })
        .paragraph(|p| {
            p.class("text-fg font-semibold")
                .text("Unable to load host runtime support data")
        })
        .paragraph(|p| p.class("text-sm text-fg-muted mt-2").text(err.to_string()))
        .build();

    layout::document("Engines", &body.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_meta_registry_client::HostInterfaceSupport;

    #[test]
    fn render_engines_lists_interfaces_and_versions() {
        let html = render_engines(&[HostEngine {
            name: "wasmtime".to_string(),
            homepage: Some("https://wasmtime.dev".to_string()),
            interfaces: vec![HostInterfaceSupport {
                interface: "wasi:http".to_string(),
                versions: vec!["0.2.0".to_string()],
            }],
        }]);

        assert!(html.contains("Host Engines"));
        assert!(html.contains("wasmtime"));
        assert!(html.contains("wasi:http"));
        assert!(html.contains("0.2.0"));
    }
}
