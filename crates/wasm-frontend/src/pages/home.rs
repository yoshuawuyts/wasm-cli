//! Front page — recently updated components and interfaces.

// r[impl frontend.pages.home]

use html::content::Section;
use html::inline_text::Anchor;
use html::text_content::Division;
use wasm_meta_registry_client::KnownPackage;

use crate::api_client::ApiClient;
use crate::layout;

/// Fetch recent packages and render the home page.
pub(crate) async fn render(client: &ApiClient) -> String {
    let packages = client.fetch_recent_packages(50).await;

    let (components, interfaces) = split_by_kind(&packages);

    let mut body = Division::builder();

    body.heading_1(|h1| {
        h1.class("text-3xl font-bold mb-8")
            .text("WebAssembly Package Registry")
    });

    if let Some(section) = render_section("Recently Updated Interfaces", &interfaces) {
        body.push(section);
    }
    if let Some(section) = render_section("Recently Updated Components", &components) {
        body.push(section);
    }

    if packages.is_empty() {
        body.paragraph(|p| {
            p.class("text-gray-500 mt-8")
                .text("No packages found. The registry may still be syncing.")
        });
    }

    layout::document("Home", &body.build().to_string())
}

/// Split packages into (components, interfaces) based on WIT metadata.
///
/// Packages with a `wit_name` are considered interfaces unless their
/// repository path suggests they are a component (heuristic: no `/`
/// separator in the WIT name is ambiguous, so we default to interface).
fn split_by_kind(packages: &[KnownPackage]) -> (Vec<&KnownPackage>, Vec<&KnownPackage>) {
    let mut components = Vec::new();
    let mut interfaces = Vec::new();

    for pkg in packages {
        // Packages without WIT metadata go into components as a fallback
        if pkg.wit_namespace.is_none() {
            components.push(pkg);
        } else {
            interfaces.push(pkg);
        }
    }

    (components, interfaces)
}

/// Render a section with a heading and a grid of package cards.
fn render_section(heading: &str, packages: &[&KnownPackage]) -> Option<Section> {
    if packages.is_empty() {
        return None;
    }

    let mut section = Section::builder();
    section.class("mb-10");
    section.heading_2(|h2| {
        h2.class("text-xl font-semibold mb-4")
            .text(heading.to_owned())
    });

    let mut grid = Division::builder();
    grid.class("grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4");
    for pkg in packages {
        grid.push(render_card(pkg));
    }
    section.push(grid.build());

    Some(section.build())
}

/// Render a single package card.
fn render_card(pkg: &KnownPackage) -> Anchor {
    let display_name = match (&pkg.wit_namespace, &pkg.wit_name) {
        (Some(ns), Some(name)) => format!("{ns}:{name}"),
        _ => pkg.repository.clone(),
    };

    let href = match (&pkg.wit_namespace, &pkg.wit_name) {
        (Some(ns), Some(name)) => format!("/{ns}/{name}"),
        _ => "#".to_string(),
    };

    let description = pkg
        .description
        .as_deref()
        .unwrap_or("No description available");

    let version = pkg.tags.first().map_or("—", String::as_str);

    Anchor::builder()
        .href(href)
        .class("block border border-gray-200 rounded-lg p-4 hover:border-accent hover:shadow-sm transition-colors")
        .span(|s| s.class("block font-mono font-semibold text-accent").text(display_name))
        .span(|s| s.class("block text-sm text-gray-500 mt-1").text(version.to_owned()))
        .span(|s| {
            s.class("block text-sm text-gray-600 mt-2 line-clamp-2")
                .text(description.to_owned())
        })
        .build()
}
