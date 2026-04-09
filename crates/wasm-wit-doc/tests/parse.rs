#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use wasm_wit_doc::{HandleKind, Stability, TypeKind, TypeRef, WorldItemDoc, parse_wit_doc};

    fn empty_deps() -> HashMap<String, String> {
        HashMap::new()
    }

    #[test]
    fn basic_record() {
        let wit = r#"
package test:basic@1.0.0;

interface types {
    /// A person record.
    record person {
        /// The person's name.
        name: string,
        /// The person's age.
        age: u32,
    }
}
"#;
        let doc = parse_wit_doc(wit, "/test/basic/1.0.0", &empty_deps()).unwrap();
        assert_eq!(doc.package_name, "test:basic");
        assert_eq!(doc.version.as_deref(), Some("1.0.0"));
        assert_eq!(doc.interfaces.len(), 1);

        let iface = &doc.interfaces[0];
        assert_eq!(iface.name, "types");
        assert_eq!(iface.url, "/test/basic/1.0.0/interface/types");
        assert_eq!(iface.types.len(), 1);

        let ty = &iface.types[0];
        assert_eq!(ty.name, "person");
        assert_eq!(ty.docs.as_deref(), Some("A person record."));
        assert_eq!(ty.url, "/test/basic/1.0.0/interface/types/person");

        match &ty.kind {
            TypeKind::Record { fields } => {
                assert_eq!(fields.len(), 2);
                assert_eq!(fields[0].name, "name");
                assert_eq!(fields[0].docs.as_deref(), Some("The person's name."));
                assert!(matches!(&fields[0].ty, TypeRef::Primitive { name } if name == "string"));
                assert_eq!(fields[1].name, "age");
                assert!(matches!(&fields[1].ty, TypeRef::Primitive { name } if name == "u32"));
            }
            other => panic!("expected Record, got {other:?}"),
        }
    }

    #[test]
    fn variant_and_enum() {
        let wit = r#"
package test:variants@0.1.0;

interface types {
    /// A color enum.
    enum color {
        red,
        green,
        blue,
    }

    /// A shape variant.
    variant shape {
        circle(f64),
        rectangle(tuple<f64, f64>),
        point,
    }
}
"#;
        let doc = parse_wit_doc(wit, "/test/variants/0.1.0", &empty_deps()).unwrap();
        let iface = &doc.interfaces[0];
        assert_eq!(iface.types.len(), 2);

        // Enum
        let color = &iface.types[0];
        assert_eq!(color.name, "color");
        match &color.kind {
            TypeKind::Enum { cases } => {
                assert_eq!(cases.len(), 3);
                assert_eq!(cases[0].name, "red");
                assert_eq!(cases[1].name, "green");
                assert_eq!(cases[2].name, "blue");
            }
            other => panic!("expected Enum, got {other:?}"),
        }

        // Variant
        let shape = &iface.types[1];
        assert_eq!(shape.name, "shape");
        match &shape.kind {
            TypeKind::Variant { cases } => {
                assert_eq!(cases.len(), 3);
                assert_eq!(cases[0].name, "circle");
                assert!(cases[0].ty.is_some());
                assert_eq!(cases[2].name, "point");
                assert!(cases[2].ty.is_none());
            }
            other => panic!("expected Variant, got {other:?}"),
        }
    }

    #[test]
    fn flags_type() {
        let wit = r#"
package test:flagtest@1.0.0;

interface perms {
    flags permissions {
        read,
        write,
        execute,
    }
}
"#;
        let doc = parse_wit_doc(wit, "/test/flagtest/1.0.0", &empty_deps()).unwrap();
        let ty = &doc.interfaces[0].types[0];
        assert_eq!(ty.name, "permissions");
        match &ty.kind {
            TypeKind::Flags { flags } => {
                assert_eq!(flags.len(), 3);
                assert_eq!(flags[0].name, "read");
                assert_eq!(flags[1].name, "write");
                assert_eq!(flags[2].name, "execute");
            }
            other => panic!("expected Flags, got {other:?}"),
        }
    }

    #[test]
    fn resource_with_methods() {
        let wit = r#"
package test:resources@1.0.0;

interface store {
    /// A key-value store.
    resource kv-store {
        /// Create a new store.
        constructor();
        /// Get a value by key.
        get: func(key: string) -> option<string>;
        /// Set a value.
        set: func(key: string, value: string);
        /// List all keys.
        list-keys: static func() -> list<string>;
    }
}
"#;
        let doc = parse_wit_doc(wit, "/test/resources/1.0.0", &empty_deps()).unwrap();
        let ty = &doc.interfaces[0].types[0];
        assert_eq!(ty.name, "kv-store");
        assert_eq!(ty.docs.as_deref(), Some("A key-value store."));

        match &ty.kind {
            TypeKind::Resource {
                constructor,
                methods,
                statics,
            } => {
                assert!(constructor.is_some(), "should have a constructor");
                assert_eq!(methods.len(), 2, "should have 2 methods");
                assert_eq!(methods[0].name, "get");
                assert_eq!(methods[0].docs.as_deref(), Some("Get a value by key."));
                // Methods include implicit `self` param.
                assert_eq!(methods[0].params.len(), 2);
                assert_eq!(methods[0].params[1].name, "key");
                assert_eq!(methods[1].name, "set");
                assert_eq!(methods[1].params.len(), 3);
                assert_eq!(statics.len(), 1, "should have 1 static");
                assert_eq!(statics[0].name, "list-keys");
            }
            other => panic!("expected Resource, got {other:?}"),
        }
    }

    #[test]
    fn freestanding_functions() {
        let wit = r#"
package test:funcs@1.0.0;

interface api {
    /// Add two numbers.
    add: func(a: u32, b: u32) -> u32;
    /// Log a message.
    log: func(msg: string);
}
"#;
        let doc = parse_wit_doc(wit, "/test/funcs/1.0.0", &empty_deps()).unwrap();
        let iface = &doc.interfaces[0];
        assert_eq!(iface.functions.len(), 2);

        let add = &iface.functions[0];
        assert_eq!(add.name, "add");
        assert_eq!(add.docs.as_deref(), Some("Add two numbers."));
        assert_eq!(add.params.len(), 2);
        assert_eq!(add.params[0].name, "a");
        assert!(add.result.is_some());

        let log = &iface.functions[1];
        assert_eq!(log.name, "log");
        assert!(log.result.is_none());
    }

    #[test]
    fn cross_interface_type_ref() {
        let wit = r#"
package test:cross@1.0.0;

interface base {
    record item {
        id: u64,
    }
}

interface api {
    use base.{item};

    get-item: func(id: u64) -> item;
}
"#;
        let doc = parse_wit_doc(wit, "/test/cross/1.0.0", &empty_deps()).unwrap();
        assert_eq!(doc.interfaces.len(), 2);

        let api_iface = doc.interfaces.iter().find(|i| i.name == "api").unwrap();
        let get_item = &api_iface.functions[0];
        assert_eq!(get_item.name, "get-item");

        // The return type should be a Named ref. The `use` creates a local
        // alias in `api`, so the URL points to the alias in api's namespace.
        match &get_item.result {
            Some(TypeRef::Named { name, url }) => {
                assert_eq!(name, "item");
                assert!(url.is_some(), "should have a URL for same-package type");
                let url_str = url.as_ref().unwrap();
                assert!(
                    url_str.contains("/interface/") && url_str.contains("/item"),
                    "URL should contain interface path and item name, got: {url_str}"
                );
            }
            other => panic!("expected Named TypeRef, got {other:?}"),
        }
    }

    #[test]
    fn worlds() {
        let wit = r#"
package test:worlds@1.0.0;

interface handler {
    handle: func(input: string) -> string;
}

/// A proxy world.
world proxy {
    import handler;
    export handler;
}
"#;
        let doc = parse_wit_doc(wit, "/test/worlds/1.0.0", &empty_deps()).unwrap();
        assert_eq!(doc.worlds.len(), 1);

        let world = &doc.worlds[0];
        assert_eq!(world.name, "proxy");
        assert_eq!(world.docs.as_deref(), Some("A proxy world."));
        assert_eq!(world.url, "/test/worlds/1.0.0/world/proxy");

        assert!(!world.imports.is_empty());
        assert!(!world.exports.is_empty());

        // Both should be interface items.
        match &world.imports[0] {
            WorldItemDoc::Interface { name, url } => {
                assert!(name.contains("handler"));
                assert!(url.is_some());
            }
            other => panic!("expected Interface, got {other:?}"),
        }
    }

    #[test]
    fn cross_interface_use_resolves_url() {
        // Test that `use` across interfaces in the same package produces
        // a Named ref with a valid URL.
        let wit = r#"
package test:crossuse@1.0.0;

interface base {
    record point {
        x: f64,
        y: f64,
    }
}

interface draw {
    use base.{point};

    draw-at: func(p: point);
}
"#;
        let doc = parse_wit_doc(wit, "/test/crossuse/1.0.0", &empty_deps()).unwrap();
        let draw = doc.interfaces.iter().find(|i| i.name == "draw").unwrap();
        let draw_at = &draw.functions[0];

        // The `point` param should have a URL.
        match &draw_at.params[0].ty {
            TypeRef::Named { name, url } => {
                assert_eq!(name, "point");
                assert!(url.is_some(), "should have a URL for the used type");
            }
            other => panic!("expected Named TypeRef, got {other:?}"),
        }
    }

    #[test]
    fn type_alias() {
        let wit = r#"
package test:alias@1.0.0;

interface types {
    type my-string = string;
    type my-list = list<u8>;
}
"#;
        let doc = parse_wit_doc(wit, "/test/alias/1.0.0", &empty_deps()).unwrap();
        let types = &doc.interfaces[0].types;
        assert_eq!(types.len(), 2);

        assert_eq!(types[0].name, "my-string");
        match &types[0].kind {
            TypeKind::Alias(TypeRef::Primitive { name }) => assert_eq!(name, "string"),
            other => panic!("expected Alias(Primitive), got {other:?}"),
        }

        assert_eq!(types[1].name, "my-list");
        match &types[1].kind {
            TypeKind::Alias(TypeRef::List { .. }) => {}
            other => panic!("expected Alias(List), got {other:?}"),
        }
    }

    #[test]
    fn handle_types() {
        let wit = r#"
package test:handles@1.0.0;

interface store {
    resource connection;

    open: func() -> connection;
    use-conn: func(c: borrow<connection>);
}
"#;
        let doc = parse_wit_doc(wit, "/test/handles/1.0.0", &empty_deps()).unwrap();
        let iface = &doc.interfaces[0];

        let open_fn = iface.functions.iter().find(|f| f.name == "open").unwrap();
        match &open_fn.result {
            Some(TypeRef::Handle {
                handle_kind,
                resource_name,
                resource_url,
            }) => {
                assert!(matches!(handle_kind, HandleKind::Own));
                assert_eq!(resource_name, "connection");
                assert!(resource_url.is_some());
            }
            other => panic!("expected Handle, got {other:?}"),
        }

        let use_fn = iface
            .functions
            .iter()
            .find(|f| f.name == "use-conn")
            .unwrap();
        match &use_fn.params[0].ty {
            TypeRef::Handle {
                handle_kind,
                resource_name,
                ..
            } => {
                assert!(matches!(handle_kind, HandleKind::Borrow));
                assert_eq!(resource_name, "connection");
            }
            other => panic!("expected Handle, got {other:?}"),
        }
    }

    #[test]
    fn empty_interface() {
        let wit = r#"
package test:empty@1.0.0;

interface nothing {}
"#;
        let doc = parse_wit_doc(wit, "/test/empty/1.0.0", &empty_deps()).unwrap();
        assert_eq!(doc.interfaces.len(), 1);
        assert!(doc.interfaces[0].types.is_empty());
        assert!(doc.interfaces[0].functions.is_empty());
    }

    #[test]
    fn stability_unknown_by_default() {
        let wit = r#"
package test:stab@1.0.0;

interface api {
    do-thing: func();
}
"#;
        let doc = parse_wit_doc(wit, "/test/stab/1.0.0", &empty_deps()).unwrap();
        let func = &doc.interfaces[0].functions[0];
        assert!(matches!(func.stability, Stability::Unknown));
    }

    #[test]
    fn invalid_wit_returns_error() {
        let result = parse_wit_doc("this is not valid wit", "/test/bad/1.0.0", &empty_deps());
        assert!(result.is_err());
    }

    #[test]
    fn multiple_interfaces_and_worlds() {
        let wit = r#"
package test:multi@1.0.0;

interface alpha {
    a-func: func();
}

interface beta {
    b-func: func();
}

world w1 {
    import alpha;
}

world w2 {
    import beta;
    export alpha;
}
"#;
        let doc = parse_wit_doc(wit, "/test/multi/1.0.0", &empty_deps()).unwrap();
        assert_eq!(doc.interfaces.len(), 2);
        assert_eq!(doc.worlds.len(), 2);
        assert_eq!(doc.worlds[0].name, "w1");
        assert_eq!(doc.worlds[1].name, "w2");
    }
}
