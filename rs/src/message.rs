use capnp::any_pointer::Builder as AnyPointerBuilder;
use capnp::private::layout::{PointerBuilder, PointerReader, StructBuilder, StructSize, ListBuilder};
use capnp::private::arena::{BuilderArena as _BuilderArena, BuilderArenaImpl};
use capnp::message::HeapAllocator;
use capnp::Word;

pub use super::NodeArena as NodeArena;
use std::rc::Rc;
use std::collections::HashMap;
use capnpc::schema_capnp;
use owning_ref::OwningHandle;
use crate::arena::{Arena, ArenaRef, ArenaRc};
use crate::Error;
use capnpc::codegen_types::RustTypeInfo;

#[derive(Clone)]
pub enum Path {
    // structure at which pointer is selected
    Enum(usize, ArenaRef<NodeArena>),
    Struct(usize, ArenaRef<NodeArena>),
    // which Which is selected
    Integer(usize),
    Bool(usize),
    Float(usize),
    Void,

    Which(usize, ArenaRef<NodeArena>),
    Group(u32),
    // list at which pointer is selected
    List(usize),
    Index(usize),

    Data(usize),
    Text(usize),
}

pub struct PathBuilder {
    root: ArenaRef<NodeArena>,
    path: Vec<Path>,
}

type BA = BuilderArenaImpl<HeapAllocator>;

fn get_node_struct_size(node: &schema_capnp::node::Reader) -> Result<StructSize, Error> {
    match node.which()? {
        schema_capnp::node::Which::Struct(x) => {
            let x: &schema_capnp::node::struct_::Reader = &x;

            Ok(StructSize {
                data: x.get_data_word_count(),
                pointers: x.get_pointer_count(),
            })
        }
        _ => Err(Error::Text("invalid node type".into()))
    }
}

impl PathBuilder {
    pub fn with_append(&self, path: Path) -> Self {
        let mut new_path = self.path.clone();
        new_path.push(path);
        return PathBuilder { path: new_path, root: self.root.clone() };
    }

    pub fn struct_current(&self) -> Result<&ArenaRef<NodeArena>, Error> {
        let mut root = &self.root;
        let mut iter = self.path.iter();

        while let Some(item) = iter.next() {
            match item {
                Path::Struct(_, x) => {
                    root = &x;
                }
                Path::Which(_, x) => {
                    root = &x;
                }
                _ => {
                    return Err(Error::Type("not a struct [2]".into()));
                }
            }
        }

        Ok(root)
    }

    pub fn struct_field(&self, name: &str) -> Result<PathBuilder, Error> {
        let branch = self.struct_current()?;

        match branch.which()? {
            schema_capnp::node::Which::Struct(x) => {
                let x: &schema_capnp::node::struct_::Reader = &x;

                if name == "which" {}

                for field in x.get_fields()? {
                    if field.get_discriminant_value() != schema_capnp::field::NO_DISCRIMINANT {
                        continue;
                    }

                    if name != field.get_name()? {
                        continue;
                    }

                    match field.which()? {
                        schema_capnp::field::Slot(y) => {
                            let y: &schema_capnp::field::slot::Reader = &y;
                            let type_ = y.get_type()?;

                            match type_.which()? {
                                schema_capnp::type_::Int8(_) |
                                schema_capnp::type_::Int16(_) |
                                schema_capnp::type_::Int32(_) |
                                schema_capnp::type_::Int64(_) |
                                schema_capnp::type_::Uint8(_) |
                                schema_capnp::type_::Uint16(_) |
                                schema_capnp::type_::Uint32(_) |
                                schema_capnp::type_::Uint64(_)
                                => {
                                    return Ok(self.with_append(Path::Integer(
                                        y.get_offset() as usize
                                    )));
                                }
                                schema_capnp::type_::Float32(_) |
                                schema_capnp::type_::Float64(_) => {
                                    return Ok(self.with_append(Path::Float(
                                        y.get_offset() as usize
                                    )));
                                }
                                schema_capnp::type_::Void(_) => {
                                    return Ok(self.with_append(Path::Void));
                                }
                                schema_capnp::type_::Text(_) => {
                                    return Ok(self.with_append(Path::Text(
                                        y.get_offset() as usize
                                    )));
                                }
                                schema_capnp::type_::Data(_) => {
                                    return Ok(self.with_append(Path::Data(
                                        y.get_offset() as usize
                                    )));
                                }
                                schema_capnp::type_::Bool(_) => {
                                    return Ok(self.with_append(Path::Bool(
                                        y.get_offset() as usize
                                    )));
                                }
                                schema_capnp::type_::Struct(z) => {
                                    let z: &schema_capnp::type_::struct_::Reader = &z;

                                    return Ok(self.with_append(Path::Struct(
                                        y.get_offset() as usize,
                                        self.root.arena().get_ref(&z.get_type_id()).ok_or(
                                            Error::Type("could not find enum value [3]".into())
                                        )?,
                                    )));
                                }
                                schema_capnp::type_::List(z) => {
                                    let z: &schema_capnp::type_::list::Reader = &z;

                                    return Ok(self.with_append(Path::List(
                                        y.get_offset() as usize
                                    )));
                                }
                                schema_capnp::type_::Enum(z) => {
                                    let z: &schema_capnp::type_::enum_::Reader = &z;

                                    return Ok(self.with_append(Path::Enum(
                                        y.get_offset() as usize,
                                        self.root.arena().get_ref(&z.get_type_id()).ok_or(
                                            Error::Type("could not find enum value [3]".into())
                                        )?,
                                    )));
                                }
                            }

                            //return Ok(self.with_append(Path::))
                        }
                        schema_capnp::field::Group(y) => {
                            let y: &schema_capnp::field::group::Reader = &y;
                            //
                        }
                    }
                }

                Err(Error::Text("unavailable".into()))
            }
            _ => {
                Err(Error::Type("not a struct [3]".into()))
            }
        }
    }

