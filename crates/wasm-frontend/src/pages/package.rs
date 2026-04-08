//! Package detail page.

// r[impl frontend.pages.package-detail]

use html::content::{Aside, Navigation, Section};
use html::inline_text::{Anchor, Span};
use html::text_content::{Division, ListItem, UnorderedList};
use wasm_meta_registry_client::KnownPackage;

use crate::layout;

/// Render the package detail page for a given package and version.
#[must_use]
pub(crate) fn render(pkg: &KnownPackage, version: &str) -> String {
    let display_name = match (&pkg.wit_namespace, &pkg.wit_name) {
        (Some(ns), Some(name)) => format!("{ns}:{name}"),
        _ => pkg.repository.clone(),
    };

    let description = pkg
        .description
        .as_deref()
        .unwrap_or("No description available");

    let mut body = Division::builder();

    // Breadcrumb
    body.push(render_breadcrumb(&display_name));

    // Title
    body.division(|div| {
        div.class("mb-8")
            .heading_1(|h1| {
                h1.class("text-3xl font-bold tracking-tight text-accent")
                    .text(display_name.clone())
            })
            .paragraph(|p| {
                p.class("text-lg text-fg-secondary mt-2")
                    .text(description.to_owned())
            })
    });

    // Grid layout: main content + sidebar
    let mut grid = Division::builder();
    grid.class("grid grid-cols-1 md:grid-cols-3 gap-12");

    // Main content column
    let mut main_col = Division::builder();
    main_col.class("md:col-span-2 space-y-8");
    if let Some(tags) = render_tags(pkg, version) {
        main_col.push(tags);
    }
    if let Some(deps) = render_dependencies(pkg) {
        main_col.push(deps);
    }
    grid.push(main_col.build());

    // Sidebar
    grid.push(render_sidebar(pkg));

    body.push(grid.build());

    layout::document(&display_name, &body.build().to_string())
}

/// Render the breadcrumb navigation.
fn render_breadcrumb(display_name: &str) -> Navigation {
    Navigation::builder()
        .class("text-sm text-fg-muted mb-4")
        .anchor(|a| {
            a.href("/")
                .class("hover:text-accent transition-colors")
                .text("Home")
        })
        .span(|s| s.class("mx-1").text("/"))
        .span(|s| s.text(display_name.to_owned()))
        .build()
}

/// Render the tags/versions section.
fn render_tags(pkg: &KnownPackage, current_version: &str) -> Option<Section> {
    if pkg.tags.is_empty() {
        return None;
    }

    let url_name = match (&pkg.wit_namespace, &pkg.wit_name) {
        (Some(ns), Some(name)) => format!("{ns}/{name}"),
        _ => pkg.repository.clone(),
    };

    let mut section = Section::builder();
    section.heading_2(|h2| h2.class("text-lg font-semibold mb-3").text("Versions"));

    let mut tag_div = Division::builder();
    tag_div.class("flex flex-wrap gap-2");
    for tag in &pkg.tags {
        let is_current = tag == current_version;
        let classes = if is_current {
            "px-3 py-1 rounded-full text-sm bg-accent text-white"
        } else {
            "px-3 py-1 rounded-full text-sm bg-surface-muted text-fg-secondary hover:bg-border-light transition-colors"
        };
        let href = format!("/{url_name}/{tag}");
        let anchor = Anchor::builder()
            .href(href)
            .class(classes)
            .text(tag.clone())
            .build();
        tag_div.push(anchor);
    }
    section.push(tag_div.build());

    Some(section.build())
}

/// Render the dependencies section.
fn render_dependencies(pkg: &KnownPackage) -> Option<Section> {
    if pkg.dependencies.is_empty() {
        return None;
    }

    let mut section = Section::builder();
    section.heading_2(|h2| h2.class("text-lg font-semibold mb-3").text("Dependencies"));

    let mut ul = UnorderedList::builder();
    ul.class("space-y-1");
    for dep in &pkg.dependencies {
        let mut li = ListItem::builder();
        li.class("text-sm");
        let dep_span = Span::builder()
            .class("text-accent")
            .text(dep.package.clone())
            .build();
        li.push(dep_span);
        if let Some(v) = &dep.version {
            li.push(Span::builder().class("text-fg-faint").text(" @ ").build());
            let version_span = Span::builder()
                .class("text-fg-faint")
                .text(v.clone())
                .build();
            li.push(version_span);
        }
        ul.push(li.build());
    }
    section.push(ul.build());

    Some(section.build())
}

/// Render the sidebar with metadata.
fn render_sidebar(pkg: &KnownPackage) -> Aside {
    let mut aside = Aside::builder();
    aside.class("space-y-4");

    let mut card = Division::builder();
    card.class("bg-surface border border-border rounded-lg p-5 space-y-4 text-sm");
    card.push(sidebar_row("Registry", &pkg.registry));
    card.push(sidebar_row("Repository", &pkg.repository));
    card.push(sidebar_row("Created", &pkg.created_at));
    card.push(sidebar_row("Last updated", &pkg.last_seen_at));
    aside.push(card.build());

    aside.build()
}

/// Render a single sidebar metadata row.
fn sidebar_row(label: &str, value: &str) -> Division {
    Division::builder()
        .division(|dt| {
            dt.class("text-fg-muted text-xs uppercase tracking-wide")
                .text(label.to_owned())
        })
        .division(|dd| dd.class("text-fg mt-0.5 break-all").text(value.to_owned()))
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_meta_registry_client::PackageDependencyRef;

    #[test]
    fn dependency_versions_include_separator() {
        let pkg = KnownPackage {
            registry: "ghcr.io".to_string(),
            repository: "example/pkg".to_string(),
            kind: None,
            description: None,
            tags: vec!["1.0.0".to_string()],
            signature_tags: vec![],
            attestation_tags: vec![],
            last_seen_at: "2026-01-01T00:00:00Z".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            wit_namespace: Some("wasi".to_string()),
            wit_name: Some("demo".to_string()),
            dependencies: vec![PackageDependencyRef {
                package: "wasi:io".to_string(),
                version: Some("0.2.0".to_string()),
            }],
        };

        let html = render_dependencies(&pkg)
            .expect("dependencies section should render")
            .to_string();
        assert!(html.contains("wasi:io"));
        assert!(html.contains(" @ "));
        assert!(html.contains("0.2.0"));
    }
}
