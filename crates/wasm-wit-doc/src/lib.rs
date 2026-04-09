//! Parse WIT text into a rich, serializable document model.
//!
//! This crate converts WIT source text into a [`WitDocument`] that captures
//! every interface, type, function, world, and doc comment — with
//! pre-resolved URLs for cross-linking. It is designed to be reusable by any
//! consumer: a server-side rendered frontend, a CLI `wasm doc` tool, or a
//! static-site generator.
//!
//! # Example
//!
//! ```
//! use std::collections::HashMap;
//! use wasm_wit_doc::parse_wit_doc;
//!
//! let wit = r#"
//! package example:greeter@1.0.0;
//!
//! interface greet {
//!     /// Say hello.
//!     hello: func(name: string) -> string;
//! }
//! "#;
//!
//! let doc = parse_wit_doc(wit, "/example/greeter/1.0.0", &HashMap::new())
//!     .expect("valid WIT");
//! assert_eq!(doc.package_name, "example:greeter");
//! assert_eq!(doc.interfaces.len(), 1);
//! assert_eq!(doc.interfaces[0].functions.len(), 1);
//! ```

mod convert;
pub mod types;

pub use types::*;

use std::collections::HashMap;
use std::hash::BuildHasher;

/// Parse WIT source text into a [`WitDocument`].
///
/// # Arguments
///
/// * `wit_text` — WIT source (text form, not binary).
/// * `url_base` — base URL path for this package (e.g.
///   `"/wasi/http/0.2.11"`). All generated URLs are rooted here.
/// * `dep_urls` — maps dependency package names (e.g. `"wasi:io"`) to their
///   URL base (e.g. `"/wasi/io/0.2.2"`), enabling cross-package links.
///
/// # Errors
///
/// Returns an error if the WIT text fails to parse.
pub fn parse_wit_doc<S: BuildHasher>(
    wit_text: &str,
    url_base: &str,
    dep_urls: &HashMap<String, String, S>,
) -> anyhow::Result<WitDocument> {
    // Build a standard HashMap for the internal converter.
    let standard: HashMap<String, String> = dep_urls
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    convert::convert(wit_text, url_base, &standard)
}