    pub fn list_index(&self, index: usize) -> Result<PathBuilder, Error> {
        // we need to find where List is located in the parent so that we could re-access it.
        if let Some(Path::List(usize)) = self.path.last() {
            Ok(self.with_append(Path::Index(index)))
        } else {
            Err(Error::Type("not a list [1]".into()))
        }
    }

    // this needs to return a value or something
    pub fn get(&self, arena: &mut BA) -> Result<(), Error> {
        let size = get_node_struct_size(&self.root)?;

        let (seg_start, _seg_len) = arena.get_segment_mut(0);
        let location: *mut Word = seg_start;
        //let Builder { ref mut arena } = *self;


        let root = PointerBuilder::get_root(arena, 0, location);
        let mut root = root.get_struct(size, None)?;

        for x in self.path.iter() {
            match x {
                Path::Struct(offset, node) => {
                    let struct_builder = root.get_pointer_field(*offset).get_struct(
                        get_node_struct_size(node)?,
                        None,
                    )?;

                    //struct_builder.
                }
                _ => {}
            }
        }


        // root.get
        Ok(())
    }
}

pub enum Type<'a> {
    Struct(StructBuilder<'a>),
    List(ListBuilder<'a>),
}

pub struct BuilderArena<'a> {
    //        items: OwningHandle<
//            Box<BuilderArenaImpl<HeapAllocator>>,
//            Box<HashMap<u64, Type<'static>>>
//        >,
    arena: BuilderArenaImpl<HeapAllocator>,
    items: Box<HashMap<u64, Type<'a>>>,
    next_idx: u64,
}

impl BuilderArena<'_> {
    pub fn new() -> Self {
        let arena = BuilderArenaImpl::new(HeapAllocator::new());
//            let items = OwningHandle::new_with_fn(
//                Box::new(arena),
//                unsafe {
//                    |arena| {
//                        Box::new(HashMap::new())
//                    }
//                },
//            );
        //let arena = Box::new(arena);
        let items = Box::new(HashMap::new());

//            BuilderArena { items, next_idx: 0 }
        BuilderArena { arena, items, next_idx: 0 }
    }

//        pub fn arena(&self) -> &mut BuilderArenaImpl<HeapAllocator> {
//            self.items.as_owner()
//        }
}

impl<'a> Arena for BuilderArena<'a> {
    type Item = Type<'a>;

    fn get(&self, idx: &u64) -> Option<&Self::Item> {
        self.items.get(idx)
    }
}


pub struct Building<'a> {
    node: ArenaRef<NodeArena>,
    builder: ArenaRef<BuilderArena<'a>>,
}

//    impl Building<'_> {
//        pub fn new_root<'a>(
//            builder: &'a mut ArenaRc<BuilderArena<'a>>,
//            node: ArenaRef<NodeArena>,
//        ) -> ArenaRef<BuilderArena<'a>> {
//            let mut arena = &mut builder.arena;
//
//            if arena.len() == 0 {
//                arena.allocate_segment(1).expect("allocate root pointer");
//                arena.allocate(0, 1).expect("allocate root pointer");
//            }
//            let (seg_start, _seg_len) = arena.get_segment_mut(0);
//            let location: *mut Word = seg_start;
//            //let Builder { ref mut arena } = *self;
//
//
//            let root = PointerBuilder::get_root(&mut builder.arena, 0, location);
//
//
//            let struct_builder = root.init_struct({
//                match node.which().expect("must exist") {
//                    schema_capnp::node::Which::Struct(x) => {
//                        let x: &schema_capnp::node::struct_::Reader = &x;
//
//                        StructSize {
//                            data: x.get_data_word_count(),
//                            pointers: x.get_pointer_count(),
//                        }
//                    },
//                    _ => unreachable!("this node is not a struct")
//                }
//            });
//
////            struct_builder.get_pointer_field(0).get_struct()
//
//            let next_idx = builder.next_idx;
//            builder.next_idx += 1;
//
//            builder.items.insert(next_idx, Type::<'a>::Struct(struct_builder));
//
//            builder.get_ref(&next_idx).unwrap()
//        }
//    }

