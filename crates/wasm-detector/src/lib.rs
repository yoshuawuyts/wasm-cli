//! A library to detect local `.wasm` files in a repository.
//!
//! This crate provides functionality to find WebAssembly files while:
//! - Respecting `.gitignore` rules
//! - Including well-known `.wasm` locations that are typically ignored
//!   (e.g., `target/wasm32-*`, `pkg/`, `dist/`)
//!
//! # Example
//!
//! ```no_run
//! use wasm_detector::WasmDetector;
//! use std::path::Path;
//!
//! let detector = WasmDetector::new(Path::new("."));
//! for result in detector {
//!     match result {
//!         Ok(entry) => println!("Found: {}", entry.path().display()),
//!         Err(e) => eprintln!("Error: {}", e),
//!     }
//! }
//! ```

use ignore::WalkBuilder;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Well-known directories that typically contain `.wasm` files but are often ignored.
///
/// These directories are scanned separately without respecting `.gitignore` rules
/// to ensure important wasm output locations are always included.
///
/// # Example
///
/// ```
/// use wasm_detector::WELL_KNOWN_WASM_DIRS;
///
/// for dir in WELL_KNOWN_WASM_DIRS {
///     println!("Scanning well-known dir: {dir}");
/// }
/// assert!(WELL_KNOWN_WASM_DIRS.contains(&"target"));
/// ```
pub const WELL_KNOWN_WASM_DIRS: &[&str] = &[
    // Rust wasm targets (the target directory is scanned for wasm32-* subdirs)
    "target", // wasm-pack output
    "pkg",    // JavaScript/jco output
    "dist",
];

/// Patterns to match within the target directory for wasm-specific subdirectories.
const TARGET_WASM_PREFIXES: &[&str] = &["wasm32-"];

/// A discovered WebAssembly file entry.
///
/// # Example
///
/// ```
/// use wasm_detector::WasmEntry;
/// use std::path::PathBuf;
///
/// let entry = WasmEntry::new(PathBuf::from("pkg/hello.wasm"));
/// assert_eq!(entry.file_name(), Some("hello.wasm"));
/// ```
#[derive(Debug, Clone)]
pub struct WasmEntry {
    path: PathBuf,
}

impl WasmEntry {
    /// Create a new WasmEntry from a path.
    ///
    /// # Example
    ///
    /// ```
    /// use wasm_detector::WasmEntry;
    /// use std::path::PathBuf;
    ///
    /// let entry = WasmEntry::new(PathBuf::from("dist/app.wasm"));
    /// assert_eq!(entry.path().to_str(), Some("dist/app.wasm"));
    /// ```
    #[must_use]
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// Returns the path to the `.wasm` file.
    ///
    /// # Example
    ///
    /// ```
    /// use wasm_detector::WasmEntry;
    /// use std::path::PathBuf;
    ///
    /// let entry = WasmEntry::new(PathBuf::from("pkg/module.wasm"));
    /// assert_eq!(entry.path(), PathBuf::from("pkg/module.wasm"));
    /// ```
    // r[impl detector.entry-methods]
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns the file name of the `.wasm` file.
    ///
    /// # Example
    ///
    /// ```
    /// use wasm_detector::WasmEntry;
    /// use std::path::PathBuf;
    ///
    /// let entry = WasmEntry::new(PathBuf::from("dist/app.wasm"));
    /// assert_eq!(entry.file_name(), Some("app.wasm"));
    /// ```
    #[must_use]
    pub fn file_name(&self) -> Option<&str> {
        self.path.file_name().and_then(|s| s.to_str())
    }

    /// Consumes the entry and returns the underlying path.
    ///
    /// # Example
    ///
    /// ```
    /// use wasm_detector::WasmEntry;
    /// use std::path::PathBuf;
    ///
    /// let entry = WasmEntry::new(PathBuf::from("pkg/lib.wasm"));
    /// let path: PathBuf = entry.into_path();
    /// assert_eq!(path, PathBuf::from("pkg/lib.wasm"));
    /// ```
    #[must_use]
    pub fn into_path(self) -> PathBuf {
        self.path
    }
}

