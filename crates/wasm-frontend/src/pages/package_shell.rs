//! Shared page shell for the package detail page and its sub-pages
//! (interface, world, item).
//!
//! Provides a two-column layout: main content on the left, and a sidebar
//! on the right with version selector, install command, metadata,
//! dependencies, and dependents.

use html::text_content::Division;
use wasm_meta_registry_client::{KnownPackage, PackageVersion};

use crate::layout;

/// Context for rendering the package page sidebar.
pub(crate) struct SidebarContext<'a> {
    /// Package being displayed.
    pub pkg: &'a KnownPackage,
    /// Current version string.
    pub version: &'a str,
    /// Version detail (annotations, size, etc.) if available.
    pub version_detail: Option<&'a PackageVersion>,
    /// Packages that import this one.
    pub importers: &'a [KnownPackage],
    /// Packages that export this one.
    pub exporters: &'a [KnownPackage],
    /// Override the description shown at the top (uses pkg.description if None).
    pub description_override: Option<&'a str>,
}

/// Render the shared page shell: two-column layout with sidebar,
/// wrapped in the HTML document layout.
#[must_use]
pub(crate) fn render_page(ctx: &SidebarContext<'_>, title: &str, body_content: Division) -> String {
    render_page_inner(ctx, title, body_content, vec![])
}

/// Render the page shell with extra breadcrumb segments after the package name.
#[must_use]
pub(crate) fn render_page_with_crumbs(
    ctx: &SidebarContext<'_>,
    title: &str,
    body_content: Division,
    extra_crumbs: Vec<crate::nav::Crumb>,
) -> String {
    render_page_inner(ctx, title, body_content, extra_crumbs)
}

/// Inner page shell renderer.
fn render_page_inner(
    ctx: &SidebarContext<'_>,
    title: &str,
    body_content: Division,
    extra_crumbs: Vec<crate::nav::Crumb>,
) -> String {
    let pkg = ctx.pkg;
    let display_name = display_name_for(pkg);
    let description = match ctx.description_override {
        Some(d) => d,
        None => pkg.description.as_deref().unwrap_or(""),
    };

    let mut body = Division::builder();
    body.class("pt-6");

    // Two-column grid: content + sidebar
    let mut grid = Division::builder();
    grid.class("grid grid-cols-1 md:grid-cols-[1fr_280px] gap-8 items-start");

    // Left: description + main content
    let mut left = Division::builder();
    if !description.is_empty() {
        left.paragraph(|p| {
            p.class("text-fg leading-relaxed mb-8 max-w-[65ch]")
                .text(description.to_owned())
        });
    }
    left.push(body_content);
    grid.push(left.build());

    // Right: sidebar (always starts at top)
    grid.push(render_sidebar(ctx, &display_name));

    body.push(grid.build());

    // Breadcrumbs
    let ns_crumb = pkg.wit_namespace.as_ref().map(|ns| crate::nav::Crumb {
        label: ns.clone(),
        href: Some(format!("/{ns}")),
    });
    let pkg_label = pkg.wit_name.as_deref().unwrap_or(&display_name);
    let url_base = url_base_for(pkg, ctx.version);
    let pkg_crumb = crate::nav::Crumb {
        label: pkg_label.to_owned(),
        href: if extra_crumbs.is_empty() {
            None
        } else {
            Some(url_base)
        },
    };
    let crumbs: Vec<crate::nav::Crumb> = ns_crumb
        .into_iter()
        .chain(std::iter::once(pkg_crumb))
        .chain(extra_crumbs)
        .collect();

    layout::document_with_breadcrumbs(title, &body.build().to_string(), &crumbs)
}

