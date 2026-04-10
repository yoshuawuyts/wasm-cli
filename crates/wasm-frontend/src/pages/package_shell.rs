//! Shared page shell for the package detail page and its sub-pages
//! (interface, world, item).
//!
//! Provides the package header (title, version selector, description,
//! install command, metadata) and tab bar (Documentation / Dependencies /
//! Dependents) that appear on every package-scoped page.

use html::text_content::Division;
use wasm_meta_registry_client::{KnownPackage, PackageVersion};

use crate::layout;

/// Which tab is currently active on the package detail page.
pub(crate) enum ActiveTab<'a> {
    /// WIT definition and worlds.
    Docs {
        /// Version detail for metadata display (may be `None` on sub-pages
        /// that don't need the full version payload).
        version_detail: Option<&'a PackageVersion>,
    },
    /// Forward dependencies of this package.
    Dependencies,
    /// Reverse dependencies: packages that import or export this interface.
    Dependents {
        importers: &'a [KnownPackage],
        exporters: &'a [KnownPackage],
    },
}

/// Render the shared page shell: header + tab bar + caller-provided body,
/// wrapped in the HTML document layout.
#[must_use]
pub(crate) fn render_page(
    pkg: &KnownPackage,
    version: &str,
    tab: &ActiveTab<'_>,
    title: &str,
    body_content: Division,
) -> String {
    let display_name = display_name_for(pkg);
    let description = pkg
        .description
        .as_deref()
        .unwrap_or("No description available");

    let version_detail = match tab {
        ActiveTab::Docs { version_detail } => *version_detail,
        _ => None,
    };

    let mut body = Division::builder();
    body.class("pt-8");

    // Header: title + description on left, metadata on right
    body.push(render_page_header(
        pkg,
        &display_name,
        description,
        version,
        version_detail,
    ));

    // Tab bar
    let url_base = url_base_for(pkg, version);
    body.push(render_tab_bar(&url_base, tab));

    // Caller-provided body content
    body.push(body_content);

    layout::document(title, &body.build().to_string())
}

/// Compute the display name from package WIT metadata.
pub(crate) fn display_name_for(pkg: &KnownPackage) -> String {
    match (&pkg.wit_namespace, &pkg.wit_name) {
        (Some(ns), Some(name)) => format!("{ns}:{name}"),
        _ => pkg.repository.clone(),
    }
}

/// Compute the URL base for tab links.
pub(crate) fn url_base_for(pkg: &KnownPackage, version: &str) -> String {
    match (&pkg.wit_namespace, &pkg.wit_name) {
        (Some(ns), Some(name)) => format!("/{ns}/{name}/{version}"),
        _ => format!("/{}/{version}", pkg.repository),
    }
}

/// Render the tab bar with links to each tab route.
fn render_tab_bar(url_base: &str, active: &ActiveTab<'_>) -> Division {
    let active_class = "text-accent border-b-2 border-accent font-semibold";
    let inactive_class = "text-fg-muted hover:text-fg";
    let tab_base = "px-4 py-2 text-sm transition-colors inline-block";

    let tabs: &[(&str, &str, bool)] = &[
        (
            "Documentation",
            url_base,
            matches!(active, ActiveTab::Docs { .. }),
        ),
        (
            "Dependencies",
            &format!("{url_base}/dependencies"),
            matches!(active, ActiveTab::Dependencies),
        ),
        (
            "Dependents",
            &format!("{url_base}/dependents"),
            matches!(active, ActiveTab::Dependents { .. }),
        ),
    ];

    Division::builder()
        .class("flex border-b border-border mb-8")
        .push({
            let mut nav = Division::builder();
            nav.class("flex");
            for &(label, href, is_active) in tabs {
                let style = if is_active {
                    active_class
                } else {
                    inactive_class
                };
                nav.anchor(|a| {
                    a.href(href.to_owned())
                        .class(format!("{tab_base} {style}"))
                        .text(label.to_owned())
                });
            }
            nav.build()
        })
        .build()
}

