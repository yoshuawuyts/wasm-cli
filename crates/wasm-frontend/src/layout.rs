//! Base HTML document layout.
//!
//! Provides the shared page shell — `<html>`, `<head>`, and `<body>` wrapper —
//! used by all pages.

// r[impl frontend.rendering.html-crate]
// r[impl frontend.styling.tailwind]
// r[impl frontend.styling.light-theme]
// r[impl frontend.styling.dark-mode]
// r[impl frontend.styling.accent-color]
// r[impl frontend.styling.responsive]

use crate::footer;
use crate::nav;

/// Accent color used throughout the UI.
///
/// RGB: R81 G47 B235 → `#512FEB`.
pub(crate) const ACCENT_COLOR: &str = "#512FEB";

/// Render a complete HTML document with the given title and body content.
///
/// Includes the shared navigation bar, Tailwind CSS via CDN, custom accent
/// color CSS variables, and footer.
#[must_use]
pub(crate) fn document(title: &str, body_content: &str) -> String {
    let escaped_title = escape_html_text(title);
    let current_path = match title {
        "Home" => "/",
        "All Packages" => "/all",
        "About" => "/about",
        "Search" => "/search",
        _ => "",
    };

    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <meta name="description" content="Browse and discover WebAssembly components and WIT interfaces published to OCI registries.">
  <title>{escaped_title} — wasm registry</title>
  <script src="https://cdn.tailwindcss.com"></script>
  <script>
    tailwind.config = {{
      theme: {{
        extend: {{
          colors: {{
            accent: 'var(--color-accent)',
            'accent-hover': 'var(--color-accent-hover)',
            page: 'var(--color-bg)',
            // Violet-tinted neutrals driven by CSS custom properties
            surface: {{
              DEFAULT: 'var(--color-surface)',
              muted:   'var(--color-surface-muted)',
            }},
            border: {{
              DEFAULT: 'var(--color-border)',
              light:   'var(--color-border-light)',
            }},
            fg: {{
              DEFAULT:   'var(--color-fg)',
              secondary: 'var(--color-fg-secondary)',
              muted:     'var(--color-fg-muted)',
              faint:     'var(--color-fg-faint)',
            }},
          }},
          fontFamily: {{
            mono: ['ui-monospace', 'Cascadia Code', 'Source Code Pro', 'Menlo', 'Consolas', 'DejaVu Sans Mono', 'monospace'],
          }},
        }}
      }}
    }}
  </script>
  <style>
    :root {{
      --color-bg: #ffffff;
      --color-accent: {ACCENT_COLOR};
      --color-accent-hover: #6a4bf0;
      --color-surface: #f8f7fb;
      --color-surface-muted: #f1eff6;
      --color-border: #e4e0ed;
      --color-border-light: #eeeaf5;
      --color-fg: #1a1625;
      --color-fg-secondary: #534e63;
      --color-fg-muted: #7c7691;
      --color-fg-faint: #a9a3bc;
    }}
    @media (prefers-color-scheme: dark) {{
      :root {{
        --color-bg: #13111d;
        --color-accent: #7c5df5;
        --color-accent-hover: #9678ff;
        --color-surface: #1e1b2e;
        --color-surface-muted: #252238;
        --color-border: #352f4a;
        --color-border-light: #2d2842;
        --color-fg: #eae8f0;
        --color-fg-secondary: #b8b3c8;
        --color-fg-muted: #8e88a3;
        --color-fg-faint: #6b6580;
      }}
    }}
    /* Consistent focus ring for keyboard navigation */
    :focus-visible {{
      outline: 2px solid var(--color-accent);
      outline-offset: 2px;
    }}
    /* Remove default outline when not keyboard-navigating */
    :focus:not(:focus-visible) {{
      outline: none;
    }}
  </style>
</head>
<body class="bg-page text-fg min-h-screen flex flex-col leading-relaxed">
  {nav}
  <main class="flex-1 w-full max-w-5xl mx-auto px-4 sm:px-6 py-10">
    {body_content}
  </main>
  {footer}
</body>
</html>"#,
        escaped_title = escaped_title,
        nav = nav::render(current_path),
        footer = footer::render(),
        body_content = body_content,
    )
}

#[must_use]
fn escape_html_text(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#x27;"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;

    // r[verify frontend.rendering.html-crate]
    // r[verify frontend.styling.tailwind]
    // r[verify frontend.styling.light-theme]
    // r[verify frontend.styling.dark-mode]
    // r[verify frontend.styling.accent-color]
    // r[verify frontend.styling.responsive]
    #[test]
    fn document_includes_expected_rendering_and_styling_primitives() {
        let html = document("Home", "<p>Body</p>");
        assert!(html.contains("<html lang=\"en\">"));
        assert!(html.contains("https://cdn.tailwindcss.com"));
        assert!(html.contains(ACCENT_COLOR));
        assert!(html.contains("<meta name=\"viewport\""));
        assert!(html.contains("bg-page text-fg"));
        assert!(html.contains("prefers-color-scheme: dark"));
    }
}
