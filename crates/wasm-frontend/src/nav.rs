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
    let about_aria = if current_path == "/about" {
        r#" aria-current="page""#
    } else {
        ""
    };

    format!(
        r#"<header class="bg-accent text-white">
  <nav class="max-w-5xl mx-auto px-4 sm:px-6 py-3 flex items-center justify-between gap-4" aria-label="Main">
    <a href="/" class="text-xl font-bold tracking-tight hover:text-white/80 transition-colors shrink-0">wasm</a>
    <form action="/search" method="get" class="hidden sm:flex flex-1 max-w-md mx-4">
      <input type="search" name="q" placeholder="Search packages…" aria-label="Search packages" class="w-full px-3 py-1.5 rounded-l text-sm bg-white/15 text-white placeholder:text-white/60 border border-white/20 focus:bg-white/25 focus:border-white/40 focus:outline-none transition-colors">
      <button type="submit" class="px-3 py-1.5 rounded-r text-sm font-medium bg-white/20 border border-l-0 border-white/20 hover:bg-white/30 transition-colors">Search</button>
    </form>
    <div class="hidden sm:flex gap-6 text-sm font-medium shrink-0">
      <a href="/all" class="underline-offset-4 hover:underline transition-colors"{all_aria}>All Packages</a>
      <a href="/about" class="underline-offset-4 hover:underline transition-colors"{about_aria}>About</a>
    </div>
    <details class="sm:hidden relative">
      <summary class="list-none cursor-pointer p-2 -mr-2 rounded hover:bg-white/10 transition-colors" aria-label="Toggle menu">
        <span class="sr-only">Toggle menu</span>
        <svg class="w-5 h-5 inline-block" fill="none" stroke="currentColor" stroke-width="2" viewBox="0 0 24 24" aria-hidden="true">
          <path stroke-linecap="round" stroke-linejoin="round" d="M4 6h16M4 12h16M4 18h16"/>
        </svg>
      </summary>
      <div class="absolute right-0 mt-2 w-56 bg-accent border border-white/20 rounded-md shadow-lg px-3 py-2 space-y-1 text-sm font-medium z-10">
        <form action="/search" method="get" class="flex mb-2">
          <input type="search" name="q" placeholder="Search packages…" aria-label="Search packages" class="flex-1 px-3 py-2 rounded-l text-sm bg-white/15 text-white placeholder:text-white/60 border border-white/20 focus:bg-white/25 focus:border-white/40 focus:outline-none transition-colors">
          <button type="submit" class="px-3 py-2 rounded-r text-sm font-medium bg-white/20 border border-l-0 border-white/20 hover:bg-white/30 transition-colors">Search</button>
        </form>
        <a href="/all" class="block py-2 px-2 rounded hover:bg-white/10 transition-colors"{all_aria}>All Packages</a>
        <a href="/about" class="block py-2 px-2 rounded hover:bg-white/10 transition-colors"{about_aria}>About</a>
      </div>
    </details>
  </nav>
</header>"#,
    )
}