/// A detector that finds `.wasm` files in a directory tree.
///
/// The detector:
/// - Respects `.gitignore` rules by default
/// - Automatically includes well-known `.wasm` locations that are typically ignored
/// - Returns an iterator over discovered `.wasm` files
///
/// # Example
///
/// ```no_run
/// use wasm_detector::WasmDetector;
/// use std::path::Path;
///
/// let detector = WasmDetector::new(Path::new("."));
/// let wasm_files: Vec<_> = detector.into_iter().filter_map(Result::ok).collect();
/// println!("Found {} wasm files", wasm_files.len());
/// ```
#[derive(Debug, Clone)]
pub struct WasmDetector {
    root: PathBuf,
    include_hidden: bool,
    follow_symlinks: bool,
}

impl WasmDetector {
    /// Create a new detector that will search from the given root directory.
    #[must_use]
    pub fn new(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
            include_hidden: false,
            follow_symlinks: false,
        }
    }

    /// Set whether to include hidden files and directories.
    ///
    /// By default, hidden files are excluded.
    #[must_use]
    pub fn include_hidden(mut self, include: bool) -> Self {
        self.include_hidden = include;
        self
    }

    /// Set whether to follow symbolic links.
    ///
    /// By default, symbolic links are not followed.
    #[must_use]
    pub fn follow_symlinks(mut self, follow: bool) -> Self {
        self.follow_symlinks = follow;
        self
    }

    /// Detect `.wasm` files and return all results as a vector.
    ///
    /// This is a convenience method that collects all results.
    /// For large directories, consider using the iterator interface instead.
    ///
    /// # Errors
    ///
    /// Returns an error if the detection fails to complete.
    // r[impl detector.convenience]
    pub fn detect(&self) -> Result<Vec<WasmEntry>, ignore::Error> {
        self.iter().collect()
    }

    /// Returns an iterator over discovered `.wasm` files.
    #[must_use]
    pub fn iter(&self) -> WasmDetectorIter {
        WasmDetectorIter::new(self)
    }

    /// Find all well-known wasm directories that exist in the root.
    // r[impl detector.target-dir]
    // r[impl detector.pkg-dir]
    // r[impl detector.dist-dir]
    fn find_well_known_dirs(&self) -> Vec<PathBuf> {
        let mut dirs = Vec::new();

        // Check for pkg/ and dist/ directories
        for dir_name in &["pkg", "dist"] {
            let dir_path = self.root.join(dir_name);
            if dir_path.is_dir() {
                dirs.push(dir_path);
            }
        }

        // Check for target/wasm32-* directories
        let target_dir = self.root.join("target");
        if target_dir.is_dir()
            && let Ok(entries) = std::fs::read_dir(&target_dir)
        {
            for entry in entries.filter_map(Result::ok) {
                let path = entry.path();
                if path.is_dir()
                    && let Some(name) = path.file_name().and_then(|n| n.to_str())
                {
                    for prefix in TARGET_WASM_PREFIXES {
                        if name.starts_with(prefix) {
                            dirs.push(path);
                            break;
                        }
                    }
                }
            }
        }

        dirs
    }
}

impl IntoIterator for WasmDetector {
    type Item = Result<WasmEntry, ignore::Error>;
    type IntoIter = WasmDetectorIter;

    fn into_iter(self) -> Self::IntoIter {
        WasmDetectorIter::new(&self)
    }
}

impl IntoIterator for &WasmDetector {
    type Item = Result<WasmEntry, ignore::Error>;
    type IntoIter = WasmDetectorIter;

    fn into_iter(self) -> Self::IntoIter {
        WasmDetectorIter::new(self)
    }
}