//    pub struct StructBuilderA {
//        i: StructBuilder<'static>,
//    }
//
//    impl StructBuilderA {
//        pub fn new(i: StructBuilder) -> StructBuilderA {
//            StructBuilderA { i }
//        }
//    }

pub struct Builder {
    arena: BuilderArenaImpl<HeapAllocator>,
    node_arena: Rc<NodeArena>,
    node_id: u64,
    initialized: bool,
}

impl Builder {
    pub fn new(id: u64, arena: Rc<NodeArena>) -> Self {
        Builder {
            arena: BuilderArenaImpl::new(HeapAllocator::new()),
            node_arena: arena.clone(),
            node_id: id,
            initialized: false,
        }
    }

    fn me<'a, 'b>(&'a self) -> schema_capnp::node::struct_::Reader<'b> {
        let node = self.node_arena.items.nodes.get(&self.node_id).expect("this must exist");

        match node.which().expect("this must exist, too") {
            schema_capnp::node::Which::Struct(x) => {
                //let x: &schema_capnp::node::struct_::Reader = &x;

                x
            }
            _x => {
                panic!("3")
            }
        }
    }

    fn struct_size(&self) -> StructSize {
        let x = self.me();
        StructSize {
            data: x.get_data_word_count(),
            pointers: x.get_pointer_count(),
        }
    }

    fn find_field<'a, 'b>(&'a self, name: String) -> Option<schema_capnp::field::Reader<'b>> {
        let x = self.me();

        for field in x.get_fields().expect("struct must have fields") {
            if field.get_name().expect("field must have a name") == name {
                return Some(field);
            }
        }
        return None;
    }

//        fn get_root_internal(&mut self) -> StructBuilder {
//            if self.arena.len() == 0 {
//                self.arena.allocate_segment(1).expect("allocate root pointer");
//                self.arena.allocate(0, 1).expect("allocate root pointer");
//            }
//            let (seg_start, _seg_len) = self.arena.get_segment_mut(0);
//            let location: *mut Word = seg_start;
//            //let Builder { ref mut arena } = *self;
//
//
//            let root = PointerBuilder::get_root(&mut self.arena, 0, location);
//
//            let ss = self.struct_size();
//
//            let r = if self.initialized {
//                root.get_struct(ss, None).expect("this must have been initialized already")
//            } else {
//                root.init_struct(ss)
//            };
//
//            r
//        }

//        pub fn get_field(&self, name: String) -> Option<u32> {
//            //self.me().get_discriminant_offset()
//            if let Some(field) = self.find_field(name) {
//                match field.which().expect("must have a which") {
//                    schema_capnp::field::Which::Slot(x) => {
//                        let x: &schema_capnp::field::slot::Reader = &x;
//                    }
//                    _ => {}
//                }
//
//                Some(0)
//            } else {
//                None
//            }
//        }
//
//        pub fn set_field(&mut self, name: String, value: u8) {
//            let x = self.me();
//            //let field = self.find_field(name);
//
//            for field in x.get_fields().expect("struct must have fields") {
//                if field.get_name().expect("field must have a name") == name {
//                    match field.which().expect("field must have a kind") {
//                        schema_capnp::field::Which::Slot(x) => {
//                            let x: &schema_capnp::field::slot::Reader<'static> = &x;
//                            let type_ = x.get_type().expect("field must have a type");
//
//
//                            match type_.which().expect("which needs to exist") {
//                                schema_capnp::type_::Int8(_) => {
//                                    //break Some(())
//                                    self.get_root_internal().set_data_field(
//                                        x.get_offset() as usize,
//                                        value,
//                                    );
//                                }
//                                schema_capnp::type_::AnyPointer(_) => {
//                                    self.get_root_internal().get_pointer_field(
//                                        x.get_offset() as usize
//                                    );
//                                }
//                                _ => {
//                                    panic!("1")
//                                }
//                            }
//                        }
//                        schema_capnp::field::Which::Group(x) => {
//                            let x: &schema_capnp::field::group::Reader = &x;
//
//                            x.get_type_id();
//                            panic!("2")
//                        }
//                    }
//                }
//            }
//        }
}

pub struct DynamicStructDef {
    struct_size: ::capnp::private::layout::StructSize,
    type_id: u64,
}