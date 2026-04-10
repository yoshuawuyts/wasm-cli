//! Conversion from `wit-parser`'s `Resolve` to the document model.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use wit_parser::{
    FunctionKind, Handle, InterfaceId, Resolve, Type as WitType, TypeDefKind, TypeId, TypeOwner,
    WorldItem, WorldKey,
};

use crate::types::{
    CaseDoc, EnumCaseDoc, FieldDoc, FlagDoc, FunctionDoc, HandleKind, InterfaceDoc, ParamDoc,
    Stability, TypeDoc, TypeKind, TypeRef, WitDocument, WorldDoc, WorldItemDoc,
};

/// Internal state used during conversion.
struct Converter<'a> {
    resolve: &'a Resolve,
    url_base: &'a str,
    dep_urls: &'a HashMap<String, String>,
    /// Interface IDs that belong to the primary package.
    own_interfaces: HashSet<InterfaceId>,
    /// Maps `TypeId` → `(url, name)` for all named types in the primary
    /// package, built during the interface pass.
    type_urls: HashMap<TypeId, (String, String)>,
}

/// Parse WIT text into a [`WitDocument`].
///
/// # Arguments
///
/// * `wit_text` — WIT source text (as produced by `WitPrinter` or
///   hand-written).
/// * `url_base` — base URL for this package (e.g. `"/wasi/http/0.2.11"`).
/// * `dep_urls` — maps dependency package names (e.g. `"wasi:io"`) to their
///   URL base (e.g. `"/wasi/io/0.2.2"`).
///
/// # Errors
///
/// Returns an error if the WIT text cannot be parsed.
pub(crate) fn convert(
    wit_text: &str,
    url_base: &str,
    dep_urls: &HashMap<String, String>,
) -> anyhow::Result<WitDocument> {
    let mut resolve = Resolve::default();
    let package_id = resolve.push_str(Path::new("input.wit"), wit_text)?;

    let package = resolve
        .packages
        .get(package_id)
        .expect("just-inserted package should exist");

    let package_name = format!("{}:{}", package.name.namespace, package.name.name);
    let version = package.name.version.as_ref().map(ToString::to_string);
    let docs = package.docs.contents.clone();

    let mut converter = Converter {
        resolve: &resolve,
        url_base,
        dep_urls,
        own_interfaces: package.interfaces.values().copied().collect(),
        type_urls: HashMap::new(),
    };

    // First pass: register all type URLs so cross-refs within this package
    // resolve correctly.
    for (iface_name, iface_id) in &package.interfaces {
        let iface = resolve
            .interfaces
            .get(*iface_id)
            .expect("interface id should be valid");
        for (type_name, type_id) in &iface.types {
            let url = format!("{url_base}/interface/{iface_name}/{type_name}");
            converter
                .type_urls
                .insert(*type_id, (url, type_name.clone()));
        }
    }

    // Second pass: build the full document.
    let interfaces = package
        .interfaces
        .iter()
        .map(|(name, id)| converter.convert_interface(name, *id))
        .collect();

    let worlds = package
        .worlds
        .iter()
        .map(|(name, id)| converter.convert_world(name, *id))
        .collect();

    Ok(WitDocument {
        package_name,
        version,
        docs,
        interfaces,
        worlds,
    })
}

