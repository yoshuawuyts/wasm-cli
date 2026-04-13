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
}

/// Render the shared page shell: two-column layout with sidebar,
/// wrapped in the HTML document layout.
#[must_use]
pub(crate) fn render_page(
    ctx: &SidebarContext<'_>,
    title: &str,
    body_content: &Division,
) -> String {
    render_page_inner(ctx, title, body_content, &[])
}

/// Render the page shell with extra breadcrumb segments after the package name.
#[must_use]
pub(crate) fn render_page_with_crumbs(
    ctx: &SidebarContext<'_>,
    title: &str,
    body_content: &Division,
    extra_crumbs: &[crate::nav::Crumb],
) -> String {
    render_page_inner(ctx, title, body_content, extra_crumbs)
}

/// Inner page shell renderer.
///
/// Uses a "golden layout": left sidebar with navigation and metadata,
/// right column for main content. The top nav bar is replaced by the
/// sidebar's own logo, breadcrumbs, and search.
fn render_page_inner(
    ctx: &SidebarContext<'_>,
    title: &str,
    body_content: &Division,
    extra_crumbs: &[crate::nav::Crumb],
) -> String {
    let pkg = ctx.pkg;
    let version = ctx.version;
    let display_name = display_name_for(pkg);

    // Build breadcrumbs (extra crumbs only — package name is in the navbar)
    let breadcrumb_html = render_breadcrumb_path(extra_crumbs);

    // Build sidebar metadata
    let sidebar_meta = render_sidebar(ctx, &display_name).to_string();

    // Build main content
    let content = body_content.to_string();

    // Top navbar with bunny, breadcrumbs, and links
    // Golden layout below: sidebar left, content right
    let pkg_url = url_base_for(pkg, version);
    let pkg_name_html = match (&pkg.wit_namespace, &pkg.wit_name) {
        (Some(ns), Some(name)) => {
            format!(
                r#"<a href="/{ns}" class="text-fg-muted hover:text-fg transition-colors">{ns}</a><span class="text-fg-faint">:</span><a href="{pkg_url}" class="text-fg-muted hover:text-fg transition-colors">{name}</a>"#
            )
        }
        _ => {
            format!(
                r#"<a href="{pkg_url}" class="text-fg-muted hover:text-fg transition-colors">{display_name}</a>"#
            )
        }
    };
    let body = format!(
        r#"<style>
  .page-grid {{
    display: grid;
    grid-template-columns: 260px 1fr;
    grid-template-areas:
      "sidebar topbar"
      "sidebar main";
    gap: 0 2.5rem;
    min-height: 100vh;
  }}
  @media (min-width: 1280px) {{
    .page-grid {{
      grid-template-columns: 260px 1fr auto;
      grid-template-areas:
        "sidebar main rightbar"
        "sidebar main rightbar";
    }}
    .page-grid .topbar {{
      display: none;
    }}
    .page-grid .rightbar {{
      display: block;
    }}
  }}
</style>
<div class="page-grid px-3 sm:px-4 pt-3 xl:pt-6">
  <aside class="space-y-5" style="grid-area:sidebar;position:sticky;top:1.5rem;align-self:start;display:flex;flex-direction:column;height:calc(100vh - 3rem)">
    <div class="space-y-5 flex-1">
    <a href="/" id="bunny" aria-label="Home" role="link" class="text-lg font-medium text-fg hover:text-accent transition-colors shrink-0 inline-block text-left mb-4" style="cursor:pointer;min-width:10ch">(๑╹ᆺ╹)</a>
    {sidebar_meta}
    </div>
    <p class="text-sm text-fg-faint pb-6">Made by Yosh Wuyts</p>
  </aside>
  <div class="topbar flex items-center justify-end gap-4 pb-2" style="grid-area:topbar">
    <a href="/docs" class="text-sm text-fg-muted hover:text-fg transition-colors">Docs</a>
    <a href="/downloads" class="text-sm text-fg-muted hover:text-fg transition-colors">Downloads</a>
    <form action="/search" method="get" class="relative flex">
      <input type="search" name="q" placeholder="Search…" aria-label="Search" class="w-48 px-3 pr-12 py-1.5 text-sm border-2 border-fg bg-page text-fg-muted focus:text-fg focus:outline-none" id="search-input">
      <span class="absolute right-3 top-1/2 -translate-y-1/2 text-sm font-mono pointer-events-none opacity-30" aria-hidden="true">[ / ]</span>
    </form>
  </div>
  <div style="grid-area:main;min-width:0;max-width:48rem">
    <div class="flex flex-wrap items-baseline text-2xl font-light tracking-display mb-6">
      {pkg_name_html}{breadcrumb_html}
    </div>
    {content}
  </div>
  <aside class="rightbar hidden" style="grid-area:rightbar;position:sticky;top:1.5rem;align-self:start">
    <div class="flex items-center gap-4">
      <a href="/docs" class="text-sm text-fg-muted hover:text-fg transition-colors">Docs</a>
      <a href="/downloads" class="text-sm text-fg-muted hover:text-fg transition-colors">Downloads</a>
      <form action="/search" method="get" class="relative flex">
        <input type="search" name="q" placeholder="Search…" aria-label="Search" class="w-36 px-3 pr-10 py-1.5 text-sm border-2 border-fg bg-page text-fg-muted focus:text-fg focus:outline-none" id="search-input-lg">
        <span class="absolute right-3 top-1/2 -translate-y-1/2 text-sm font-mono pointer-events-none opacity-30" aria-hidden="true">[ / ]</span>
      </form>
    </div>
  </aside>
</div>
<script>
(function(){{
  var b=document.getElementById('bunny');
  if(!b)return;
  var anims=[
    ['(๑╹ᆺ╹)','(๑°ᆺ°)!','(๑◉ᆺ◉)!!'],
    ['(๑╹ᆺ╹)','(๑°ᆺ°)♪','ヽ(๑≧ᆺ≦)ノ'],
    ['(๑╹ᆺ╹)','(๑╹ᆺ╹)>','(๑°ᆺ°)>>']
  ];
  var seq=anims[Math.floor(Math.random()*anims.length)];
  var timer=null;
  b.addEventListener('mouseenter',function(){{
    if(timer)return;
    b.textContent=seq[1];
    timer=setTimeout(function(){{
      b.textContent=seq[2];
    }},80);
  }});
  b.addEventListener('mouseleave',function(){{
    if(timer){{clearTimeout(timer);timer=null;}}
    b.textContent=seq[0];
  }});
}})();
</script>"#
    );

    layout::document_full_width(title, &body)
}

