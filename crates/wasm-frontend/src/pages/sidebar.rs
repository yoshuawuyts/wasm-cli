//! Shared sidebar components for detail pages.
//!
//! Provides a navigation sidebar showing sibling interfaces/worlds and
//! package metadata, matching the layout of the package detail page.

use html::content::Aside;
use html::text_content::{Division, UnorderedList};
use wasm_wit_doc::WitDocument;

/// Context needed to render the detail page sidebar.
pub(crate) struct SidebarContext<'a> {
    /// The package display name (e.g. `"wasi:cli"`).
    pub display_name: &'a str,
    /// The current version string.
    pub version: &'a str,
    /// The parsed WIT document for navigation links.
    pub doc: &'a WitDocument,
    /// Which sidebar item is currently active.
    pub active: SidebarActive<'a>,
}

/// Which item in the sidebar is currently active.
pub(crate) enum SidebarActive<'a> {
    /// An interface page (name of the interface).
    Interface(&'a str),
    /// An item within an interface (interface name, item name).
    Item(&'a str, #[allow(dead_code)] &'a str),
    /// A world page (name of the world).
    World(&'a str),
}

/// Render the sidebar for a detail page.
pub(crate) fn render_sidebar(ctx: &SidebarContext<'_>) -> Aside {
    let pkg_url = format!(
        "/{}/{}",
        ctx.display_name.replace(':', "/"),
        ctx.version
    );

    let mut aside = Aside::builder();
    aside.class("space-y-4");

    // Navigation card
    aside.push(render_nav_card(ctx, &pkg_url));

    // Metadata card
    aside.push(render_meta_card(ctx, &pkg_url));

    aside.build()
}

/// Render the navigation card with interfaces and worlds.
fn render_nav_card(ctx: &SidebarContext<'_>, pkg_url: &str) -> Division {
    let mut card = Division::builder();
    card.class("bg-surface border border-border rounded-lg p-4 text-sm");

    // Package link at top
    card.division(|d| {
        d.class("mb-3 pb-3 border-b border-border")
            .anchor(|a| {
                a.href(pkg_url.to_owned())
                    .class("text-accent hover:underline font-semibold text-sm")
                    .text(ctx.display_name.to_owned())
            })
    });

    // Worlds section
    if !ctx.doc.worlds.is_empty() {
        card.division(|d| {
            d.class("mb-3")
                .division(|label| {
                    label
                        .class("text-fg-muted text-xs uppercase tracking-wide mb-1.5")
                        .text("Worlds")
                });
            let mut ul = UnorderedList::builder();
            ul.class("space-y-0.5");
            for world in &ctx.doc.worlds {
                let is_active = matches!(
                    ctx.active,
                    SidebarActive::World(name) if name == world.name
                );
                let style = if is_active {
                    "block px-2 py-1 rounded text-accent bg-accent/10 font-medium text-xs font-mono truncate"
                } else {
                    "block px-2 py-1 rounded text-fg hover:text-accent hover:bg-surface-muted text-xs font-mono truncate transition-colors"
                };
                ul.list_item(|li| {
                    li.anchor(|a| {
                        a.href(world.url.clone())
                            .class(style.to_owned())
                            .text(world.name.clone())
                    })
                });
            }
            d.push(ul.build());
            d
        });
    }

    // Interfaces section
    if !ctx.doc.interfaces.is_empty() {
        card.division(|d| {
            d.division(|label| {
                    label
                        .class("text-fg-muted text-xs uppercase tracking-wide mb-1.5")
                        .text("Interfaces")
                });
            let mut ul = UnorderedList::builder();
            ul.class("space-y-0.5");
            for iface in &ctx.doc.interfaces {
                let is_active = matches!(
                    ctx.active,
                    SidebarActive::Interface(name) if name == iface.name
                ) || matches!(
                    ctx.active,
                    SidebarActive::Item(iface_name, _) if iface_name == iface.name
                );
                let style = if is_active {
                    "block px-2 py-1 rounded text-accent bg-accent/10 font-medium text-xs font-mono truncate"
                } else {
                    "block px-2 py-1 rounded text-fg hover:text-accent hover:bg-surface-muted text-xs font-mono truncate transition-colors"
                };
                ul.list_item(|li| {
                    li.anchor(|a| {
                        a.href(iface.url.clone())
                            .class(style.to_owned())
                            .text(iface.name.clone())
                    })
                });
            }
            d.push(ul.build());
            d
        });
    }

    card.build()
}

/// Render the metadata card (install command, version, etc.).
fn render_meta_card(ctx: &SidebarContext<'_>, pkg_url: &str) -> Division {
    let install_cmd = format!(
        "wasm install {}@{}",
        ctx.display_name, ctx.version
    );

    Division::builder()
        .class("bg-surface border border-border rounded-lg p-4 space-y-3 text-sm")
        .division(|d| {
            d.class("text-fg-muted text-xs uppercase tracking-wide")
                .text("Install")
        })
        .division(|d| {
            d.class(
                "flex items-center gap-2 bg-surface-muted border border-border \
                 rounded-md px-3 py-2 font-mono text-xs text-fg",
            )
            .code(|code| {
                code.class("flex-1 select-all overflow-hidden whitespace-nowrap text-ellipsis")
                    .text(install_cmd)
            })
        })
        .division(|d| {
            d.class("text-fg-muted text-xs uppercase tracking-wide")
                .text("Version")
        })
        .division(|d| {
            d.class("text-fg mt-0.5")
                .anchor(|a| {
                    a.href(pkg_url.to_owned())
                        .class("text-accent hover:underline")
                        .text(ctx.version.to_owned())
                })
        })
        .build()
}
