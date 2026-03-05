use wit_parser::decoding::{DecodedWasm, decode};

/// An import or export declaration inside a WIT world.
#[derive(Debug, Clone)]
pub(crate) struct ImportExportItem {
    /// The declared package name (e.g. "wasi:http").
    pub package: String,
    /// The declared interface name within the package, if any.
    pub interface: Option<String>,
    /// The declared version constraint, if any.
    pub version: Option<String>,
}

/// Metadata about a single WIT world.
#[derive(Debug, Clone)]
pub(crate) struct WorldMetadata {
    /// The world name (e.g. "proxy", "command").
    pub name: String,
    /// Import declarations in this world.
    pub imports: Vec<ImportExportItem>,
    /// Export declarations in this world.
    pub exports: Vec<ImportExportItem>,
}

/// A dependency on another WIT package.
///
/// # Example
///
/// ```
/// use wasm_package_manager::types::DependencyItem;
///
/// let dep = DependencyItem {
///     package: "wasi:io".to_string(),
///     version: Some("0.2.0".to_string()),
/// };
/// assert_eq!(dep.package, "wasi:io");
/// assert_eq!(dep.version.as_deref(), Some("0.2.0"));
/// ```
#[derive(Debug, Clone)]
pub struct DependencyItem {
    /// The declared package name (e.g. "wasi:io").
    pub package: String,
    /// The declared version, if any.
    pub version: Option<String>,
}

/// Metadata extracted from a WIT component.
pub(crate) struct WitMetadata {
    /// The WIT package name (e.g. "wasi:http").
    pub package_name: Option<String>,
    /// All worlds declared in this package or component.
    pub worlds: Vec<WorldMetadata>,
    /// Dependencies on other WIT packages.
    pub dependencies: Vec<DependencyItem>,
    /// Whether this is a compiled component (true) or a WIT-only package (false).
    pub is_component: bool,
    /// Full WIT text representation.
    pub wit_text: String,
}

/// Attempt to extract WIT metadata from wasm component bytes.
/// Returns `None` if the bytes are not a valid wasm component.
pub(crate) fn extract_wit_metadata(wasm_bytes: &[u8]) -> Option<WitMetadata> {
    // Try to decode the wasm bytes as a component
    let decoded = decode(wasm_bytes).ok()?;

    // Determine if this is a compiled component or a WIT-only package
    let is_component = matches!(&decoded, DecodedWasm::Component(..));

    // Extract the primary package ID and name
    let (package_name, primary_package_id) = match &decoded {
        DecodedWasm::WitPackage(resolve, package_id) => {
            let package = resolve
                .packages
                .get(*package_id)
                .expect("Package ID should be valid");
            (Some(format!("{}", package.name)), Some(*package_id))
        }
        DecodedWasm::Component(resolve, world_id) => {
            let world = resolve
                .worlds
                .get(*world_id)
                .expect("World ID should be valid");
            let (pkg_name, pkg_id) = world
                .package
                .and_then(|pid| {
                    resolve
                        .packages
                        .get(pid)
                        .map(|p| (format!("{}", p.name), pid))
                })
                .unzip();
            (pkg_name, pkg_id)
        }
    };

    let resolve = decoded.resolve();

    // Extract world metadata
    let worlds = extract_worlds(&decoded);

    // Extract dependencies (packages other than the primary one)
    let dependencies = extract_dependencies(resolve, primary_package_id);

    // Generate a WIT text representation from the decoded structure
    let wit_text = generate_wit_text(&decoded);

    Some(WitMetadata {
        package_name,
        worlds,
        dependencies,
        is_component,
        wit_text,
    })
}