impl Converter<'_> {
    /// Convert a single interface.
    fn convert_interface(&self, name: &str, id: InterfaceId) -> InterfaceDoc {
        let iface = self
            .resolve
            .interfaces
            .get(id)
            .expect("interface id should be valid");

        let iface_url = format!("{}/interface/{name}", self.url_base);

        // Collect all types, separating resource-associated functions.
        let mut resource_constructors: HashMap<TypeId, FunctionDoc> = HashMap::new();
        let mut resource_methods: HashMap<TypeId, Vec<FunctionDoc>> = HashMap::new();
        let mut resource_statics: HashMap<TypeId, Vec<FunctionDoc>> = HashMap::new();
        let mut freestanding_functions = Vec::new();

        for (_, func) in &iface.functions {
            match &func.kind {
                FunctionKind::Constructor(res_id) => {
                    let doc = self.convert_function(func, name);
                    resource_constructors.insert(*res_id, doc);
                }
                FunctionKind::Method(res_id) | FunctionKind::AsyncMethod(res_id) => {
                    let doc = self.convert_function(func, name);
                    resource_methods.entry(*res_id).or_default().push(doc);
                }
                FunctionKind::Static(res_id) | FunctionKind::AsyncStatic(res_id) => {
                    let doc = self.convert_function(func, name);
                    resource_statics.entry(*res_id).or_default().push(doc);
                }
                FunctionKind::Freestanding | FunctionKind::AsyncFreestanding => {
                    freestanding_functions.push(self.convert_function(func, name));
                }
            }
        }

        // Convert types.
        let types = iface
            .types
            .iter()
            .map(|(type_name, type_id)| {
                self.convert_type_def(
                    type_name,
                    *type_id,
                    name,
                    &mut resource_constructors,
                    &mut resource_methods,
                    &mut resource_statics,
                )
            })
            .collect();

        InterfaceDoc {
            name: name.to_owned(),
            docs: iface.docs.contents.clone(),
            types,
            functions: freestanding_functions,
            url: iface_url,
        }
    }

    /// Convert a single type definition.
    fn convert_type_def(
        &self,
        name: &str,
        type_id: TypeId,
        iface_name: &str,
        constructors: &mut HashMap<TypeId, FunctionDoc>,
        methods: &mut HashMap<TypeId, Vec<FunctionDoc>>,
        statics: &mut HashMap<TypeId, Vec<FunctionDoc>>,
    ) -> TypeDoc {
        let type_def = self
            .resolve
            .types
            .get(type_id)
            .expect("type id should be valid");

        let url = format!("{}/interface/{iface_name}/{name}", self.url_base);
        let stability = convert_stability(&type_def.stability);
        let docs = type_def.docs.contents.clone();

        let kind = match &type_def.kind {
            TypeDefKind::Record(record) => TypeKind::Record {
                fields: record
                    .fields
                    .iter()
                    .map(|f| FieldDoc {
                        name: f.name.clone(),
                        ty: self.convert_type(f.ty),
                        docs: f.docs.contents.clone(),
                    })
                    .collect(),
            },
            TypeDefKind::Variant(variant) => TypeKind::Variant {
                cases: variant
                    .cases
                    .iter()
                    .map(|c| CaseDoc {
                        name: c.name.clone(),
                        ty: c.ty.map(|t| self.convert_type(t)),
                        docs: c.docs.contents.clone(),
                    })
                    .collect(),
            },
            TypeDefKind::Enum(e) => TypeKind::Enum {
                cases: e
                    .cases
                    .iter()
                    .map(|c| EnumCaseDoc {
                        name: c.name.clone(),
                        docs: c.docs.contents.clone(),
                    })
                    .collect(),
            },
            TypeDefKind::Flags(flags) => TypeKind::Flags {
                flags: flags
                    .flags
                    .iter()
                    .map(|f| FlagDoc {
                        name: f.name.clone(),
                        docs: f.docs.contents.clone(),
                    })
                    .collect(),
            },
            TypeDefKind::Resource => TypeKind::Resource {
                constructor: constructors.remove(&type_id),
                methods: methods.remove(&type_id).unwrap_or_default(),
                statics: statics.remove(&type_id).unwrap_or_default(),
            },
            TypeDefKind::Handle(handle) => TypeKind::Alias(self.convert_handle(handle)),
            TypeDefKind::Type(ty) => TypeKind::Alias(self.convert_type(*ty)),
            TypeDefKind::List(ty) => TypeKind::Alias(TypeRef::List {
                ty: Box::new(self.convert_type(*ty)),
            }),
            TypeDefKind::Option(ty) => TypeKind::Alias(TypeRef::Option {
                ty: Box::new(self.convert_type(*ty)),
            }),
            TypeDefKind::Result(r) => TypeKind::Alias(TypeRef::Result {
                ok: r.ok.map(|t| Box::new(self.convert_type(t))),
                err: r.err.map(|t| Box::new(self.convert_type(t))),
            }),
            TypeDefKind::Tuple(t) => TypeKind::Alias(TypeRef::Tuple {
                types: t.types.iter().map(|ty| self.convert_type(*ty)).collect(),
            }),
            TypeDefKind::Future(ty) => TypeKind::Alias(TypeRef::Future {
                ty: ty.map(|t| Box::new(self.convert_type(t))),
            }),
            TypeDefKind::Stream(ty) => TypeKind::Alias(TypeRef::Stream {
                ty: ty.map(|t| Box::new(self.convert_type(t))),
            }),
            TypeDefKind::Unknown => TypeKind::Alias(TypeRef::Primitive {
                name: "unknown".to_owned(),
            }),
            TypeDefKind::Map(key, val) => TypeKind::Alias(TypeRef::Tuple {
                types: vec![self.convert_type(*key), self.convert_type(*val)],
            }),
            TypeDefKind::FixedLengthList(ty, _len) => TypeKind::Alias(TypeRef::List {
                ty: Box::new(self.convert_type(*ty)),
            }),
        };

        TypeDoc {
            name: name.to_owned(),
            docs,
            kind,
            stability,
            url,
        }
    }

    /// Convert a `wit_parser::Type` to a `TypeRef`.
    fn convert_type(&self, ty: WitType) -> TypeRef {
        match ty {
            WitType::Bool => TypeRef::Primitive {
                name: "bool".to_owned(),
            },
            WitType::U8 => TypeRef::Primitive {
                name: "u8".to_owned(),
            },
            WitType::U16 => TypeRef::Primitive {
                name: "u16".to_owned(),
            },
            WitType::U32 => TypeRef::Primitive {
                name: "u32".to_owned(),
            },
            WitType::U64 => TypeRef::Primitive {
                name: "u64".to_owned(),
            },
            WitType::S8 => TypeRef::Primitive {
                name: "s8".to_owned(),
            },
            WitType::S16 => TypeRef::Primitive {
                name: "s16".to_owned(),
            },
            WitType::S32 => TypeRef::Primitive {
                name: "s32".to_owned(),
            },
            WitType::S64 => TypeRef::Primitive {
                name: "s64".to_owned(),
            },
            WitType::F32 => TypeRef::Primitive {
                name: "f32".to_owned(),
            },
            WitType::F64 => TypeRef::Primitive {
                name: "f64".to_owned(),
            },
            WitType::Char => TypeRef::Primitive {
                name: "char".to_owned(),
            },
            WitType::String => TypeRef::Primitive {
                name: "string".to_owned(),
            },
            WitType::ErrorContext => TypeRef::Primitive {
                name: "error-context".to_owned(),
            },
            WitType::Id(id) => self.convert_type_id(id),
        }
    }

    /// Resolve a `TypeId` into a `TypeRef`, following aliases, looking up
    /// URLs, and handling anonymous composite types.
    fn convert_type_id(&self, id: TypeId) -> TypeRef {
        let type_def = self.resolve.types.get(id).expect("type id should be valid");

        // Named type in our package — link to it.
        if let Some((url, name)) = self.type_urls.get(&id) {
            return TypeRef::Named {
                name: name.clone(),
                url: Some(url.clone()),
            };
        }

        // Named type in another package — try dep_urls.
        if let Some(name) = &type_def.name {
            if let TypeOwner::Interface(iface_id) = type_def.owner {
                let iface = self
                    .resolve
                    .interfaces
                    .get(iface_id)
                    .expect("interface id should be valid");
                if let Some(pkg_id) = iface.package {
                    let pkg = self
                        .resolve
                        .packages
                        .get(pkg_id)
                        .expect("package id should be valid");
                    let pkg_key = format!("{}:{}", pkg.name.namespace, pkg.name.name);
                    if let (Some(dep_base), Some(iface_name)) =
                        (self.dep_urls.get(&pkg_key), &iface.name)
                    {
                        let url = format!("{dep_base}/interface/{iface_name}/{name}");
                        return TypeRef::Named {
                            name: name.clone(),
                            url: Some(url),
                        };
                    }
                    // External type without URL mapping — render name without
                    // link.
                    return TypeRef::Named {
                        name: name.clone(),
                        url: None,
                    };
                }
            }

            // Named type with no resolvable package.
            return TypeRef::Named {
                name: name.clone(),
                url: None,
            };
        }

        // Anonymous (structural) type — inline its structure.
        match &type_def.kind {
            TypeDefKind::List(ty) => TypeRef::List {
                ty: Box::new(self.convert_type(*ty)),
            },
            TypeDefKind::Option(ty) => TypeRef::Option {
                ty: Box::new(self.convert_type(*ty)),
            },
            TypeDefKind::Result(r) => TypeRef::Result {
                ok: r.ok.map(|t| Box::new(self.convert_type(t))),
                err: r.err.map(|t| Box::new(self.convert_type(t))),
            },
            TypeDefKind::Tuple(t) => TypeRef::Tuple {
                types: t.types.iter().map(|ty| self.convert_type(*ty)).collect(),
            },
            TypeDefKind::Handle(handle) => self.convert_handle(handle),
            TypeDefKind::Future(ty) => TypeRef::Future {
                ty: ty.map(|t| Box::new(self.convert_type(t))),
            },
            TypeDefKind::Stream(ty) => TypeRef::Stream {
                ty: ty.map(|t| Box::new(self.convert_type(t))),
            },
            TypeDefKind::Type(inner) => self.convert_type(*inner),
            _ => TypeRef::Primitive {
                name: "unknown".to_owned(),
            },
        }
    }

    /// Convert a `Handle` to a `TypeRef`.
    fn convert_handle(&self, handle: &Handle) -> TypeRef {
        let (handle_kind, res_id) = match handle {
            Handle::Own(id) => (HandleKind::Own, *id),
            Handle::Borrow(id) => (HandleKind::Borrow, *id),
        };
        let type_def = self
            .resolve
            .types
            .get(res_id)
            .expect("resource type id should be valid");
        let resource_name = type_def
            .name
            .clone()
            .unwrap_or_else(|| "unknown".to_owned());
        let resource_url = self.type_urls.get(&res_id).map(|(url, _)| url.clone());
        TypeRef::Handle {
            handle_kind,
            resource_name,
            resource_url,
        }
    }

    /// Convert a function.
    fn convert_function(&self, func: &wit_parser::Function, iface_name: &str) -> FunctionDoc {
        let display_name = func.item_name().to_owned();
        let url = format!("{}/interface/{iface_name}/{}", self.url_base, display_name);

        FunctionDoc {
            name: display_name,
            docs: func.docs.contents.clone(),
            params: func
                .params
                .iter()
                .map(|param| ParamDoc {
                    name: param.name.clone(),
                    ty: self.convert_type(param.ty),
                })
                .collect(),
            result: func.result.map(|t| self.convert_type(t)),
            stability: convert_stability(&func.stability),
            url,
        }
    }

    /// Convert a world.
    fn convert_world(&self, name: &str, id: wit_parser::WorldId) -> WorldDoc {
        let world = self
            .resolve
            .worlds
            .get(id)
            .expect("world id should be valid");

        let url = format!("{}/world/{name}", self.url_base);

        WorldDoc {
            name: name.to_owned(),
            docs: world.docs.contents.clone(),
            imports: world
                .imports
                .iter()
                .map(|(key, item)| self.convert_world_item(key, item))
                .collect(),
            exports: world
                .exports
                .iter()
                .map(|(key, item)| self.convert_world_item(key, item))
                .collect(),
            url,
        }
    }

    /// Convert a world import/export item.
    fn convert_world_item(&self, key: &WorldKey, item: &WorldItem) -> WorldItemDoc {
        match item {
            WorldItem::Interface { id, .. } => {
                let iface = self
                    .resolve
                    .interfaces
                    .get(*id)
                    .expect("interface id should be valid");
                let (display_name, url) = self.resolve_interface_ref(*id, iface);
                WorldItemDoc::Interface {
                    name: display_name,
                    url,
                }
            }
            WorldItem::Function(func) => {
                let iface_name = match key {
                    WorldKey::Name(n) => n.as_str(),
                    WorldKey::Interface(_) => "world",
                };
                WorldItemDoc::Function(self.convert_function(func, iface_name))
            }
            WorldItem::Type { id: type_id, .. } => {
                let type_def = self
                    .resolve
                    .types
                    .get(*type_id)
                    .expect("type id should be valid");
                let type_name = type_def
                    .name
                    .clone()
                    .unwrap_or_else(|| "unnamed".to_owned());
                let mut constructors = HashMap::new();
                let mut methods = HashMap::new();
                let mut statics = HashMap::new();
                WorldItemDoc::Type(self.convert_type_def(
                    &type_name,
                    *type_id,
                    "world",
                    &mut constructors,
                    &mut methods,
                    &mut statics,
                ))
            }
        }
    }

    /// Build a display name and optional URL for an interface reference used
    /// in world imports/exports.
    fn resolve_interface_ref(
        &self,
        id: InterfaceId,
        iface: &wit_parser::Interface,
    ) -> (String, Option<String>) {
        if let Some(pkg_id) = iface.package {
            let pkg = self
                .resolve
                .packages
                .get(pkg_id)
                .expect("package id should be valid");
            let pkg_key = format!("{}:{}", pkg.name.namespace, pkg.name.name);
            let iface_name = iface.name.as_deref().unwrap_or("unnamed");
            let version_suffix = pkg
                .name
                .version
                .as_ref()
                .map(|v| format!("@{v}"))
                .unwrap_or_default();
            let display = format!("{pkg_key}/{iface_name}{version_suffix}");

            // Check if this is an interface in our own package.
            if self.own_interfaces.contains(&id) {
                let iface_url = format!("{}/interface/{iface_name}", self.url_base);
                return (display, Some(iface_url));
            }

            // Check dep_urls for external packages.
            if let Some(dep_base) = self.dep_urls.get(&pkg_key) {
                let url = format!("{dep_base}/interface/{iface_name}");
                return (display, Some(url));
            }

            (display, None)
        } else {
            let name = iface
                .name
                .clone()
                .unwrap_or_else(|| format!("interface-{id:?}"));
            (name, None)
        }
    }
}

/// Convert `wit_parser::Stability` to our `Stability`.
fn convert_stability(s: &wit_parser::Stability) -> Stability {
    match s {
        wit_parser::Stability::Unknown => Stability::Unknown,
        wit_parser::Stability::Unstable {
            feature,
            deprecated,
            ..
        } => Stability::Unstable {
            feature: feature.clone(),
            deprecated: deprecated.as_ref().map(ToString::to_string),
        },
        wit_parser::Stability::Stable {
            since, deprecated, ..
        } => Stability::Stable {
            since: since.to_string(),
            deprecated: deprecated.as_ref().map(ToString::to_string),
        },
    }
}
