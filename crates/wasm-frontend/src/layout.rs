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
/// Wasm logo purple in OKLCH: L=0.49, C=0.257, H=280.
pub(crate) const ACCENT_COLOR: &str = "oklch(0.49 0.257 280)";

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
        "Docs" => "/docs",
        "Search" => "/search",
        _ => "",
    };

    format!(
        r#"<!DOCTYPE html>
<html lang="en" style="view-transition-name:root">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <meta name="color-scheme" content="light dark">
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
            // WIT item kind colors
            wit: {{
              struct:   'var(--color-wit-struct)',
              enum:     'var(--color-wit-enum)',
              resource: 'var(--color-wit-resource)',
              func:     'var(--color-wit-func)',
              world:    'var(--color-wit-world)',
              iface:    'var(--color-wit-iface)',
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
    /* Color system: OKLCH, rooted in Wasm logo purple (hue 280).
       Neutrals use hue 290 for a violet tint. All text tokens
       pass WCAG AA (4.5:1) against bg. */
    :root {{
      color-scheme: light dark;
      --color-bg: oklch(1 0 290);
      --color-accent: {ACCENT_COLOR};
      --color-accent-hover: oklch(0.42 0.257 280);
      --color-surface: oklch(0.975 0.006 290);
      --color-surface-muted: oklch(0.955 0.01 290);
      --color-border: oklch(0.91 0.018 290);
      --color-border-light: oklch(0.94 0.014 290);
      --color-fg: oklch(0.20 0.03 290);
      --color-fg-secondary: oklch(0.40 0.03 290);
      --color-fg-muted: oklch(0.54 0.025 290);
      --color-fg-faint: oklch(0.56 0.02 290);
      /* WIT item kind colors */
      --color-wit-struct: oklch(0.45 0.2 260);
      --color-wit-enum: oklch(0.45 0.14 180);
      --color-wit-resource: oklch(0.50 0.16 70);
      --color-wit-func: oklch(0.42 0.2 240);
      --color-wit-world: oklch(0.48 0.18 330);
      --color-wit-iface: oklch(0.45 0.16 210);
    }}
    html, body {{
      background-color: var(--color-bg);
      color: var(--color-fg);
    }}
    @media (prefers-color-scheme: dark) {{
      :root {{
        --color-bg: oklch(0.185 0.025 290);
        --color-accent: oklch(0.70 0.16 280);
        --color-accent-hover: oklch(0.76 0.13 280);
        --color-surface: oklch(0.23 0.03 290);
        --color-surface-muted: oklch(0.26 0.035 290);
        --color-border: oklch(0.32 0.04 290);
        --color-border-light: oklch(0.29 0.038 290);
        --color-fg: oklch(0.94 0.01 290);
        --color-fg-secondary: oklch(0.78 0.025 290);
        --color-fg-muted: oklch(0.66 0.03 290);
        --color-fg-faint: oklch(0.62 0.025 290);
        /* WIT item kind colors (dark) */
        --color-wit-struct: oklch(0.72 0.15 260);
        --color-wit-enum: oklch(0.72 0.12 180);
        --color-wit-resource: oklch(0.75 0.14 70);
        --color-wit-func: oklch(0.70 0.15 240);
        --color-wit-world: oklch(0.75 0.14 330);
        --color-wit-iface: oklch(0.72 0.13 210);
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
    @view-transition {{
      navigation: auto;
    }}
    ::view-transition-old(root),
    ::view-transition-new(root) {{
      animation-duration: 0s;
    }}
    @media (prefers-reduced-motion: reduce) {{
      ::view-transition-old(root),
      ::view-transition-new(root) {{
        animation: none;
      }}
    }}
    /* Card entrance stagger */
    @keyframes card-in {{
      from {{ opacity: 0; transform: translateY(4px); }}
      to   {{ opacity: 1; transform: translateY(0); }}
    }}
    .card-enter {{
      animation: card-in 0.25s cubic-bezier(0.25, 1, 0.5, 1) both;
    }}
    @media (prefers-reduced-motion: reduce) {{
      .card-enter {{ animation: none; }}
    }}
    /* Card hover — Linear-style: border + bg shift, no vertical lift */
    .card-lift {{
      transition: border-color 0.15s, background-color 0.15s, box-shadow 0.15s, color 0.15s;
    }}
    .card-lift:hover {{
      box-shadow: 0 1px 3px oklch(0.20 0.03 290 / 0.06);
    }}
    @media (prefers-color-scheme: dark) {{
      .card-lift:hover {{
        box-shadow: 0 1px 3px oklch(0 0 0 / 0.2);
      }}
    }}
    @media (prefers-reduced-motion: reduce) {{
      .card-lift {{ transition: none; }}
    }}
    /* Search focus ring — Linear-style */
    .search-glow:focus {{
      box-shadow: 0 0 0 3px oklch(0.49 0.257 280 / 0.12);
    }}
    @media (prefers-color-scheme: dark) {{
      .search-glow:focus {{
        box-shadow: 0 0 0 3px oklch(0.70 0.16 280 / 0.2);
      }}
    }}
    /* Button press */
    .btn-press:active {{
      transform: translateY(1px);
    }}
    /* Copy hint */
    .copy-hint {{
      cursor: pointer;
      position: relative;
    }}
    .copy-hint::after {{
      content: 'click to copy';
      position: absolute;
      right: -0.25rem;
      top: 50%;
      transform: translateX(100%) translateY(-50%);
      font-size: 0.65rem;
      color: var(--color-fg-faint);
      opacity: 0;
      transition: opacity 0.15s;
      white-space: nowrap;
      pointer-events: none;
    }}
    .copy-hint:hover::after {{
      opacity: 1;
    }}
    .copy-hint.copied::after {{
      content: 'copied!';
      color: var(--color-accent);
      opacity: 1;
    }}
    @media (prefers-reduced-motion: reduce) {{
      .copy-hint::after {{ transition: none; }}
    }}
    /* Keyboard shortcut badge — inside search input, Linear-style */
    .search-kbd {{
      position: absolute;
      right: 0.5rem;
      top: 50%;
      transform: translateY(-50%);
      display: inline-flex;
      align-items: center;
      justify-content: center;
      width: 1.25rem;
      height: 1.25rem;
      border: 1px solid var(--color-border);
      border-radius: 0.25rem;
      font-size: 0.7rem;
      font-family: inherit;
      color: var(--color-fg-faint);
      background: var(--color-surface);
      line-height: 1;
      pointer-events: none;
      transition: opacity 0.1s;
    }}
    .search-form:focus-within .search-kbd {{
      opacity: 0;
      pointer-events: none;
    }}
  </style>
</head>
<body class="bg-page text-fg min-h-screen flex flex-col leading-relaxed">
  {nav}
  <main class="flex-1 w-full max-w-5xl mx-auto px-4 sm:px-6 pb-10">
    {body_content}
  </main>
  {footer}
  <script>
    // Focus search on / key (developer convention)
    document.addEventListener('keydown', function(e) {{
      if (e.key === '/' && !e.ctrlKey && !e.metaKey && !e.altKey) {{
        var el = document.activeElement;
        var tag = el && el.tagName;
        if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT' || (el && el.isContentEditable)) return;
        var search = document.getElementById('search-input');
        if (search) {{ e.preventDefault(); search.focus(); }}
      }}
    }});
    // Click-to-copy for install hint
    document.addEventListener('click', function(e) {{
      var el = e.target.closest('.copy-hint');
      if (!el) return;
      var text = el.textContent || '';
      if (navigator.clipboard) {{
        navigator.clipboard.writeText(text).then(function() {{
          el.classList.add('copied');
          setTimeout(function() {{ el.classList.remove('copied'); }}, 1200);
        }});
      }}
    }});
  </script>
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
        assert!(html.contains("<html lang=\"en\""));
        assert!(html.contains("https://cdn.tailwindcss.com"));
        assert!(html.contains(ACCENT_COLOR));
        assert!(html.contains("<meta name=\"viewport\""));
        assert!(html.contains("<meta name=\"color-scheme\" content=\"light dark\">"));
        assert!(html.contains("bg-page text-fg"));
        assert!(html.contains("html, body"));
        assert!(html.contains("background-color: var(--color-bg);"));
        assert!(html.contains("color: var(--color-fg);"));
        assert!(html.contains("prefers-color-scheme: dark"));
    }
}