/// Render the right sidebar with all package metadata.
fn render_sidebar(ctx: &SidebarContext<'_>, display_name: &str) -> Division {
    let pkg = ctx.pkg;
    let version = ctx.version;
    let version_detail = ctx.version_detail;
    let annotations = version_detail.and_then(|d| d.annotations.as_ref());

    let mut sidebar = Division::builder();
    sidebar.class("space-y-6 text-sm");

    // Version selector
    if !pkg.tags.is_empty() {
        let url_name = match (&pkg.wit_namespace, &pkg.wit_name) {
            (Some(ns), Some(name)) => format!("{ns}/{name}"),
            _ => pkg.repository.clone(),
        };
        sidebar.push(render_version_select(pkg, version, &url_name));
    }

    // Install command
    sidebar.division(|d| d.class("text-sm text-fg-muted mb-1").text("Install"));
    sidebar.push(render_install_command(display_name, version));

    // Metadata
    let mut meta = Division::builder();
    meta.class("space-y-2 leading-relaxed");

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
    sidebar.push(meta.build());

    // Dependencies
    if !pkg.dependencies.is_empty() {
        sidebar.division(|div| {
            div.class("border-t-2 border-fg pt-4").heading_3(|h3| {
                h3.class("text-sm font-medium text-fg-muted mb-2")
                    .text("Dependencies")
            });
            let mut ul = html::text_content::UnorderedList::builder();
            ul.class("space-y-1");
            for dep in &pkg.dependencies {
                ul.list_item(|li| {
                    li.class("font-mono text-sm");
                    match dep.package.split_once(':') {
                        Some((ns, name)) => {
                            li.anchor(|a| {
                                a.href(format!("/{ns}/{name}"))
                                    .class("text-accent hover:underline")
                                    .text(dep.package.clone())
                            });
                        }
                        None => {
                            li.span(|s| s.class("text-fg").text(dep.package.clone()));
                        }
                    }
                    if let Some(v) = &dep.version {
                        li.span(|s| s.class("text-fg-faint ml-1").text(format!("@{v}")));
                    }
                    li
                });
            }
            div.push(ul.build());
            div
        });
    }

    // Dependents
    let total_dependents = ctx.importers.len() + ctx.exporters.len();
    if total_dependents > 0 {
        sidebar.division(|div| {
            div.class("border-t-2 border-fg pt-4").heading_3(|h3| {
                h3.class("text-sm font-medium text-fg-muted mb-2")
                    .text(format!("Dependents ({total_dependents})"))
            });

            let mut all: Vec<&KnownPackage> =
                ctx.importers.iter().chain(ctx.exporters.iter()).collect();
            all.sort_by(|a, b| a.repository.cmp(&b.repository));
            all.dedup_by(|a, b| a.repository == b.repository);

            let mut ul = html::text_content::UnorderedList::builder();
            ul.class("space-y-1");
            for dep_pkg in all.iter().take(10) {
                let name = match (&dep_pkg.wit_namespace, &dep_pkg.wit_name) {
                    (Some(ns), Some(n)) => format!("{ns}:{n}"),
                    _ => dep_pkg.repository.clone(),
                };
                ul.list_item(|li| {
                    li.class("text-sm");
                    match (&dep_pkg.wit_namespace, &dep_pkg.wit_name) {
                        (Some(ns), Some(n)) => {
                            li.anchor(|a| {
                                a.href(format!("/{ns}/{n}"))
                                    .class("text-accent hover:underline font-mono")
                                    .text(name.clone())
                            });
                        }
                        _ => {
                            li.span(|s| s.class("text-fg font-mono").text(name.clone()));
                        }
                    }
                    li
                });
            }
            div.push(ul.build());

            if all.len() > 10 {
                div.paragraph(|p| {
                    p.class("text-sm text-fg-faint mt-1")
                        .text(format!("and {} more", all.len() - 10))
                });
            }
            div
        });
    }

    sidebar.build()
}

/// Compute the display name from package WIT metadata.
pub(crate) fn display_name_for(pkg: &KnownPackage) -> String {
    match (&pkg.wit_namespace, &pkg.wit_name) {
        (Some(ns), Some(name)) => format!("{ns}:{name}"),
        _ => pkg.repository.clone(),
    }
}

/// Compute the URL base for sub-page links.
pub(crate) fn url_base_for(pkg: &KnownPackage, version: &str) -> String {
    match (&pkg.wit_namespace, &pkg.wit_name) {
        (Some(ns), Some(name)) => format!("/{ns}/{name}/{version}"),
        _ => format!("/{}/{version}", pkg.repository),
    }
}

/// Render the version selector dropdown.
fn render_version_select(pkg: &KnownPackage, current_version: &str, url_name: &str) -> Division {
    let mut select = html::forms::Select::builder();
    select
        .id("version-select")
        .name("version")
        .class("w-full px-2 py-1.5 border-2 border-fg bg-page text-fg text-sm cursor-pointer");

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
        .division(|d| d.class("text-sm text-fg-muted mb-1").text("Version"))
        .push(select.build())
        .script(|s| s.text(script_body))
        .build()
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
        .division(|div| {
            div.class(
                "flex items-center gap-2 border-2 border-fg \
                 px-3 py-2 font-mono text-sm text-fg",
            )
            .code(|code| {
                code.class("flex-1 select-all overflow-hidden whitespace-nowrap text-ellipsis")
                    .text(command)
            })
            .button(|btn| {
                btn.id("copy-install-btn")
                    .class("shrink-0 text-fg-muted hover:text-fg transition-opacity cursor-pointer")
            })
            .script(|s| s.text(script))
        })
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
