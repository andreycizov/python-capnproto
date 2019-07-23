use capnp::private::layout;
use capnpc::schema_capnp;

use std::collections::HashMap;
use capnp::Word;
use crate::Error;
use std::ops::Deref;

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

impl Brand {
    fn from_reader(
        reader: &schema_capnp::brand::Reader,
    ) -> Result<Brand, Error> {
        let mut r = Vec::with_capacity(reader.get_scopes()?.len() as usize);

        for item in reader.get_scopes()?.iter() {
            let scope_id = item.get_scope_id();
            let scope_kind = match item.which()? {
                schema_capnp::brand::scope::Bind(x) => {
                    let x: &capnp::struct_list::Reader<schema_capnp::brand::binding::Owned> = &x;
                    let mut r = Vec::with_capacity(x.len() as usize);
                    for item in x.iter() {
                        let y = match item.which()? {
                            schema_capnp::brand::binding::Type(x) => {
                                let x: &schema_capnp::type_::Reader = &x?;

                                BrandBinding::Type(Type::from_reader(x)?)
                            }
                            schema_capnp::brand::binding::Unbound(()) => {
                                BrandBinding::Unbound
                            }
                        };

                        r.push(y);
                    }

                    BrandScopeKind::Bind(r)
                }
                schema_capnp::brand::scope::Inherit(()) => {
                    BrandScopeKind::Inherit
                }
            };

            r.push(BrandScope { scope_id, kind: scope_kind });
        }

        Ok(Brand { scopes: r })
    }
}

#[derive(Clone)]
pub struct Annotation {
    id: Id,
    brand: Brand,
    value: Value,
}

impl Annotation {
    fn from_reader(
        reader: &schema_capnp::annotation::Reader
    ) -> Result<Annotation, Error> {
        Ok(Annotation {
            id: reader.get_id(),
            brand: Brand::from_reader(&reader.get_brand()?)?,
            value: Value::from_reader(reader.get_value()?)?,
        })
    }
}

struct Annotations(Vec<Annotation>);

impl Annotations {
    fn from_reader(
        reader: &capnp::struct_list::Reader<schema_capnp::annotation::Owned>
    ) -> Result<Annotations, Error> {
        let mut r = Vec::with_capacity(reader.len() as usize);
        for item in reader.iter() {
            r.push(Annotation::from_reader(&item)?)
        }
        Ok(Annotations(r))
    }
}

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

pub struct Parameters(Vec<Parameter>);

impl Parameters {
    pub fn from_reader(
        reader: &capnp::struct_list::Reader<crate::schema_capnp::node::parameter::Owned>
    ) -> Result<Parameters, Error> {
        let mut r = Vec::with_capacity(reader.len() as usize);
        for x in reader.iter() {
            r.push(Parameter { name: x.get_name()?.into() });
        }
        Ok(Parameters(r))
    }
}

impl Deref for Parameters {
    type Target = Vec<Parameter>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

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
pub struct NestedNodes(Vec<(NodeName, NodeId)>);

impl NestedNodes {
    pub fn from_reader(
        reader: &capnp::struct_list::Reader<schema_capnp::node::nested_node::Owned>
    ) -> Result<NestedNodes, Error> {
        let mut r = Vec::with_capacity(reader.len() as usize);

        for item in reader.iter() {
            r.push((item.get_name()?.into(), item.get_id()));
        }

        Ok(NestedNodes(r))
    }
}

impl Deref for NestedNodes {
    type Target = Vec<(NodeName, NodeId)>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Clone)]
pub struct Node {
    id: NodeId,

    scope_id: Id,
    parameters: Parameters,
    is_generic: bool,
    // True if this node is generic, meaning that it or one of its parent scopes has a non-empty
    // `parameters`.

    nested: NestedNodes,

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
    AnyPointer(AnyPointerType),
}