/// Render the page header: title + description on left, metadata on right.
fn render_page_header(
    pkg: &KnownPackage,
    display_name: &str,
    description: &str,
    current_version: &str,
    version_detail: Option<&PackageVersion>,
) -> Division {
    let url_name = match (&pkg.wit_namespace, &pkg.wit_name) {
        (Some(ns), Some(name)) => format!("{ns}/{name}"),
        _ => pkg.repository.clone(),
    };

    let annotations = version_detail.and_then(|d| d.annotations.as_ref());

    let mut header = Division::builder();
    header.class("flex flex-col md:flex-row md:items-start md:justify-between gap-6 mb-6");

    // Left: title with inline version + description + install command
    header.division(|left| {
        left.class("flex-1 min-w-0")
            .division(|title_row| {
                title_row
                    .class("flex items-baseline gap-2 flex-wrap")
                    .heading_1(|h1| {
                        h1.class("text-3xl font-bold tracking-tight text-accent")
                            .text(display_name.to_owned())
                    })
                    .push(render_version_inline(pkg, current_version, &url_name))
            })
            .paragraph(|p| {
                p.class("text-fg-secondary mt-1")
                    .text(description.to_owned())
            })
            .division(|d| {
                d.class("mt-4")
                    .push(render_install_command(display_name, current_version))
            })
    });

    // Right: metadata rows
    header.division(|right| {
        right.class("shrink-0 md:w-72 space-y-3 text-sm");

        let mut meta = Division::builder();
        meta.class("space-y-2 text-xs leading-relaxed");

        if let Some(source) = annotations.and_then(|a| a.source.as_deref()) {
            meta.push(meta_link_row("Repository", &abbreviate_url(source), source));
        } else {
            let repo_url = format!("https://{}/{}", pkg.registry, pkg.repository);
            let repo_display = format!("{}/{}", pkg.registry, pkg.repository);
            meta.push(meta_link_row("Repository", &repo_display, &repo_url));
        }

        if let Some(license) = annotations.and_then(|a| a.licenses.as_deref()) {
            meta.push(meta_row("License", license));
        }
        if let Some(kind) = &pkg.kind {
            meta.push(meta_row("Kind", &kind.to_string()));
        }
        if let Some(size) = version_detail.and_then(|d| d.size_bytes) {
            meta.push(meta_row("Size", &format_size(size)));
        }
        if let Some(created) = version_detail.and_then(|d| d.created_at.as_deref()) {
            meta.push(meta_row("Published", &format_date(created)));
        }
        if let Some(docs_url) = annotations.and_then(|a| a.documentation.as_deref()) {
            meta.push(meta_link_row("Docs", &abbreviate_url(docs_url), docs_url));
        }
        let source = annotations.and_then(|a| a.source.as_deref());
        if let Some(url) = annotations.and_then(|a| a.url.as_deref())
            && source != Some(url)
        {
            meta.push(meta_link_row("Homepage", &abbreviate_url(url), url));
        }

        right.push(meta.build());
        right
    });

    header.build()
}

