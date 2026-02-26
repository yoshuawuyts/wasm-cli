use wasmparser::{Encoding, Parser, Payload};

/// Determine whether raw wasm bytes represent a WIT package (interface-only)
/// rather than a compiled component.
///
/// A WIT package is a WebAssembly component that contains only types, imports,
/// exports, and custom sections. A compiled component additionally contains
/// code/instantiation sections such as `ModuleSection`, `ComponentSection`,
/// `InstanceSection`, etc. Core modules are never WIT packages.
///
/// Returns `true` if the bytes are a WIT package, `false` if they contain
/// code/instantiation (a real component), are a core module, or if parsing
/// fails.
#[must_use]
pub fn is_wit_package(bytes: &[u8]) -> bool {
    let parser = Parser::new(0);
    for payload in parser.parse_all(bytes) {
        match payload {
            Ok(Payload::Version { encoding, .. }) => {
                if encoding != Encoding::Component {
                    // Core modules are not WIT packages
                    return false;
                }
            }
            Ok(Payload::ModuleSection { .. })
            | Ok(Payload::ComponentSection { .. })
            | Ok(Payload::InstanceSection(_))
            | Ok(Payload::ComponentInstanceSection(_))
            | Ok(Payload::ComponentCanonicalSection(_))
            | Ok(Payload::CoreTypeSection(_))
            | Ok(Payload::ComponentStartSection { .. }) => {
                // Contains code/instantiation — it's a real component
                return false;
            }
            Err(_) => return false,
            _ => {}
        }
    }
    // Only had types, imports, exports, custom sections — WIT package
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_bytes_are_not_wit_package() {
        assert!(!is_wit_package(b"not a wasm component"));
    }

    #[test]
    fn empty_bytes_are_not_wit_package() {
        assert!(!is_wit_package(&[]));
    }

    #[test]
    fn core_module_is_not_wit_package() {
        // A core WebAssembly module is not a WIT package — only components can be.
        let core_module = [
            0x00, 0x61, 0x73, 0x6d, // \0asm magic
            0x01, 0x00, 0x00, 0x00, // version 1
        ];
        assert!(!is_wit_package(&core_module));
    }
}
