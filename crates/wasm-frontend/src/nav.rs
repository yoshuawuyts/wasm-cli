//! Navigation bar component.

/// Render the site navigation bar.
///
/// `current_path` is used to mark the active link with `aria-current="page"`.
#[must_use]
pub(crate) fn render(current_path: &str) -> String {
    let all_aria = if current_path == "/all" {
        r#" aria-current="page""#
    } else {
        ""
    };
    let docs_aria = if current_path == "/docs" {
        r#" aria-current="page""#
    } else {
        ""
    };
    let engines_aria = if current_path == "/engines" {
        r#" aria-current="page""#
    } else {
        ""
    };
    let about_aria = if current_path == "/about" {
        r#" aria-current="page""#
    } else {
        ""
    };

    format!(
        r#"<nav class="w-full max-w-5xl mx-auto px-4 sm:px-6 pt-6 pb-2 flex items-center justify-between" aria-label="Main">
  <a href="/" class="text-lg font-bold tracking-tight text-fg hover:text-accent transition-colors">wasm</a>
  <div class="flex gap-5 text-sm">
    <a href="/all" class="text-fg-muted hover:text-fg transition-colors"{all_aria}>Packages</a>
    <a href="/engines" class="text-fg-muted hover:text-fg transition-colors"{engines_aria}>Engines</a>
    <a href="/docs" class="text-fg-muted hover:text-fg transition-colors"{docs_aria}>Docs</a>
    <a href="/about" class="text-fg-muted hover:text-fg transition-colors"{about_aria}>About</a>
  </div>
</nav>"#,
    )
}