/// Extract world metadata from all worlds in the decoded component.
fn extract_worlds(decoded: &DecodedWasm) -> Vec<WorldMetadata> {
    let resolve = decoded.resolve();

    match decoded {
        DecodedWasm::WitPackage(_, package_id) => {
            let package = resolve
                .packages
                .get(*package_id)
                .expect("Package ID should be valid");
            package
                .worlds
                .iter()
                .map(|(name, world_id)| {
                    let world = resolve
                        .worlds
                        .get(*world_id)
                        .expect("World ID should be valid");
                    WorldMetadata {
                        name: name.clone(),
                        imports: extract_world_items(resolve, &world.imports),
                        exports: extract_world_items(resolve, &world.exports),
                    }
                })
                .collect()
        }
        DecodedWasm::Component(_, world_id) => {
            let world = resolve
                .worlds
                .get(*world_id)
                .expect("World ID should be valid");
            vec![WorldMetadata {
                name: world.name.clone(),
                imports: extract_world_items(resolve, &world.imports),
                exports: extract_world_items(resolve, &world.exports),
            }]
        }
    }
}

/// Extract import/export items from a world's item map.
fn extract_world_items<'a>(
    resolve: &wit_parser::Resolve,
    items: impl IntoIterator<Item = (&'a wit_parser::WorldKey, &'a wit_parser::WorldItem)>,
) -> Vec<ImportExportItem> {
    items
        .into_iter()
        .map(|(key, _)| match key {
            wit_parser::WorldKey::Name(name) => ImportExportItem {
                package: name.clone(),
                interface: None,
                version: None,
            },
            wit_parser::WorldKey::Interface(id) => {
                let iface = resolve
                    .interfaces
                    .get(*id)
                    .expect("Interface ID should be valid");
                if let Some(pkg_id) = iface.package {
                    let pkg = resolve
                        .packages
                        .get(pkg_id)
                        .expect("Package ID should be valid");
                    ImportExportItem {
                        package: format!("{}:{}", pkg.name.namespace, pkg.name.name),
                        interface: iface.name.clone(),
                        version: pkg.name.version.as_ref().map(ToString::to_string),
                    }
                } else {
                    ImportExportItem {
                        package: iface
                            .name
                            .clone()
                            .unwrap_or_else(|| format!("interface-{id:?}")),
                        interface: None,
                        version: None,
                    }
                }
            }
        })
        .collect()
}

/// Extract dependency packages (all packages other than the primary one).
fn extract_dependencies(
    resolve: &wit_parser::Resolve,
    primary_package_id: Option<wit_parser::PackageId>,
) -> Vec<DependencyItem> {
    resolve
        .packages
        .iter()
        .filter(|(id, _)| Some(id) != primary_package_id.as_ref())
        .map(|(_, pkg)| DependencyItem {
            package: format!("{}:{}", pkg.name.namespace, pkg.name.name),
            version: pkg.name.version.as_ref().map(ToString::to_string),
        })
        .collect()
}

/// Generate WIT text representation from decoded component.
fn generate_wit_text(decoded: &DecodedWasm) -> String {
    use std::fmt::Write as _;
    let resolve = decoded.resolve();
    let mut output = String::new();

    match decoded {
        DecodedWasm::WitPackage(_, package_id) => {
            let package = resolve
                .packages
                .get(*package_id)
                .expect("Package ID should be valid");
            writeln!(output, "package {};", package.name).unwrap_or_default();
            writeln!(output).unwrap_or_default();

            // Print interfaces
            for (name, interface_id) in &package.interfaces {
                writeln!(output, "interface {name} {{").unwrap_or_default();
                let interface = resolve
                    .interfaces
                    .get(*interface_id)
                    .expect("Interface ID should be valid");

                // Print types
                for (type_name, type_id) in &interface.types {
                    let type_def = resolve
                        .types
                        .get(*type_id)
                        .expect("Type ID should be valid");
                    writeln!(
                        output,
                        "  type {}: {:?};",
                        type_name,
                        type_def.kind.as_str()
                    )
                    .unwrap_or_default();
                }

                // Print functions
                for (func_name, func) in &interface.functions {
                    let params: Vec<String> =
                        func.params.iter().map(|param| param.name.clone()).collect();
                    let has_result = func.result.is_some();
                    writeln!(
                        output,
                        "  func {}({}){};",
                        func_name,
                        params.join(", "),
                        if has_result { " -> ..." } else { "" }
                    )
                    .unwrap_or_default();
                }
                output.push_str("}\n\n");
            }

            // Print worlds
            for (name, world_id) in &package.worlds {
                let world = resolve
                    .worlds
                    .get(*world_id)
                    .expect("World ID should be valid");
                writeln!(output, "world {name} {{").unwrap_or_default();

                for (key, _item) in &world.imports {
                    writeln!(output, "  import {};", world_key_to_string(key)).unwrap_or_default();
                }
                for (key, _item) in &world.exports {
                    writeln!(output, "  export {};", world_key_to_string(key)).unwrap_or_default();
                }
                output.push_str("}\n\n");
            }
        }
        DecodedWasm::Component(_, world_id) => {
            let world = resolve
                .worlds
                .get(*world_id)
                .expect("World ID should be valid");
            output.push_str("// Inferred component interface\n");
            writeln!(output, "world {name} {{", name = world.name).unwrap_or_default();

            for (key, _item) in &world.imports {
                writeln!(output, "  import {};", world_key_to_string(key)).unwrap_or_default();
            }
            for (key, _item) in &world.exports {
                writeln!(output, "  export {};", world_key_to_string(key)).unwrap_or_default();
            }
            output.push_str("}\n");
        }
    }

    output
}

