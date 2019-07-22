use capnp::private::layout;
use capnpc::schema_capnp;

use std::collections::HashMap;
use capnp::Word;

type Id = u64;
type NodeId = Id;
type NodeName = String;
type VarName = String;

#[derive(Clone)]
pub enum BrandBinding {
    Unbound,
    Type(Type),
}

#[derive(Clone)]
pub enum BrandScopeKind {
    Bind(Vec<BrandBinding>),
    Inherit,
}

#[derive(Clone)]
pub struct BrandScope {
    scope_id: Id,
    kind: BrandScopeKind,
}

#[derive(Clone)]
pub struct Brand {
    scopes: Vec<BrandScope>,
}

#[derive(Clone)]
pub struct Annotation {
    id: Id,
    brand: Brand,
    value: Value,
}

type Annotations = Vec<Annotation>;

#[derive(Clone)]
pub enum FieldKind {
    Slot {
        offset: u32,
        type_: Type,
        default_value: Value,
        had_explicit_default: bool,
    },
    Group {
        type_id: Id,
    },
}

#[derive(Clone)]
pub struct Field {
    name: String,
    code_order: u16,
    annotations: Annotations,

    kind: FieldKind,
}

#[derive(Clone)]
pub struct Enumerant {
    ordinal: u32,
    name: String,
    code_order: u16,
    annotations: Annotations,
}

#[derive(Clone)]
pub struct UnionItem {
    enumerant: Enumerant,
    field: Field,
}

#[derive(Clone)]
pub struct Union {
    items: Vec<UnionItem>
}

#[derive(Clone)]
pub struct Method {
    name: String,
    code_order: u16,
    implicit_parameters: Parameters,
    param_type: NodeId,
    param_brand: Brand,
    result_type: NodeId,
    result_brand: Brand,

    annotations: Annotations,
}

#[derive(Clone)]
pub struct Superclass {
    id: NodeId,
    brand: Brand,
}

#[derive(Clone)]
pub struct Parameter {
    name: VarName
}

type Parameters = Vec<Parameter>;

#[derive(Clone)]
pub enum NodeKind {
    File,
    Struct {
        // structs can be initialized and return a Builder ?
        // structs can be initialized and return a Reader ?
        // structs can be deserialized and blah-blah-blah

        size: layout::StructSize,
        preferred_list_encoding: schema_capnp::ElementSize,
        is_group: bool,

        // all fields that are not in the union
        fields: Vec<Field>,

        // fields that are in the union
        // we basically need to create a subtype that is relevant to the fields here (which must be accessible)
        // through Node
        which: Option<Union>,
    },
    Enum {
        items: Vec<Enumerant>,
    },
    Interface {
        methods: Vec<Method>,
        superclasses: Vec<Superclass>,
    },
    Const {
        type_: Type,
        value: Value,
    },
    Annotation {
        type_: Type,
    },
}

#[derive(Clone)]
pub struct Node {
    id: NodeId,

    scope_id: Id,
    parameters: Parameters,
    is_generic: bool,
    // True if this node is generic, meaning that it or one of its parent scopes has a non-empty
    // `parameters`.

    nested: Vec<(NodeName, NodeId)>,

    annotations: Annotations,

    kind: NodeKind,
}

#[derive(Clone)]
pub enum Type {
    Void,
    Bool,
    Int8,
    Int16,
    Int32,
    Int64,
    Uint8,
    Uint16,
    Uint32,
    Uint64,
    Float32,
    Float64,
    Text,
    Data,
    List { element: Box<Type> },
    Enum { id: NodeId, brand: Brand },
    Struct { id: NodeId, brand: Brand },
    Interface { id: NodeId, brand: Brand },
    AnyPointer(AnyPointerType)
}

#[derive(Clone)]
pub enum AnyPointerType {
    UnconstrainedAnyKind,
    UnconstrainedStruct,
    UnconstrainedList,
    UnconstrainedCap,
    Parameter { scope_id: Id, index: u16 },
    ImplicitMethodParamater { index: u16 }
}

#[derive(Clone)]
pub enum Value {
    Void,
    Bool(bool),
    Int8(i8),
    Int16(i16),
    Int32(i32),
    Int64(i64),
    Uint8(u8),
    Uint16(u16),
    Uint32(u32),
    Uint64(u64),
    Float32(f32),
    Float64(f64),
    Text(Vec<u8>),
    Data(Vec<u8>),
    List(AnyPointerValue),
    Enum(u16),
    Struct(AnyPointerValue),
    Interface(()),
    AnyPointer(AnyPointerValue),
}

#[derive(Clone)]
pub enum AnyPointerValue {
    Struct(),
    List()
}

pub struct Arena {
    items: HashMap<NodeId, NodeKind>,
}

impl Arena {
    //pub fn from_
}