impl Type {
    fn from_reader(
        reader: &schema_capnp::type_::Reader
    ) -> Result<Type, Error> {
        let r = match reader.which()? {
            schema_capnp::type_::Void(()) => Type::Void,
            schema_capnp::type_::Bool(()) => Type::Bool,
            schema_capnp::type_::Uint8(()) => Type::Uint8,
            schema_capnp::type_::Uint16(()) => Type::Uint16,
            schema_capnp::type_::Uint32(()) => Type::Uint32,
            schema_capnp::type_::Uint64(()) => Type::Uint64,
            schema_capnp::type_::Int8(()) => Type::Int8,
            schema_capnp::type_::Int16(()) => Type::Int16,
            schema_capnp::type_::Int32(()) => Type::Int32,
            schema_capnp::type_::Int64(()) => Type::Int64,
            schema_capnp::type_::Float32(()) => Type::Float32,
            schema_capnp::type_::Float64(()) => Type::Float64,
            schema_capnp::type_::Text(()) => Type::Text,
            schema_capnp::type_::Data(()) => Type::Data,

            schema_capnp::type_::List(x) => {
                let x: &schema_capnp::type_::list::Reader = &x;

                Type::List { element: Box::new(Type::from_reader(&x.get_element_type()?)?) }
            }
            schema_capnp::type_::Enum(x) => {
                let x: &schema_capnp::type_::enum_::Reader = &x;

                Type::Enum { id: x.get_type_id(), brand: Brand::from_reader(&x.get_brand()?)? }
            }
            schema_capnp::type_::Struct(x) => {
                let x: &schema_capnp::type_::struct_::Reader = &x;

                Type::Struct { id: x.get_type_id(), brand: Brand::from_reader(x.get_brand()?)? }
            }
            schema_capnp::type_::Interface(x) => {
                let x: &schema_capnp::type_::interface::Reader = &x;

                Type::Interface { id: x.get_type_id(), brand: Brand::from_reader(x.get_brand()?)? }
            }
            schema_capnp::type_::AnyPointer(x) => {
                let x: &schema_capnp::type_::any_pointer::Reader = &x;

                let type_ = match x.which()? {
                    schema_capnp::type_::any_pointer::Unconstrained(y) => {
                        let y: &schema_capnp::type_::any_pointer::unconstrained::Reader = &y;

                        match y.which()? {
                            schema_capnp::type_::any_pointer::unconstrained::AnyKind(()) => {
                                AnyPointerType::Any
                            }
                            schema_capnp::type_::any_pointer::unconstrained::Struct(()) => {
                                AnyPointerType::Struct
                            }
                            schema_capnp::type_::any_pointer::unconstrained::List(()) => {
                                AnyPointerType::List
                            }
                            schema_capnp::type_::any_pointer::unconstrained::Capability(()) => {
                                AnyPointerType::Capability
                            }
                        }
                    }
                    schema_capnp::type_::any_pointer::Parameter(y) => {
                        let y: &schema_capnp::type_::any_pointer::parameter::Reader = &y;

                        AnyPointerType::Parameter { scope_id: y.get_scope_id(), index: y.get_parameter_index() }
                    }
                    schema_capnp::type_::any_pointer::ImplicitMethodParameter(y) => {
                        let y: &schema_capnp::type_::any_pointer::implicit_method_parameter::Reader = &y;

                        AnyPointerType::ImplicitMethodParamater { index: y.get_parameter_index() }
                    }
                };

                Type::AnyPointer(type_)
            }
        };

        Ok(r)
    }
}

#[derive(Clone)]
pub enum AnyPointerType {
    Any,
    Struct,
    List,
    Capability,
    Parameter { scope_id: Id, index: u16 },
    ImplicitMethodParamater { index: u16 },
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
    List(),
    Interface(),
}

pub struct Arena {
    items: HashMap<NodeId, Node>,
}

impl Arena {
    pub fn from_reader(
        request: &schema_capnp::code_generator_request::Reader
    ) -> Result<Arena, Error> {
        let mut arena_items = HashMap::with_capacity(request.get_nodes()?.len() as usize);

        for node in request.get_nodes()? {
            match node.which()? {
                schema_capnp::node::Which::File(_) => {
                    let new_node = Node {
                        id: node.get_id(),
                        scope_id: node.get_scope_id(),
                        parameters: Parameters::from_reader(
                            &node.get_parameters()?
                        )?,
                        is_generic: node.get_is_generic(),
                        nested: NestedNodes::from_reader(
                            &node.get_nested_nodes()?
                        )?,
                        annotations: Annotations::from_reader(&node.get_annotations()?)?,
                        kind: match node.which()? {
                            schema_capnp::node::File(()) => NodeKind::File,
                            schema_capnp::node::Struct(x) => {
                                let x: &schema_capnp::node::struct_::Reader = &x;


                            }
                            schema_capnp::node::Enum(x) => {
                                let x: &schema_capnp::node::enum_::Reader = &x;
                            }
                            schema_capnp::node::Interface(x) => {
                                let x: &schema_capnp::node::interface::Reader = &x;


                            }
                            schema_capnp::node::Const(x) => {
                                let x: &schema_capnp::node::const_::Reader = &x;
                            }
                            schema_capnp::node::Annotation(x) => {
                                let x: &schema_capnp::node::annotation::Reader = &x;

                            }
                        }
                    };

                    arena_items.insert(node.get_id(), new_node);
                }
            }
        }

        Ok(Arena { items: arena_items })
    }
}