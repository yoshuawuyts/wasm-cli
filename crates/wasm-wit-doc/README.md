# wasm-wit-doc

Parse WIT text into a rich, serializable document model for documentation
rendering. Produces a `WitDocument` with interfaces, types, functions, worlds,
doc comments, and pre-resolved cross-links.

## Usage

```rust
use std::collections::HashMap;
use wasm_wit_doc::parse_wit_doc;

let wit_text = r#"
package example:my-pkg@1.0.0;

interface types {
    /// A greeting message.
    record greeting {
        message: string,
    }
}
"#;

let dep_urls = HashMap::new();
let doc = parse_wit_doc(wit_text, "/example/my-pkg/1.0.0", &dep_urls).unwrap();
assert_eq!(doc.interfaces.len(), 1);
```