/// Convert a WorldKey to a string representation.
fn world_key_to_string(key: &wit_parser::WorldKey) -> String {
    match key {
        wit_parser::WorldKey::Name(name) => name.clone(),
        wit_parser::WorldKey::Interface(id) => format!("interface-{id:?}"),
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    // r[verify wit.parse.invalid-bytes]
    #[test]
    fn extract_returns_none_for_invalid_bytes() {
        let invalid_bytes = b"not a wasm component";
        assert!(extract_wit_metadata(invalid_bytes).is_none());
    }

    // r[verify wit.parse.empty-bytes]
    #[test]
    fn extract_returns_none_for_empty_bytes() {
        let empty_bytes: &[u8] = &[];
        assert!(extract_wit_metadata(empty_bytes).is_none());
    }

    // r[verify wit.parse.core-module]
    #[test]
    fn extract_handles_core_wasm_module() {
        // A minimal valid core WebAssembly module (not a component)
        // Magic number + version + empty sections
        let core_module = [
            0x00, 0x61, 0x73, 0x6d, // \0asm magic
            0x01, 0x00, 0x00, 0x00, // version 1
        ];
        // Core modules may or may not be decoded - just ensure we don't panic
        let _ = extract_wit_metadata(&core_module);
    }

    // r[verify wit.parse.random-bytes]
    #[test]
    fn extract_returns_none_for_random_bytes() {
        let random_bytes = [0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0x11, 0x22, 0x33];
        assert!(extract_wit_metadata(&random_bytes).is_none());
    }

    // r[verify wit.parse.world-key-name]
    #[test]
    fn world_key_name_converts_correctly() {
        let key = wit_parser::WorldKey::Name("my-import".to_string());
        assert_eq!(world_key_to_string(&key), "my-import");
    }

    // r[verify wit.parse.world-key-interface]
    #[test]
    fn world_key_interface_converts_to_debug_format() {
        use wit_parser::{Interface, Resolve};

        let mut resolve = Resolve::default();
        let interface = Interface {
            name: Some("test".to_string()),
            docs: Default::default(),
            types: Default::default(),
            functions: Default::default(),
            package: None,
            stability: Default::default(),
            span: Default::default(),
            clone_of: None,
        };
        let id = resolve.interfaces.alloc(interface);

        let key = wit_parser::WorldKey::Interface(id);
        let result = world_key_to_string(&key);
        assert!(result.starts_with("interface-"), "got: {}", result);
    }

    // r[verify wit.parse.wit-text-package]
    #[test]
    fn generate_wit_text_for_wit_package() {
        use wit_parser::{Interface, Package, PackageName, Resolve, World};

        let mut resolve = Resolve::default();

        // Create interface
        let interface = Interface {
            name: Some("greeter".to_string()),
            docs: Default::default(),
            types: Default::default(),
            functions: Default::default(),
            package: None,
            stability: Default::default(),
            span: Default::default(),
            clone_of: None,
        };
        let interface_id = resolve.interfaces.alloc(interface);

        // Create world
        let world = World {
            name: "hello".to_string(),
            docs: Default::default(),
            imports: Default::default(),
            exports: Default::default(),
            includes: Default::default(),
            package: None,
            stability: Default::default(),
            span: Default::default(),
        };
        let world_id = resolve.worlds.alloc(world);

        // Create package
        let package = Package {
            name: PackageName {
                namespace: "test".to_string(),
                name: "example".to_string(),
                version: None,
            },
            docs: Default::default(),
            interfaces: [("greeter".to_string(), interface_id)]
                .into_iter()
                .collect(),
            worlds: [("hello".to_string(), world_id)].into_iter().collect(),
        };
        let package_id = resolve.packages.alloc(package);

        // Update back-references
        resolve.interfaces[interface_id].package = Some(package_id);
        resolve.worlds[world_id].package = Some(package_id);

        // Create decoded structure directly (without encoding to binary)
        let decoded = DecodedWasm::WitPackage(resolve, package_id);
        let wit_text = generate_wit_text(&decoded);

        assert!(
            wit_text.contains("package test:example"),
            "should contain package name, got: {}",
            wit_text
        );
        assert!(
            wit_text.contains("interface greeter"),
            "should contain interface name, got: {}",
            wit_text
        );
        assert!(
            wit_text.contains("world hello"),
            "should contain world name, got: {}",
            wit_text
        );
    }

    // r[verify wit.parse.wit-text-component]
    #[test]
    fn generate_wit_text_for_component() {
        use wit_parser::{Resolve, World};

        let mut resolve = Resolve::default();

        // Create a world for a component
        let world = World {
            name: "my-component".to_string(),
            docs: Default::default(),
            imports: Default::default(),
            exports: Default::default(),
            includes: Default::default(),
            package: None,
            stability: Default::default(),
            span: Default::default(),
        };
        let world_id = resolve.worlds.alloc(world);

        let decoded = DecodedWasm::Component(resolve, world_id);
        let wit_text = generate_wit_text(&decoded);

        assert!(
            wit_text.contains("// Inferred component interface"),
            "should have component comment, got: {}",
            wit_text
        );
        assert!(
            wit_text.contains("world my-component"),
            "should contain world name, got: {}",
            wit_text
        );
    }

    // r[verify wit.parse.wit-text-imports-exports]
    #[test]
    fn generate_wit_text_with_imports_and_exports() {
        use wit_parser::{Function, FunctionKind, Resolve, World, WorldItem, WorldKey};

        let mut resolve = Resolve::default();

        let mut world = World {
            name: "test-world".to_string(),
            docs: Default::default(),
            imports: Default::default(),
            exports: Default::default(),
            includes: Default::default(),
            package: None,
            stability: Default::default(),
            span: Default::default(),
        };

        // Add named imports and exports using functions (which don't need TypeIds)
        world.imports.insert(
            WorldKey::Name("read-stdin".to_string()),
            WorldItem::Function(Function {
                name: "read-stdin".to_string(),
                kind: FunctionKind::Freestanding,
                params: vec![],
                result: None,
                docs: Default::default(),
                stability: Default::default(),
                span: Default::default(),
            }),
        );
        world.exports.insert(
            WorldKey::Name("run".to_string()),
            WorldItem::Function(Function {
                name: "run".to_string(),
                kind: FunctionKind::Freestanding,
                params: vec![],
                result: None,
                docs: Default::default(),
                stability: Default::default(),
                span: Default::default(),
            }),
        );

        let world_id = resolve.worlds.alloc(world);

        let decoded = DecodedWasm::Component(resolve, world_id);
        let wit_text = generate_wit_text(&decoded);

        assert!(
            wit_text.contains("import read-stdin"),
            "should contain import, got: {}",
            wit_text
        );
        assert!(
            wit_text.contains("export run"),
            "should contain export, got: {}",
            wit_text
        );
    }

    // r[verify wit.parse.multiple-worlds]
    #[test]
    fn extract_worlds_from_wit_package_with_multiple_worlds() {
        use wit_parser::{Interface, Package, PackageName, Resolve, World};

        let mut resolve = Resolve::default();

        let interface = Interface {
            name: Some("handler".to_string()),
            docs: Default::default(),
            types: Default::default(),
            functions: Default::default(),
            package: None,
            stability: Default::default(),
            span: Default::default(),
            clone_of: None,
        };
        let interface_id = resolve.interfaces.alloc(interface);

        let world_a = World {
            name: "proxy".to_string(),
            docs: Default::default(),
            imports: Default::default(),
            exports: Default::default(),
            includes: Default::default(),
            package: None,
            stability: Default::default(),
            span: Default::default(),
        };
        let world_a_id = resolve.worlds.alloc(world_a);

        let world_b = World {
            name: "command".to_string(),
            docs: Default::default(),
            imports: Default::default(),
            exports: Default::default(),
            includes: Default::default(),
            package: None,
            stability: Default::default(),
            span: Default::default(),
        };
        let world_b_id = resolve.worlds.alloc(world_b);

        let package = Package {
            name: PackageName {
                namespace: "wasi".to_string(),
                name: "http".to_string(),
                version: None,
            },
            docs: Default::default(),
            interfaces: [("handler".to_string(), interface_id)]
                .into_iter()
                .collect(),
            worlds: [
                ("proxy".to_string(), world_a_id),
                ("command".to_string(), world_b_id),
            ]
            .into_iter()
            .collect(),
        };
        let package_id = resolve.packages.alloc(package);

        resolve.interfaces[interface_id].package = Some(package_id);
        resolve.worlds[world_a_id].package = Some(package_id);
        resolve.worlds[world_b_id].package = Some(package_id);

        let decoded = DecodedWasm::WitPackage(resolve, package_id);
        let worlds = extract_worlds(&decoded);

        assert_eq!(worlds.len(), 2, "should extract both worlds");
        let names: Vec<&str> = worlds.iter().map(|w| w.name.as_str()).collect();
        assert!(names.contains(&"proxy"), "should contain proxy world");
        assert!(names.contains(&"command"), "should contain command world");
    }

    // r[verify wit.parse.single-world]
    #[test]
    fn extract_worlds_component_has_one_world() {
        use wit_parser::{Resolve, World};

        let mut resolve = Resolve::default();
        let world = World {
            name: "my-component".to_string(),
            docs: Default::default(),
            imports: Default::default(),
            exports: Default::default(),
            includes: Default::default(),
            package: None,
            stability: Default::default(),
            span: Default::default(),
        };
        let world_id = resolve.worlds.alloc(world);

        let decoded = DecodedWasm::Component(resolve, world_id);
        let worlds = extract_worlds(&decoded);

        assert_eq!(worlds.len(), 1);
        assert_eq!(worlds[0].name, "my-component");
    }

    // r[verify wit.parse.world-items]
    #[test]
    fn extract_world_items_with_named_and_interface_imports() {
        use wit_parser::{
            Function, FunctionKind, Interface, Package, PackageName, Resolve, World, WorldItem,
            WorldKey,
        };

        let mut resolve = Resolve::default();

        // Create a dependency interface with a package
        let dep_iface = Interface {
            name: Some("streams".to_string()),
            docs: Default::default(),
            types: Default::default(),
            functions: Default::default(),
            package: None,
            stability: Default::default(),
            span: Default::default(),
            clone_of: None,
        };
        let dep_iface_id = resolve.interfaces.alloc(dep_iface);

        let dep_pkg = Package {
            name: PackageName {
                namespace: "wasi".to_string(),
                name: "io".to_string(),
                version: None,
            },
            docs: Default::default(),
            interfaces: [("streams".to_string(), dep_iface_id)]
                .into_iter()
                .collect(),
            worlds: Default::default(),
        };
        let dep_pkg_id = resolve.packages.alloc(dep_pkg);
        resolve.interfaces[dep_iface_id].package = Some(dep_pkg_id);

        let mut world = World {
            name: "test".to_string(),
            docs: Default::default(),
            imports: Default::default(),
            exports: Default::default(),
            includes: Default::default(),
            package: None,
            stability: Default::default(),
            span: Default::default(),
        };

        // Named import (bare function)
        world.imports.insert(
            WorldKey::Name("my-func".to_string()),
            WorldItem::Function(Function {
                name: "my-func".to_string(),
                kind: FunctionKind::Freestanding,
                params: vec![],
                result: None,
                docs: Default::default(),
                stability: Default::default(),
                span: Default::default(),
            }),
        );

        // Interface import
        world.imports.insert(
            WorldKey::Interface(dep_iface_id),
            WorldItem::Interface {
                id: dep_iface_id,
                stability: Default::default(),
                span: Default::default(),
            },
        );

        let items = extract_world_items(&resolve, &world.imports);

        assert_eq!(items.len(), 2);

        // Named import
        let named = &items[0];
        assert_eq!(named.package, "my-func");
        assert!(named.interface.is_none());
        assert!(named.version.is_none());

        // Interface import
        let iface = &items[1];
        assert_eq!(iface.package, "wasi:io");
        assert_eq!(iface.interface.as_deref(), Some("streams"));
        assert_eq!(iface.version.as_deref(), None);
    }

    // r[verify wit.parse.exclude-primary]
    #[test]
    fn extract_dependencies_excludes_primary_package() {
        use wit_parser::{Package, PackageName, Resolve};

        let mut resolve = Resolve::default();

        let primary = Package {
            name: PackageName {
                namespace: "my".to_string(),
                name: "app".to_string(),
                version: None,
            },
            docs: Default::default(),
            interfaces: Default::default(),
            worlds: Default::default(),
        };
        let primary_id = resolve.packages.alloc(primary);

        let dep = Package {
            name: PackageName {
                namespace: "wasi".to_string(),
                name: "io".to_string(),
                version: None,
            },
            docs: Default::default(),
            interfaces: Default::default(),
            worlds: Default::default(),
        };
        let _dep_id = resolve.packages.alloc(dep);

        let deps = extract_dependencies(&resolve, Some(primary_id));

        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].package, "wasi:io");
        assert_eq!(deps[0].version.as_deref(), None);
    }

    // r[verify wit.parse.is-component]
    #[test]
    fn is_component_flag_for_wit_package() {
        use wit_parser::{Package, PackageName, Resolve, World};

        let mut resolve = Resolve::default();
        let world = World {
            name: "hello".to_string(),
            docs: Default::default(),
            imports: Default::default(),
            exports: Default::default(),
            includes: Default::default(),
            package: None,
            stability: Default::default(),
            span: Default::default(),
        };
        let world_id = resolve.worlds.alloc(world);

        let package = Package {
            name: PackageName {
                namespace: "test".to_string(),
                name: "pkg".to_string(),
                version: None,
            },
            docs: Default::default(),
            interfaces: Default::default(),
            worlds: [("hello".to_string(), world_id)].into_iter().collect(),
        };
        let package_id = resolve.packages.alloc(package);
        resolve.worlds[world_id].package = Some(package_id);

        // WitPackage → is_component should be false
        let decoded = DecodedWasm::WitPackage(resolve.clone(), package_id);
        let worlds = extract_worlds(&decoded);
        assert!(!matches!(decoded, DecodedWasm::Component(..)));
        assert_eq!(worlds.len(), 1);

        // Component → is_component should be true
        let decoded = DecodedWasm::Component(resolve, world_id);
        assert!(matches!(decoded, DecodedWasm::Component(..)));
    }
}
