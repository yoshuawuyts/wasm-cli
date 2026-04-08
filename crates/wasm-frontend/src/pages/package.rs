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
                h1.class("text-3xl font-bold font-mono text-accent")
                    .text(display_name.clone())
            })
            .paragraph(|p| {
                p.class("text-lg text-gray-600 mt-2")
                    .text(description.to_owned())
            })
    });

    // Grid layout: main content + sidebar
    let mut grid = Division::builder();
    grid.class("grid grid-cols-1 md:grid-cols-3 gap-8");

    // Main content column
    let mut main_col = Division::builder();
    main_col.class("md:col-span-2 space-y-6");
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
        .class("text-sm text-gray-500 mb-4")
        .anchor(|a| a.href("/").class("hover:text-accent").text("Home"))
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
            "px-3 py-1 rounded-full text-sm font-mono bg-accent text-white"
        } else {
            "px-3 py-1 rounded-full text-sm font-mono bg-gray-100 text-gray-700 hover:bg-gray-200"
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
        li.class("font-mono text-sm");
        let dep_span = Span::builder()
            .class("text-accent")
            .text(dep.package.clone())
            .build();
        li.push(dep_span);
        if let Some(v) = &dep.version {
            let version_span = Span::builder()
                .class("text-gray-400")
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
    card.class("border border-gray-200 rounded-lg p-4 space-y-3 text-sm");
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
            dt.class("text-gray-500 text-xs uppercase tracking-wide")
                .text(label.to_owned())
        })
        .division(|dd| {
            dd.class("font-mono text-gray-900 mt-0.5 break-all")
                .text(value.to_owned())
        })
        .build()
}
