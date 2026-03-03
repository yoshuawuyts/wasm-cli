# wasm-detector

A library to detect local `.wasm` files in a repository.

## Features

- Detects `.wasm` files in a directory
- Respects `.gitignore` rules
- Includes well-known .wasm locations that are typically ignored:
  - `target/wasm32-*/**/*.wasm` (Rust wasm targets)
  - `pkg/**/*.wasm` (wasm-pack output)
  - `dist/**/*.wasm` (JavaScript/jco output)

## Usage

```rust
use wasm_detector::WasmDetector;
use std::path::Path;

let detector = WasmDetector::new(Path::new("."));
for result in detector {
    match result {
        Ok(entry) => println!("Found: {}", entry.path().display()),
        Err(e) => eprintln!("Error: {}", e),
    }
}
```

## Well-known Locations

The detector automatically includes these typically-ignored directories:

| Location          | Description                  |
|-------------------|------------------------------|
| `target/wasm32-*` | Rust wasm32 target outputs   |
| `pkg/`            | wasm-pack output directory   |
| `dist/`           | JavaScript/jco build output  |

## License

Licensed under Apache License, Version 2.0, with LLVM Exceptions.
