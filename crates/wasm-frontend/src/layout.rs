//! Base HTML document layout.
//!
//! Provides the shared page shell — `<html>`, `<head>`, and `<body>` wrapper —
//! used by all pages.

// r[impl frontend.rendering.html-crate]
// r[impl frontend.styling.tailwind]
// r[impl frontend.styling.light-theme]
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
            accent: '{ACCENT_COLOR}',
            'accent-hover': '#6a4bf0',
            // Violet-tinted neutrals (hue 260)
            surface: {{
              DEFAULT: '#f8f7fb',  // faint violet tint for sections
              muted:   '#f1eff6',  // slightly stronger for cards/wells
            }},
            border: {{
              DEFAULT: '#e4e0ed', // tinted border
              light:   '#eeeaf5', // tinted divider
            }},
            fg: {{
              DEFAULT:   '#1a1625', // tinted near-black
              secondary: '#534e63', // tinted gray-600
              muted:     '#7c7691', // tinted gray-500
              faint:     '#a9a3bc', // tinted gray-400
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
      --accent: {ACCENT_COLOR};
    }}
    /* Consistent focus ring for keyboard navigation */
    :focus-visible {{
      outline: 2px solid {ACCENT_COLOR};
      outline-offset: 2px;
    }}
    /* Remove default outline when not keyboard-navigating */
    :focus:not(:focus-visible) {{
      outline: none;
    }}
  </style>
</head>
<body class="bg-white text-fg min-h-screen flex flex-col leading-relaxed">
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