/// Render breadcrumb segments as inline HTML.
fn render_breadcrumb_path(crumbs: &[crate::nav::Crumb]) -> String {
    use std::fmt::Write;
    let mut html = String::new();
    for crumb in crumbs {
        html.push_str(r#" <span class="text-fg-faint mx-0.5">/</span> "#);
        if let Some(href) = &crumb.href {
            write!(
                html,
                r#"<a href="{href}" class="text-fg-muted hover:text-fg transition-colors">{label}</a>"#,
                label = crumb.label
            )
            .unwrap();
        } else {
            write!(
                html,
                r#"<span class="text-fg">{label}</span>"#,
                label = crumb.label
            )
            .unwrap();
        }
    }
    html
}

/// Render the right sidebar with all package metadata.
fn render_sidebar(ctx: &SidebarContext<'_>, display_name: &str) -> Division {
    let pkg = ctx.pkg;
    let version = ctx.version;
    let version_detail = ctx.version_detail;
    let annotations = version_detail.and_then(|d| d.annotations.as_ref());

    let mut sidebar = Division::builder();
    sidebar.class("space-y-4 text-sm");

    // Version selector
    if !pkg.tags.is_empty() {
        let url_name = match (&pkg.wit_namespace, &pkg.wit_name) {
            (Some(ns), Some(name)) => format!("{ns}/{name}"),
            _ => pkg.repository.clone(),
        };
        sidebar.push(render_version_select(pkg, version, &url_name));
    }

    // Install command
    let install_cmd = render_install_command(display_name, version);
    sidebar.push(install_cmd);

    // Metadata
    sidebar.division(|wrapper| {
        wrapper.class("").division(|label| {
            label
                .class("text-sm font-medium text-fg-muted border-2 border-b-0 border-fg px-3 py-1 inline-block")
                .text("Metadata")
        });
        let mut meta = Division::builder();
        meta.class("space-y-3 border-2 border-fg p-3");

        {
            let registry_url = format!("https://{}/{}", pkg.registry, pkg.repository);
            let registry_display = format!("{}/{}", pkg.registry, pkg.repository);
            meta.push(meta_link_row("Registry", &registry_display, &registry_url));
        }
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
        wrapper.push(meta.build());
        wrapper
    });

    // Dependencies
    if !pkg.dependencies.is_empty() {
        sidebar.division(|wrapper| {
            wrapper.class("").heading_3(|h3| {
                h3.class("text-sm font-medium text-fg-muted border-2 border-b-0 border-fg px-3 py-1 inline-block")
                    .text("Dependencies")
            });
            wrapper.division(|div| {
                div.class("border-2 border-fg p-3");
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
            wrapper
        });
    }

    // Dependents
    let total_dependents = ctx.importers.len() + ctx.exporters.len();
    if total_dependents > 0 {
        sidebar.division(|wrapper| {
            wrapper.class("").heading_3(|h3| {
                h3.class("text-sm font-medium text-fg-muted border-2 border-b-0 border-fg px-3 py-1 inline-block")
                    .text(format!("Dependents ({total_dependents})"))
            });
            wrapper.division(|div| {
                div.class("border-2 border-fg p-3");

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
            wrapper
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
    let script_body = format!(
        "document.getElementById('version-select').addEventListener('change',function(){{\
        var p=window.location.pathname;\
        var base='/{url_name}/';\
        var rest=p.indexOf(base)===0?p.slice(base.length):'';\
        var slash=rest.indexOf('/');\
        var sub=slash>=0?rest.slice(slash):'';\
        window.location.href=base+this.value+sub\
        }})"
    );

    Division::builder()
        .division(|d| {
            d.class("flex items-center gap-2 border-2 border-fg px-3 py-2")
                .span(|s| s.class("text-sm text-fg-muted shrink-0").text("Version"))
                .push({
                    let mut s = html::forms::Select::builder();
                    s.id("version-select")
                        .name("version")
                        .class("flex-1 bg-transparent text-fg text-sm cursor-pointer border-0 outline-none");
                    for tag in &pkg.tags {
                        let is_current = tag == current_version;
                        if is_current {
                            s.option(|opt| opt.value(tag.clone()).text(tag.clone()).selected(true));
                        } else {
                            s.option(|opt| opt.value(tag.clone()).text(tag.clone()));
                        }
                    }
                    s.build()
                })
        })
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
        .class("")
        .span(|s| {
            s.class("text-fg-muted block text-sm")
                .text(label.to_owned())
        })
        .span(|s| s.class("text-fg text-sm").text(value.to_owned()))
        .build()
}

/// Render a label: linked-value metadata row.
fn meta_link_row(label: &str, text: &str, href: &str) -> Division {
    Division::builder()
        .class("")
        .span(|s| {
            s.class("text-fg-muted block text-sm")
                .text(label.to_owned())
        })
        .anchor(|a| {
            a.href(href.to_owned())
                .class("text-accent hover:underline break-all")
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
