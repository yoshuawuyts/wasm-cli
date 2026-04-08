//! All packages listing page.

// r[impl frontend.pages.all]

use html::inline_text::Anchor;
use html::text_content::Division;
use wasm_meta_registry_client::KnownPackage;

use crate::api_client::ApiClient;
use crate::layout;

/// Fetch all packages and render a paginated list.
pub(crate) async fn render(client: &ApiClient) -> String {
    let packages = client.fetch_all_packages(0, 100).await;

    let mut body = Division::builder();

    body.heading_1(|h1| h1.class("text-3xl font-bold mb-8").text("All Packages"));

    if packages.is_empty() {
        body.paragraph(|p| {
            p.class("text-gray-500")
                .text("No packages found. The registry may still be syncing.")
        });
    } else {
        let mut list = Division::builder();
        list.class("space-y-2");
        for pkg in &packages {
            list.push(render_row(pkg));
        }
        body.push(list.build());
    }

    layout::document("All Packages", &body.build().to_string())
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
        .class("flex items-center justify-between border border-gray-200 rounded-lg px-4 py-3 hover:border-accent hover:shadow-sm transition-colors")
        .span(|outer| {
            outer
                .class("block")
                .span(|s| s.class("font-mono font-semibold text-accent").text(display_name))
                .span(|s| s.class("text-sm text-gray-500 ml-2").text(version.to_owned()))
                .span(|s| {
                    s.class("block text-sm text-gray-600 mt-0.5 line-clamp-1")
                        .text(description.to_owned())
                })
        })
        .build()
}
