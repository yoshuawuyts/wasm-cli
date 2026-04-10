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
pub(crate) const ACCENT_COLOR: &str = "#fcfdf7";

/// Render a complete HTML document with the given title and body content.
///
/// Includes the shared navigation bar, Tailwind CSS via CDN, custom accent
/// color CSS variables, and footer.
#[must_use]
pub(crate) fn document(title: &str, body_content: &str) -> String {
    let escaped_title = escape_html_text(title);

    format!(
        r#"<!DOCTYPE html>
<html lang="en" style="view-transition-name:root">
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
            }},
          }},
          fontFamily: {{
            sans: ['"Iosevka Web"', 'Iosevka', '"SF Mono"', '"Fira Code"', '"Cascadia Code"', 'Consolas', 'monospace'],
            mono: ['"Iosevka Web"', 'Iosevka', '"SF Mono"', '"Fira Code"', '"Cascadia Code"', 'Consolas', 'monospace'],
          }},
          letterSpacing: {{
            display: '-0.06em',
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
      --color-bg: #232cf4;
      --color-accent: {ACCENT_COLOR};
      --color-accent-hover: #e8e9e3;
      --color-surface: rgba(252, 253, 247, 0.08);
      --color-surface-muted: rgba(252, 253, 247, 0.05);
      --color-border: {ACCENT_COLOR};
      --color-border-light: rgba(252, 253, 247, 0.2);
      --color-fg: {ACCENT_COLOR};
      --color-fg-secondary: {ACCENT_COLOR};
      --color-fg-muted: {ACCENT_COLOR};
      --color-fg-faint: {ACCENT_COLOR};
      /* WIT item kind colors */
      --color-wit-struct: {ACCENT_COLOR};
      --color-wit-enum: {ACCENT_COLOR};
      --color-wit-resource: {ACCENT_COLOR};
      --color-wit-func: {ACCENT_COLOR};
      --color-wit-world: {ACCENT_COLOR};
      --color-wit-iface: {ACCENT_COLOR};
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
    /* Card hover — border change only, no shadows */
    .card-lift {{
      transition: border-color 0.15s, background-color 0.15s;
    }}
    @media (prefers-reduced-motion: reduce) {{
      .card-lift {{ transition: none; }}
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
      border: 1px solid var(--color-border);
      border-radius: 0.375rem;
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
      display: inline-block;
      opacity: 0;
      transform: translateY(-0.6em);
    }}
    /* Exit: slow start, then accelerate down and away */
    .carousel-word.out {{
      opacity: 0;
      transform: translateY(0.6em);
      transition:
        opacity 0.25s cubic-bezier(0.55, 0, 1, 0.45),
        transform 0.25s cubic-bezier(0.55, 0, 1, 0.45);
    }}
    /* Enter: fly in fast from above, overshoot slightly, settle back */
    .carousel-word.in {{
      opacity: 1;
      transform: translateY(0);
      transition:
        opacity 0.3s cubic-bezier(0.22, 1.15, 0.36, 1),
        transform 0.3s cubic-bezier(0.22, 1.15, 0.36, 1);
    }}
    @media (prefers-reduced-motion: reduce) {{
      .carousel-word,
      .carousel-word.out,
      .carousel-word.in {{
        transition: none;
      }}
    }}
    /* Tab buttons */
    .tab-btn {{
      padding: 0.5rem 0.75rem;
      font-size: 0.875rem;
      color: var(--color-fg-muted);
      background: none;
      border: none;
      border-bottom: 2px solid transparent;
      cursor: pointer;
      transition: color 0.15s, border-color 0.15s;
      margin-bottom: -1px;
    }}
    .tab-btn:hover {{
      color: var(--color-fg);
    }}
    .tab-btn[aria-selected="true"] {{
      color: var(--color-fg);
      border-bottom-color: var(--color-accent);
    }}
    @media (prefers-reduced-motion: reduce) {{
      .tab-btn {{ transition: none; }}
    }}
  </style>
</head>
<body class="bg-page text-fg min-h-screen flex flex-col leading-relaxed font-sans antialiased">
  <main class="flex-1 w-full max-w-6xl mx-auto px-4 sm:px-6 pb-10">
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
      el.classList.add('in');
      setInterval(function() {{
        if (input.value) return;
        // Fade out gently
        el.classList.remove('in');
        el.classList.add('out');
        // Swap text after exit completes, then fade in
        var swapDelay = reducedMotion ? 0 : 250;
        setTimeout(function() {{
          var next = idx;
          while (next === idx) next = Math.floor(Math.random() * words.length);
          idx = next;
          el.textContent = words[idx];
          // Disable transition, snap to top start position, then animate in
          el.classList.remove('out');
          el.style.transition = 'none';
          // Force reflow so the snap is committed
          void el.offsetWidth;
          el.style.transition = '';
          requestAnimationFrame(function() {{
            el.classList.add('in');
          }});
        }}, swapDelay);
      }}, 7000);
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