/// Iterator over discovered `.wasm` files.
///
/// This iterator combines results from multiple walks:
/// 1. A main walk that respects `.gitignore`
/// 2. Additional walks for well-known directories (ignoring `.gitignore`)
///
/// # Example
///
/// ```no_run
/// use wasm_detector::WasmDetector;
/// use std::path::Path;
///
/// let detector = WasmDetector::new(Path::new("."));
/// let mut iter = detector.iter();
/// while let Some(result) = iter.next() {
///     if let Ok(entry) = result {
///         println!("Found: {}", entry.path().display());
///     }
/// }
/// ```
pub struct WasmDetectorIter {
    /// The main walker that respects gitignore
    main_walker: ignore::Walk,
    /// Walkers for well-known directories (ignoring gitignore)
    well_known_walkers: Vec<ignore::Walk>,
    /// Current index in well_known_walkers
    current_well_known_idx: usize,
    /// Set of paths already seen (to avoid duplicates)
    seen_paths: HashSet<PathBuf>,
    /// Whether we've finished the main walk
    main_walk_done: bool,
}

impl std::fmt::Debug for WasmDetectorIter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WasmDetectorIter")
            .field("main_walk_done", &self.main_walk_done)
            .field("current_well_known_idx", &self.current_well_known_idx)
            .field("seen_paths_count", &self.seen_paths.len())
            .finish_non_exhaustive()
    }
}

// r[impl detector.find-wasm]
// r[impl detector.gitignore]
// r[impl detector.empty-dir]
impl WasmDetectorIter {
    fn new(detector: &WasmDetector) -> Self {
        // Build the main walker that respects gitignore
        let main_walker = WalkBuilder::new(&detector.root)
            .hidden(!detector.include_hidden)
            .follow_links(detector.follow_symlinks)
            .git_ignore(true)
            .git_global(true)
            .git_exclude(true)
            .build();

        // Build walkers for well-known directories (ignoring gitignore)
        let well_known_dirs = detector.find_well_known_dirs();
        let well_known_walkers: Vec<_> = well_known_dirs
            .into_iter()
            .map(|dir| {
                WalkBuilder::new(dir)
                    .hidden(!detector.include_hidden)
                    .follow_links(detector.follow_symlinks)
                    .git_ignore(false) // Don't respect gitignore for well-known dirs
                    .git_global(false)
                    .git_exclude(false)
                    .build()
            })
            .collect();

        Self {
            main_walker,
            well_known_walkers,
            current_well_known_idx: 0,
            seen_paths: HashSet::new(),
            main_walk_done: false,
        }
    }

    /// Try to get the next .wasm file from the main walker
    fn next_from_main(&mut self) -> Option<Result<WasmEntry, ignore::Error>> {
        loop {
            match self.main_walker.next() {
                Some(Ok(entry)) => {
                    let path = entry.path();
                    if path.is_file() && path.extension().is_some_and(|ext| ext == "wasm") {
                        let path_buf = path.to_path_buf();
                        self.seen_paths.insert(path_buf.clone());
                        return Some(Ok(WasmEntry::new(path_buf)));
                    }
                    // Continue to next entry
                }
                Some(Err(e)) => return Some(Err(e)),
                None => {
                    self.main_walk_done = true;
                    return None;
                }
            }
        }
    }

    /// Try to get the next .wasm file from well-known walkers
    fn next_from_well_known(&mut self) -> Option<Result<WasmEntry, ignore::Error>> {
        while self.current_well_known_idx < self.well_known_walkers.len() {
            let Some(walker) = self.well_known_walkers.get_mut(self.current_well_known_idx) else {
                self.current_well_known_idx += 1;
                continue;
            };
            for entry in walker {
                match entry {
                    Ok(entry) => {
                        let path = entry.path();
                        if !path.is_file() || path.extension().is_none_or(|ext| ext != "wasm") {
                            continue;
                        }
                        let path_buf = path.to_path_buf();
                        if self.seen_paths.contains(&path_buf) {
                            continue;
                        }
                        self.seen_paths.insert(path_buf.clone());
                        return Some(Ok(WasmEntry::new(path_buf)));
                    }
                    Err(e) => return Some(Err(e)),
                }
            }
            self.current_well_known_idx += 1;
        }
        None
    }
}

impl Iterator for WasmDetectorIter {
    type Item = Result<WasmEntry, ignore::Error>;

    fn next(&mut self) -> Option<Self::Item> {
        // First, exhaust the main walker
        if !self.main_walk_done
            && let Some(result) = self.next_from_main()
        {
            return Some(result);
        }

        // Then, go through well-known directories
        self.next_from_well_known()
    }
}