/// Render the install command section with a copy button.
fn render_install_command(display_name: &str, version: &str) -> Division {
    let command = format!("wasm install {display_name}@{version}");

    let copy_icon = "<svg xmlns='http://www.w3.org/2000/svg' width='16' height='16' viewBox='0 0 24 24' fill='none' stroke='currentColor' stroke-width='2' stroke-linecap='round' stroke-linejoin='round'><rect x='9' y='9' width='13' height='13' rx='2' ry='2'/><path d='M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1'/></svg>";
    let check_icon = "<svg xmlns='http://www.w3.org/2000/svg' width='16' height='16' viewBox='0 0 24 24' fill='none' stroke='currentColor' stroke-width='2' stroke-linecap='round' stroke-linejoin='round'><polyline points='20 6 9 17 4 12'/></svg>";

    let script = format!(
        "(function(){{\
        var btn=document.getElementById('copy-install-btn');\
        var copyIcon=\"{copy_icon}\";\
        var checkIcon=\"{check_icon}\";\
        btn.innerHTML=copyIcon;\
        btn.addEventListener('click',function(){{\
        navigator.clipboard.writeText('{command}').then(function(){{\
        btn.innerHTML=checkIcon;\
        setTimeout(function(){{btn.innerHTML=copyIcon}},2000)\
        }})}})}})()",
    );

    Division::builder()
        .class("max-w-lg group/install")
        .division(|div| {
            div.class(
                "flex items-center gap-2 bg-surface-muted border border-border \
                 rounded-md px-3 py-2 font-mono text-xs text-fg",
            )
            .code(|code| {
                code.class("flex-1 select-all overflow-hidden whitespace-nowrap text-ellipsis")
                    .text(command)
            })
            .button(|btn| {
                btn.id("copy-install-btn").class(
                    "shrink-0 text-fg-muted opacity-0 group-hover/install:opacity-100 \
                     hover:text-fg transition-opacity cursor-pointer",
                )
            })
            .script(|s| s.text(script))
        })
        .build()
}

/// Render the inline version selector: `@ <select>` next to the package title.
fn render_version_inline(pkg: &KnownPackage, current_version: &str, url_name: &str) -> Division {
    let mut select = html::forms::Select::builder();
    select
        .id("version-select")
        .name("version")
        .class("px-1.5 py-0.5 rounded border border-border bg-surface text-fg text-xl font-bold cursor-pointer");

    for tag in &pkg.tags {
        let is_current = tag == current_version;
        if is_current {
            select.option(|opt| opt.value(tag.clone()).text(tag.clone()).selected(true));
        } else {
            select.option(|opt| opt.value(tag.clone()).text(tag.clone()));
        }
    }

    let script_body = format!(
        "document.getElementById('version-select').addEventListener('change',function(){{window.location.href='/{url_name}/'+this.value}})"
    );

    Division::builder()
        .class("flex items-baseline gap-1")
        .span(|s| s.class("text-xl text-fg-muted font-bold").text("@"))
        .push(select.build())
        .script(|s| s.text(script_body))
        .build()
}

/// Render a label: value metadata row.
fn meta_row(label: &str, value: &str) -> Division {
    Division::builder()
        .class("flex gap-2")
        .span(|s| {
            s.class("text-fg-muted w-20 shrink-0")
                .text(label.to_owned())
        })
        .span(|s| s.class("text-fg truncate").text(value.to_owned()))
        .build()
}

/// Render a label: linked-value metadata row.
fn meta_link_row(label: &str, text: &str, href: &str) -> Division {
    Division::builder()
        .class("flex gap-2")
        .span(|s| {
            s.class("text-fg-muted w-20 shrink-0")
                .text(label.to_owned())
        })
        .anchor(|a| {
            a.href(href.to_owned())
                .class("text-accent hover:underline truncate")
                .text(text.to_owned())
        })
        .build()
}

/// Format a byte count as a human-readable size string.
#[allow(clippy::cast_precision_loss)]
fn format_size(bytes: i64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = KIB * 1024.0;
    const GIB: f64 = MIB * 1024.0;

    let bytes = bytes as f64;
    if bytes < KIB {
        format!("{bytes} B")
    } else if bytes < MIB {
        format!("{:.1} KiB", bytes / KIB)
    } else if bytes < GIB {
        format!("{:.1} MiB", bytes / MIB)
    } else {
        format!("{:.1} GiB", bytes / GIB)
    }
}

/// Abbreviate a URL for display (strip scheme and trailing slash).
fn abbreviate_url(url: &str) -> String {
    url.strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(url)
        .trim_end_matches('/')
        .to_owned()
}

/// Format an ISO 8601 timestamp as a short date (YYYY-MM-DD).
fn format_date(iso: &str) -> String {
    iso.split('T').next().unwrap_or(iso).to_owned()
}
