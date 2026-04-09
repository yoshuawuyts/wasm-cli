//! Document model types for WIT documentation.
//!
//! These types represent a parsed WIT package as a rich, navigable document
//! model with pre-resolved URLs for cross-linking.

use serde::{Deserialize, Serialize};

/// Root document for a WIT package.
///
/// Contains all interfaces and worlds defined in the package, with
/// pre-resolved URLs for navigation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WitDocument {
    /// The WIT package name (e.g. `"wasi:http"`).
    pub package_name: String,
    /// The package version, if any (e.g. `"0.2.11"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Package-level documentation comment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub docs: Option<String>,
    /// All interfaces defined in this package.
    pub interfaces: Vec<InterfaceDoc>,
    /// All worlds defined in this package.
    pub worlds: Vec<WorldDoc>,
}

/// Documentation for a single WIT interface.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterfaceDoc {
    /// The interface name (e.g. `"types"`, `"outgoing-handler"`).
    pub name: String,
    /// Documentation comment for this interface.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub docs: Option<String>,
    /// Types defined in this interface.
    pub types: Vec<TypeDoc>,
    /// Freestanding functions defined in this interface.
    pub functions: Vec<FunctionDoc>,
    /// Pre-resolved URL for this interface page.
    pub url: String,
}

/// Documentation for a single type definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeDoc {
    /// The type name (e.g. `"outgoing-request"`, `"method"`).
    pub name: String,
    /// Documentation comment for this type.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub docs: Option<String>,
    /// The kind of type and its structural details.
    pub kind: TypeKind,
    /// API stability information.
    #[serde(default)]
    pub stability: Stability,
    /// Pre-resolved URL for this type's detail page.
    pub url: String,
}

/// The structural kind of a type definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum TypeKind {
    /// A record type with named fields.
    Record {
        /// The record's fields.
        fields: Vec<FieldDoc>,
    },
    /// A variant type (tagged union) with named cases.
    Variant {
        /// The variant's cases.
        cases: Vec<CaseDoc>,
    },
    /// An enum type with named cases (no payloads).
    Enum {
        /// The enum's cases.
        cases: Vec<EnumCaseDoc>,
    },
    /// A flags type (named bit flags).
    Flags {
        /// The flag definitions.
        flags: Vec<FlagDoc>,
    },
    /// A resource type with an optional constructor, methods, and static
    /// functions.
    Resource {
        /// The resource constructor, if defined.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        constructor: Option<FunctionDoc>,
        /// Instance methods (first parameter is implicitly `borrow<self>`).
        methods: Vec<FunctionDoc>,
        /// Static functions associated with this resource.
        statics: Vec<FunctionDoc>,
    },
    /// A type alias referring to another type.
    Alias(TypeRef),
}

/// A reference to a type, used in fields, parameters, and return types.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum TypeRef {
    /// A WIT primitive type (`bool`, `u8`, `string`, etc.).
    Primitive {
        /// The primitive type name.
        name: String,
    },
    /// A reference to a named type defined in an interface.
    Named {
        /// The type name.
        name: String,
        /// Pre-resolved URL to the type's detail page, if available.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        url: Option<String>,
    },
    /// `list<T>`.
    List {
        /// The element type.
        #[serde(rename = "type")]
        ty: Box<TypeRef>,
    },
    /// `option<T>`.
    Option {
        /// The inner type.
        #[serde(rename = "type")]
        ty: Box<TypeRef>,
    },
    /// `result<ok, err>`.
    Result {
        /// The success type, if any.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ok: Option<Box<TypeRef>>,
        /// The error type, if any.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        err: Option<Box<TypeRef>>,
    },
    /// `tuple<T1, T2, ...>`.
    Tuple {
        /// The element types.
        types: Vec<TypeRef>,
    },
    /// `own<T>` or `borrow<T>`.
    Handle {
        /// Whether this is an `own` or `borrow` handle.
        handle_kind: HandleKind,
        /// The resource name.
        resource_name: String,
        /// Pre-resolved URL to the resource's detail page.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        resource_url: Option<String>,
    },
    /// `future<T>` or bare `future`.
    Future {
        /// The inner type, if any.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[serde(rename = "type")]
        ty: Option<Box<TypeRef>>,
    },
    /// `stream<T>` or bare `stream`.
    Stream {
        /// The inner type, if any.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[serde(rename = "type")]
        ty: Option<Box<TypeRef>>,
    },
}

/// The kind of a handle type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HandleKind {
    /// `own<T>` — exclusive ownership.
    Own,
    /// `borrow<T>` — borrowed reference.
    Borrow,
}

/// A record field.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldDoc {
    /// The field name.
    pub name: String,
    /// The field's type.
    #[serde(rename = "type")]
    pub ty: TypeRef,
    /// Documentation comment for this field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub docs: Option<String>,
}

/// A variant case (may have a payload type).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaseDoc {
    /// The case name.
    pub name: String,
    /// The payload type, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "type")]
    pub ty: Option<TypeRef>,
    /// Documentation comment for this case.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub docs: Option<String>,
}

/// An enum case (no payload).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnumCaseDoc {
    /// The case name.
    pub name: String,
    /// Documentation comment for this case.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub docs: Option<String>,
}

/// A flag definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlagDoc {
    /// The flag name.
    pub name: String,
    /// Documentation comment for this flag.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub docs: Option<String>,
}

/// Documentation for a function.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDoc {
    /// The function name (e.g. `"handle"`, `"new"`, `"method"`).
    pub name: String,
    /// Documentation comment for this function.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub docs: Option<String>,
    /// The function's parameters.
    pub params: Vec<ParamDoc>,
    /// The return type, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<TypeRef>,
    /// API stability information.
    #[serde(default)]
    pub stability: Stability,
    /// Pre-resolved URL for this function's detail page.
    pub url: String,
}

/// A function parameter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParamDoc {
    /// The parameter name.
    pub name: String,
    /// The parameter type.
    #[serde(rename = "type")]
    pub ty: TypeRef,
}

/// Documentation for a WIT world.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldDoc {
    /// The world name (e.g. `"proxy"`, `"command"`).
    pub name: String,
    /// Documentation comment for this world.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub docs: Option<String>,
    /// Items imported by this world.
    pub imports: Vec<WorldItemDoc>,
    /// Items exported by this world.
    pub exports: Vec<WorldItemDoc>,
    /// Pre-resolved URL for this world's detail page.
    pub url: String,
}

/// An item imported or exported by a world.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum WorldItemDoc {
    /// A named interface import/export.
    Interface {
        /// The interface name as declared (e.g.
        /// `"wasi:http/types@0.2.11"`).
        name: String,
        /// Pre-resolved URL to the interface page, if available.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        url: Option<String>,
    },
    /// A freestanding function import/export.
    Function(FunctionDoc),
    /// A type export.
    Type(TypeDoc),
}

/// API stability metadata.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(tag = "level")]
pub enum Stability {
    /// Stability is not specified.
    #[default]
    Unknown,
    /// Unstable / feature-gated.
    Unstable {
        /// The feature gate name.
        feature: String,
        /// Deprecation version, if deprecated.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        deprecated: Option<String>,
    },
    /// Stable since a given version.
    Stable {
        /// The version where this became stable.
        since: String,
        /// Deprecation version, if deprecated.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        deprecated: Option<String>,
    },
}
