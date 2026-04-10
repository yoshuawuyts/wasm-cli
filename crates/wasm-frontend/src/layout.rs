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

/// Accent color used throughout the UI.
pub(crate) const ACCENT_COLOR: &str = "#232cf4";

/// Render a complete HTML document with the given title and body content.
///
/// Includes the shared navigation bar, Tailwind CSS via CDN, custom accent
/// color CSS variables, and footer.
#[must_use]
pub(crate) fn document(title: &str, body_content: &str) -> String {
    document_inner(title, body_content, "")
}

/// Render a complete HTML document with nav bar, title, and body content.
#[must_use]
pub(crate) fn document_with_nav(title: &str, body_content: &str) -> String {
    let nav = crate::nav::render(&[]);
    document_inner(title, body_content, &nav)
}

/// Render a complete HTML document with nav bar, breadcrumbs, title, and body.
#[must_use]
pub(crate) fn document_with_breadcrumbs(
    title: &str,
    body_content: &str,
    crumbs: &[crate::nav::Crumb],
) -> String {
    let nav = crate::nav::render(crumbs);
    document_inner(title, body_content, &nav)
}

/// Inner document renderer.
fn document_inner(title: &str, body_content: &str, nav: &str) -> String {
    let escaped_title = escape_html_text(title);

    format!(
        r#"<!DOCTYPE html>
<html lang="en" style="view-transition-name:root;background:#d9d9d9">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <meta name="description" content="Browse and discover WebAssembly components and WIT interfaces published to OCI registries.">
  <title>{escaped_title} — wasm registry</title>
  <link rel="preload" href="/fonts/iosevka-regular.woff2" as="font" type="font/woff2" crossorigin>
  <link rel="preload" href="/fonts/iosevka-semibold.woff2" as="font" type="font/woff2" crossorigin>
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
              import:   'var(--color-wit-import)',
            }},
          }},
          fontFamily: {{
            sans: ['"Iosevka Web"', 'Iosevka', '"SF Mono"', '"Fira Code"', '"Cascadia Code"', 'Consolas', 'monospace'],
            mono: ['"Iosevka Web"', 'Iosevka', '"SF Mono"', '"Fira Code"', '"Cascadia Code"', 'Consolas', 'monospace'],
          }},
          letterSpacing: {{
            display: '-0.06em',
          }},
          fontSize: {{
            xs: ['0.75rem', {{ lineHeight: '1.125rem' }}],
            sm: ['0.875rem', {{ lineHeight: '1.375rem' }}],
            lg: ['1.125rem', {{ lineHeight: '1.625rem' }}],
          }},
        }}
      }}
    }}
  </script>
  <style>
    /* Self-hosted Iosevka webfont */
    @font-face {{
      font-family: 'Iosevka Web';
      font-style: normal;
      font-weight: 400;
      font-display: swap;
      src: url('/fonts/iosevka-regular.woff2') format('woff2');
    }}
    @font-face {{
      font-family: 'Iosevka Web';
      font-style: normal;
      font-weight: 500;
      font-display: swap;
      src: url('/fonts/iosevka-medium.woff2') format('woff2');
    }}
    @font-face {{
      font-family: 'Iosevka Web';
      font-style: normal;
      font-weight: 600;
      font-display: swap;
      src: url('/fonts/iosevka-semibold.woff2') format('woff2');
    }}
    @font-face {{
      font-family: 'Iosevka Web';
      font-style: normal;
      font-weight: 700;
      font-display: swap;
      src: url('/fonts/iosevka-bold.woff2') format('woff2');
    }}
    /* Color system: two-tone, inspired by charcuterie.elastiq.ch.
       Warm off-white background, vivid blue foreground. */
    :root {{
      --color-bg: #d9d9d9;
      --color-accent: {ACCENT_COLOR};
      --color-accent-hover: #1a22c0;
      --color-surface: #cfcfcf;
      --color-surface-muted: #c8c8c8;
      --color-border: rgba(35, 44, 244, 0.25);
      --color-border-light: rgba(35, 44, 244, 0.12);
      --color-fg: {ACCENT_COLOR};
      --color-fg-secondary: rgba(35, 44, 244, 0.75);
      --color-fg-muted: rgba(35, 44, 244, 0.55);
      --color-fg-faint: rgba(35, 44, 244, 0.4);
      /* WIT item kind colors */
      --color-wit-struct: #4338ca;
      --color-wit-enum: #0d7377;
      --color-wit-resource: #b45309;
      --color-wit-func: #15803d;
      --color-wit-world: #9333ea;
      --color-wit-iface: #0369a1;
      --color-wit-import: #b91c1c;
    }}
    html, body {{
      background-color: var(--color-bg);
      color: var(--color-fg);
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
    ::view-transition-old(root) {{
      animation: none;
    }}
    ::view-transition-new(root) {{
      animation: none;
    }}
    @media (prefers-reduced-motion: reduce) {{
      ::view-transition-old(root),
      ::view-transition-new(root) {{
        animation: none;
      }}
    }}
    /* Card hover — pop out with scale, shadow, and strong border */
    .card-lift {{
      transition: transform 0.1s ease-out, box-shadow 0.1s ease-out;
      transform-origin: center center;
    }}
    .card-lift:hover {{
      transform: scale(1.03);
      box-shadow: 0 4px 16px rgba(0, 0, 0, 0.1);
      z-index: 1;
      position: relative;
      outline: 2px solid var(--color-fg);
      outline-offset: -2px;
    }}
    @media (prefers-reduced-motion: reduce) {{
      .card-lift {{ transition: none; }}
      .card-lift:hover {{ transform: none; box-shadow: none; }}
    }}
    /* Card kind variants — thin left border for categorization */
    .card-interface {{
      border-left: 2px solid var(--color-wit-iface);
    }}
    .card-component {{
      border-left: 2px solid var(--color-accent);
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
      width: 1.5rem;
      height: 1.5rem;
      border: 2px solid var(--color-border);
      border-radius: 0;
      font-size: 0.8125rem;
      font-family: inherit;
      color: var(--color-fg-muted);
      background: var(--color-surface-muted);
      line-height: 1;
      pointer-events: none;
      transition: opacity 0.1s;
    }}
    .search-form:focus-within .search-kbd {{
      opacity: 0;
      pointer-events: none;
    }}
    /* Search carousel placeholder */
    .search-carousel {{
      position: absolute;
      left: 1rem;
      top: 50%;
      transform: translateY(-50%);
      font-size: 1rem;
      color: var(--color-fg-faint);
      pointer-events: none;
      white-space: nowrap;
      overflow: hidden;
      transition: opacity 0.3s cubic-bezier(0.25, 1, 0.5, 1);
    }}
    .carousel-word {{
      display: inline;
    }}
    @media (prefers-reduced-motion: reduce) {{
      .carousel-word {{
        transition: none;
      }}
    }}
    /* Tab buttons — square, bordered, Charcuterie style */
    .tab-btn {{
      padding: 0.5rem 1rem;
      font-size: 1rem;
      color: var(--color-fg);
      background: var(--color-bg);
      border: 2px solid var(--color-fg);
      border-bottom: none;
      margin-left: -2px;
      cursor: pointer;
      transition: color 0.15s, background-color 0.15s;
      flex: 1;
      display: flex;
      justify-content: space-between;
      align-items: baseline;
    }}
    .tab-btn:first-child {{
      margin-left: 0;
    }}
    .tab-btn:hover {{
      background: var(--color-fg);
      color: var(--color-bg);
    }}
    .tab-btn:hover > * {{
      opacity: 1;
    }}
    .tab-btn[aria-selected="true"] {{
      background: var(--color-fg);
      color: var(--color-bg);
    }}
    .tab-btn[aria-selected="true"] > * {{
      opacity: 1;
    }}
    @media (prefers-reduced-motion: reduce) {{
      .tab-btn {{ transition: none; }}
    }}
  </style>
</head>
<body class="bg-page text-fg min-h-screen flex flex-col leading-relaxed font-sans antialiased">
  {nav}
  <main class="flex-1 w-full max-w-6xl mx-auto px-6 sm:px-8 pb-12">
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
    // Tab switching
    document.addEventListener('click', function(e) {{
      var btn = e.target.closest('.tab-btn');
      if (!btn) return;
      var group = btn.closest('.tab-group');
      if (!group) return;
      var tab = btn.getAttribute('data-tab');
      // Update tab buttons
      group.querySelectorAll('.tab-btn').forEach(function(b) {{
        b.setAttribute('aria-selected', b === btn ? 'true' : 'false');
      }});
      // Show/hide panels
      group.querySelectorAll('.tab-panel').forEach(function(p) {{
        if (p.id === 'panel-' + tab) {{
          p.style.display = '';
        }} else {{
          p.style.display = 'none';
        }}
      }});
    }});
    // Search placeholder carousel
    (function() {{
      var words = [
        'components\u2026',
        'interfaces\u2026',
        'libraries\u2026',
        'plugins\u2026',
        'servers\u2026',
        'tools\u2026',
        'apps\u2026',
        'extensions\u2026',
        'handlers\u2026',
        'services\u2026',
        'applets\u2026',
        'clients\u2026',
        'addons\u2026',
        'modules\u2026',
        'packages\u2026',
        'widgets\u2026',
        'expansions\u2026',
        'augmentations\u2026',
        'supplements\u2026',
        'accessories\u2026',
        'middleware\u2026',
        'hooks\u2026',
        'mods\u2026',
        'bundles\u2026',
        'toolkits\u2026',
        'SDKs\u2026',
        'adapters\u2026',
        'drivers\u2026',
        'providers\u2026',
        'connectors\u2026',
        'shims\u2026',
        'polyfills\u2026',
      ];
      var el = document.getElementById('carousel-word');
      var overlay = document.getElementById('search-carousel');
      var input = document.getElementById('search-input');
      if (!el || !overlay || !input) return;
      var idx = 0;
      var reducedMotion = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
      function updateVisibility() {{
        var hasValue = input.value.length > 0;
        overlay.style.opacity = hasValue ? '0' : '';
      }}
      input.addEventListener('input', updateVisibility);
      input.addEventListener('focus', updateVisibility);
      input.addEventListener('blur', updateVisibility);
      updateVisibility();

      var currentWord = words[idx];
      el.textContent = currentWord;
      var typing = false;

      function jitter() {{
        return 50 + Math.random() * 90;
      }}

      function deleteWord(cb) {{
        var text = el.textContent;
        if (text.length === 0) {{ cb(); return; }}
        typing = true;
        var first = true;
        function step() {{
          text = text.slice(0, -1);
          el.textContent = text;
          if (text.length > 0) {{
            if (first) {{
              first = false;
              setTimeout(step, 300);
            }} else {{
              setTimeout(step, 20 + Math.random() * 25);
            }}
          }} else {{
            typing = false;
            cb();
          }}
        }}
        setTimeout(step, 20);
      }}

      function typeWord(word, cb) {{
        var i = 0;
        typing = true;
        function step() {{
          i++;
          el.textContent = word.slice(0, i);
          if (i < word.length) {{
            setTimeout(step, jitter());
          }} else {{
            typing = false;
            if (cb) cb();
          }}
        }}
        setTimeout(step, jitter());
      }}

      function cycle() {{
        if (input.value || typing) return;
        deleteWord(function() {{
          setTimeout(function() {{
            var next = idx;
            while (next === idx) next = Math.floor(Math.random() * words.length);
            idx = next;
            typeWord(words[idx]);
          }}, reducedMotion ? 0 : 200);
        }});
      }}

      setInterval(cycle, 5000);
    }})();
  </script>
</body>
</html>"#,
        escaped_title = escaped_title,
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
    // r[verify frontend.styling.accent-color]
    // r[verify frontend.styling.responsive]
    #[test]
    fn document_includes_expected_rendering_and_styling_primitives() {
        let html = document("Home", "<p>Body</p>");
        assert!(html.contains("<html lang=\"en\""));
        assert!(html.contains("https://cdn.tailwindcss.com"));
        assert!(html.contains(ACCENT_COLOR));
        assert!(html.contains("<meta name=\"viewport\""));
        assert!(html.contains("bg-page text-fg"));
        assert!(html.contains("html, body"));
        assert!(html.contains("background-color: var(--color-bg);"));
        assert!(html.contains("color: var(--color-fg);"));
    }
}
