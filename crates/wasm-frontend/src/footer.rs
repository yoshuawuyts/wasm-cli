//! Footer component.

use html::content::Footer;

/// Render the site footer.
#[must_use]
pub(crate) fn render() -> String {
    Footer::builder()
        .class("border-t border-border mt-16")
        .division(|div| {
            div.class("max-w-5xl mx-auto px-4 py-8 flex flex-col gap-4 text-sm text-fg-muted")
                .division(|nav| {
                    nav.class("flex items-center justify-between")
                        .anchor(|a| {
                            a.href("/")
                                .class("text-lg font-bold tracking-tight text-fg hover:text-accent transition-colors")
                                .text("wasm")
                        })
                        .division(|links| {
                            links
                                .class("flex gap-5")
                                .anchor(|a| {
                                    a.href("/all")
                                        .class("text-fg-muted hover:text-fg transition-colors")
                                        .text("Browse")
                                })
                                .anchor(|a| {
                                    a.href("/docs")
                                        .class("text-fg-muted hover:text-fg transition-colors")
                                        .text("Docs")
                                })
                                .anchor(|a| {
                                    a.href("/about")
                                        .class("text-fg-muted hover:text-fg transition-colors")
                                        .text("About")
                                })
                        })
                })
                .paragraph(|p| {
                    p.class("text-fg-faint text-xs")
                        .text("wasm registry \u{2014} ")
                        .anchor(|a| {
                            a.href("https://github.com/yoshuawuyts/wasm-cli")
                                .class("text-accent hover:underline transition-colors")
                                .text("open-source")
                        })
                        .text(" on GitHub")
                })
        })
        .build()
        .to_string()
}
