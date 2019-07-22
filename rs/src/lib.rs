use std::io::{Error as IoError, ErrorKind};
use pyo3::{create_exception, exceptions, PyObjectProtocol};
use pyo3::prelude::*;
use capnp::{serialize, Error as _CapnpError, NotInSchema, Word};
use capnp::serialize::OwnedSegments;
use std::process::Command;
use pyo3::types::{PyTuple, PyList, PyString, PyAny};
use std::path::{PathBuf, Path};
use capnpc::schema_capnp;
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;
use owning_ref::OwningHandle;
use std::any::Any;
use capnpc::codegen_types::RustTypeInfo;
use capnp::private::layout;
use pyo3::class::basic::PyObjectGetAttrProtocol;
use capnp::message::HeapAllocator;
use std::ops::{Deref, DerefMut};
use std::marker::PhantomData;
use crate::arena::Arena;

pub mod objs;
pub mod message;
pub mod arena;

create_exception!(wrapper, CapnpError, pyo3::exceptions::Exception);

pub enum Error {
    Capnp(_CapnpError),
    Io(IoError),
    NotInSchema(u16),
    Py(PyErr),
    Text(String),
    Type(String),
    Attribute(String),
}

impl From<_CapnpError> for Error {
    fn from(x: _CapnpError) -> Error {
        Error::Capnp(x)
    }
}

impl From<NotInSchema> for Error {
    fn from(x: NotInSchema) -> Error {
        Error::NotInSchema(x.0)
    }
}

impl From<IoError> for Error {
    fn from(x: IoError) -> Error {
        Error::Io(x)
    }
}

impl From<PyErr> for Error {
    fn from(x: PyErr) -> Error {
        Error::Py(x)
    }
}

impl From<Error> for PyErr {
    fn from(err: Error) -> PyErr {
        match err {
            Error::Capnp(x) => PyErr::new::<CapnpError, String>(x.to_string()),
            Error::Io(x) => PyErr::new::<exceptions::IOError, String>(x.to_string()),
            Error::NotInSchema(x) =>
                PyErr::new::<exceptions::TypeError, String>(
                    format!("Not in schema: {}", x)
                ),
            Error::Py(x) => x,
            Error::Text(x) => PyErr::new::<exceptions::Exception, String>(
                x
            ),
            Error::Type(x) => PyErr::new::<exceptions::TypeError, String>(
                x
            ),
            Error::Attribute(x) => PyErr::new::<exceptions::AttributeError, String>(
                x
            )
        }
    }
}

#[pyclass]
pub struct CompilerCommand {
    files: Vec<PathBuf>,
    src_prefixes: Vec<PathBuf>,
    import_paths: Vec<PathBuf>,
    no_standard_import: bool,
}

impl CompilerCommand {
    fn new(
        files: Vec<PathBuf>,
        src_prefixes: Vec<PathBuf>,
        import_paths: Vec<PathBuf>,
        no_standard_import: bool,
    ) -> CompilerCommand {
        CompilerCommand { files, src_prefixes, import_paths, no_standard_import }
    }

    fn build_command(&self) -> Command {
        let mut command = ::std::process::Command::new("capnp");
        command.arg("compile").arg("-o").arg("-");

        if self.no_standard_import {
            command.arg("--no-standard-import");
        }

        for import_path in &self.import_paths {
            command.arg(&format!("--import-path={}", import_path.display()));
        }

        for src_prefix in &self.src_prefixes {
            command.arg(&format!("--src-prefix={}", src_prefix.display()));
        }

        for file in &self.files {
            command.arg(&format!("{}", file.display()));
        }

        command.stdout(std::process::Stdio::piped());
        command.stderr(std::process::Stdio::inherit());

        return command;
    }

    pub fn compile(&self) -> PyResult<Definition> {
        fn inner(this: &CompilerCommand) -> Result<Definition, Error> {
            let mut cmd = this.build_command();
            let mut p = cmd.spawn()?;

            let mut reader = p.stdout.take().ok_or(IoError::from(ErrorKind::NotFound))?;

            let message: capnp::message::Reader<OwnedSegments> = serialize::read_message(&mut reader, capnp::message::ReaderOptions::new())?;
            let message = Box::new(message);

            let oref = OwningHandle::new_with_fn(
                message,
                unsafe {
                    |message| {
                        let root: schema_capnp::code_generator_request::Reader = (*message).get_root().unwrap();

                        let mut nodes = HashMap::with_capacity(root.get_nodes().unwrap().len() as usize);

                        for n in root.get_nodes().unwrap() {
                            nodes.insert(n.get_id(), n);
                        }

                        Box::new(ArenaItem {
                            definition: root.clone(),
                            nodes: nodes,
                        })
                    }
                },
            );


            let arena = NodeArena {
                items: oref
            };

            Ok(Definition {
                arena: Rc::new(arena),
            })
        }
        inner(self).map_err(PyErr::from)
    }
}

type Nodes<'a> = HashMap<u64, schema_capnp::node::Reader<'a>>;

pub struct ArenaItem<'a> {
    definition: schema_capnp::code_generator_request::Reader<'a>,
    nodes: Nodes<'a>,
}

pub struct NodeArena {
    items: OwningHandle<
        Box<capnp::message::Reader<OwnedSegments>>,
        Box<ArenaItem<'static>>
    >,
}




impl Arena for NodeArena {
    type Item = schema_capnp::node::Reader<'static>;

    fn get(&self, idx: &u64) -> Option<&Self::Item> {
        self.items.nodes.get(idx)
    }
}

#[pyclass]
pub struct Definition {
    arena: Rc<NodeArena>
}


#[derive(Clone)]
pub struct NodeInner {
    arena: Rc<NodeArena>,
    id: u64,
    nested: Vec<NodePy>,
}

#[pyclass]
#[derive(Clone)]
pub struct NodePy {
    i: NodeInner
}

impl NodeInner {
    // build field serialization first!

    fn get_reader<'a, 'b>(&'b self) -> &'b schema_capnp::node::Reader {
        self.arena.items.nodes.get(&self.id).unwrap()
    }

    fn which(&self) -> Result<(), Error> {
        // we need to build serializers and deserializers of structures.
        // (a) these should be incredibly similar to
        //     whatever the Rust compiler does.

        // we essentially are supposed to define fields in a certain
        // way.

        // basically, every node has a Reader and a Builder

        // we will concentrate on readers first to get the general
        // idea of what should be done


        // zero_fields_of_group initializes a group to a zero

        use schema_capnp::node::WhichReader as WhichNode;
        use schema_capnp::field::WhichReader as WhichField;
        let name = self.get_reader().get_display_name()?[
            self.get_reader().get_display_name_prefix_length() as usize..
            ].to_string();
        let id = self.type_id().clone();

        match self.get_reader().which()? {
            WhichNode::File(()) => {}
            WhichNode::Struct(x) => {
                let x: &schema_capnp::node::struct_::Reader = &x;

                let size = layout::StructSize {
                    data: x.get_data_word_count(),
                    pointers: x.get_pointer_count(),
                };

                x.get_preferred_list_encoding()?;

                x.get_is_group();
                x.get_discriminant_count();
                x.get_discriminant_offset();

                for field in x.get_fields()? {
                    field.get_name()?;
                    let code_order = field.get_code_order();

                    field.get_annotations()?;

                    let discriminant_value = field.get_discriminant_value();

                    let is_in_union = discriminant_value != schema_capnp::field::NO_DISCRIMINANT;


                    match field.which()? {
                        WhichField::Slot(x) => {
                            let x: &schema_capnp::field::slot::Reader = &x;
                            // capnp/src/codegen.rs:511
                            x.get_offset();
                            x.get_type()?;

                            let val = x.get_default_value()?;
                            match val.which()? {
                                schema_capnp::value::Struct(x) => {
                                    let x: &capnp::any_pointer::Reader = &x;

//                                    let words: Vec<Word> = x.get_as()?;
                                }
                                _ => {}
                            }
                            x.get_had_explicit_default();
                        }
                        WhichField::Group(x) => {
                            let x: &schema_capnp::field::group::Reader = &x;

                            let id = x.get_type_id();

                            // are there unnamed groups ?
                        }
                    }
                }
            }
            WhichNode::Enum(x) => {
                let x: &schema_capnp::node::enum_::Reader = &x;

                use itertools::Itertools;


                let items: Vec<(usize, String)> = x.get_enumerants()?
                    .iter()
                    .enumerate()
                    .sorted_by(
                        |a, b| Ord::cmp(&a.1.get_code_order(), &b.1.get_code_order())
                    )
                    .map(|x| x.1.get_name().map(|y| (x.0, y.to_string())))
                    .collect::<Result<Vec<_>, _>>()?;
            }
            WhichNode::Interface(x) => {
                let x: &schema_capnp::node::interface::Reader = &x;

                let methods = x.get_methods()?;

                for method in methods {
                    method.get_name()?;
                    method.get_code_order();
                    // Specifies order in which the methods were declared in the code.
                    // Like Struct.Field.codeOrder.

                    method.get_implicit_parameters()?;
                    // The parameters listed in [] (typically, type / generic parameters), whose bindings are intended
                    // to be inferred rather than specified explicitly, although not all languages support this.
                    method.get_param_struct_type();
                    // ID of the parameter struct type.  If a named parameter list was specified in the method
                    // declaration (rather than a single struct parameter type) then a corresponding struct type is
                    // auto-generated.  Such an auto-generated type will not be listed in the interface's
                    // `nestedNodes` and its `scopeId` will be zero -- it is completely detached from the namespace.
                    // (Awkwardly, it does of course inherit generic parameters from the method's scope, which makes
                    // this a situation where you can't just climb the scope chain to find where a particular
                    // generic parameter was introduced. Making the `scopeId` zero was a mistake.)

                    method.get_param_brand()?;

                    method.get_result_struct_type();

                    method.get_result_brand()?;

                    method.get_annotations()?;
                }

                let superclasses = x.get_superclasses()?;

                for superclass in superclasses {
                    let id = superclass.get_id();
                    let brand = superclass.get_brand()?;
                }
            }
            WhichNode::Const(x) => {
                let x: &schema_capnp::node::const_::Reader = &x;

                let val = x.get_value()?;
                let ty = x.get_type()?;

                ty.is_prim();
            }
            WhichNode::Annotation(x) => {
                let x: &schema_capnp::node::annotation::Reader = &x;
            }
        };

        Ok(())
    }
}

#[pymethods]
impl NodePy {
    fn __getattr__(&self, name: String) -> PyResult<impl IntoPyObject> {
        let name = name;

        let inner = |this: &NodeInner| -> Result<_, Error> {
            let me = this.arena.items.nodes.get(&this.id).ok_or(Error::Text("could not find me".to_string()))?;
            let nested = me.get_nested_nodes()?;
            println!("... {}", name);

            for x in nested {
                println!("> {}", x.get_name()?);
                if name == x.get_name()? {
                    return Ok(NodePy { i: NodeInner { arena: this.arena.clone(), id: x.get_id(), nested: Vec::new() } });
                }
            }

            Err(Error::Attribute(name))
        };

        inner(&self.i).map_err(PyErr::from)
    }

    fn children(&self) -> PyResult<Vec<String>> {
        let inner = |this: &NodeInner| -> Result<Vec<String>, Error> {
            let me = this.arena.items.nodes.get(&this.id).ok_or(Error::Text("could not find me".to_string()))?;
            let nested = me.get_nested_nodes()?;

            let mut children = Vec::with_capacity(nested.len() as usize);

            for x in nested {
                children.push(x.get_name()?.to_string());
            }

            Ok(children)
        };

        inner(&self.i).map_err(PyErr::from)
    }

    fn __str__(&self, _py: Python) -> PyResult<String> {
        self.__repr__(_py)
    }
    fn __repr__(&self, _py: Python) -> PyResult<String> {
        let inner = |this: &NodeInner| -> Result<String, Error> {
            let mut b = String::with_capacity(1024);

            let reader = this.arena.items.nodes.get(&this.id).unwrap();
            let prefix = reader.get_display_name_prefix_length() as usize;

            b.push_str(reader.get_display_name()?[prefix..].as_ref());
            b.push_str("(");

            let first = true;
            let mut second = false;

            for x in this.nested.iter().map(|x| x.__repr__(_py)) {
                let x = x?;

                if second {
                    b.push_str(", ");
                }

                b.push_str(x.as_ref());

                if first {
                    second = true;
                }
            }
            b.push_str(")");

            Ok(b)
        };

        inner(&self.i).map_err(PyErr::from)
    }
}

#[pyproto]
impl PyObjectProtocol for NodePy {
    fn __getattr__(&self, name: String) -> PyResult<NodePy>
    {
        let inner = |this: &NodeInner| -> Result<_, Error> {
            let me = this.arena.items.nodes.get(&this.id).ok_or(Error::Text("could not find me".to_string()))?;
            let nested = me.get_nested_nodes()?;

            for x in nested {
                if name == x.get_name()? {
                    return Ok(NodePy { i: NodeInner { arena: this.arena.clone(), id: x.get_id(), nested: Vec::new() } });
                }
            }

            Err(Error::Attribute(name))
        };

        inner(&self.i).map_err(PyErr::from)
    }
}

#[pymethods]
impl Definition {
    #[getter]
    fn id(&self, _py: Python) -> PyResult<Vec<NodePy>> {
        fn inner(this: &Definition) -> Result<Vec<NodePy>, Error> {
            let mut to_visit: VecDeque<(u64, u64)> = VecDeque::new();

            let mut waiting: HashMap<u64, u32> = HashMap::with_capacity(this.arena.items.nodes.len());
            let mut readers: HashMap<u64, &schema_capnp::node::Reader> = HashMap::with_capacity(this.arena.items.nodes.len());
            let mut nesters: HashMap<u64, Vec<u64>> = HashMap::with_capacity(this.arena.items.nodes.len());
            let mut done: HashMap<u64, NodePy> = HashMap::with_capacity(this.arena.items.nodes.len());
            let mut roots: Vec<u64> = Vec::new();

            let visit_node = |waiting: &mut HashMap<u64, u32>,
                              nesters: &mut HashMap<u64, Vec<u64>>,
                              to_visit: &mut VecDeque<(u64, u64)>,
                              readers: &mut HashMap<u64, &schema_capnp::node::Reader>, item_id: u64| -> Result<(), Error> {
                let v = readers.get(&item_id).unwrap();
                let id = v.get_id();
                let nested_nodes = v.get_nested_nodes()?;
                waiting.insert(id, nested_nodes.len());
                let mut nesters_curr = Vec::new();
                for nested in nested_nodes {
                    to_visit.push_front((id, nested.get_id()));
                    nesters_curr.push(nested.get_id());
                }
                nesters.insert(id, nesters_curr);

                Ok(())
            };

            for (_k, v) in this.arena.items.nodes.iter() {
                readers.insert(v.get_id(), v);
                match v.which()? {
                    schema_capnp::node::File(_x) => {
                        visit_node(&mut waiting, &mut nesters, &mut to_visit, &mut readers, v.get_id())?;
                        roots.push(v.get_id());
                    }
                    _ => {}
                }
            };

            let mut build_par = |nesters: &mut HashMap<u64, Vec<u64>>,
                                 waiting: &mut HashMap<u64, u32>,
                                 parent_id: u64| -> Result<(), Error> {
                //let rdr = readers.remove(&parent_id).unwrap();
                waiting.remove(&parent_id);

                let nesters_curr = nesters.remove(&parent_id).unwrap();
                let mut nested_set: Vec<NodePy> = Vec::with_capacity(nesters_curr.len());

                for x in nesters_curr {
                    let y = done.get(&x).unwrap();
                    nested_set.push(y.clone());
                }

                done.insert(
                    parent_id,
                    NodePy { i: NodeInner { id: parent_id, arena: this.arena.clone(), nested: nested_set } },
                );

                Ok(())
            };

            while let Some((parent_id, item_id)) = to_visit.pop_front() {
                visit_node(&mut waiting, &mut nesters, &mut to_visit, &mut readers, item_id)?;

                if {
                    let ctr = waiting.get_mut(&item_id).unwrap();
                    *ctr == 0
                } {
                    build_par(&mut nesters, &mut waiting, item_id)?;
                }

                if {
                    let ctr = waiting.get_mut(&parent_id).unwrap();
                    *ctr -= 1;
                    *ctr == 0
                } {
                    build_par(&mut nesters, &mut waiting, parent_id)?;
                }
            }

            let mut rtn = Vec::with_capacity(roots.len());

            for x in roots {
                let a = done.remove(&x).unwrap();
                rtn.push(a);
            }

            return Ok(rtn);
        }

        inner(self).map_err(PyErr::from)
    }
}

//pub mod empty {
//    pub struct DynamicBuilder<'a> {
//        builder: ::capnp::private::layout::StructBuilder<'a>,
//        struct_size: ::capnp::private::layout::StructSize,
//        type_id: u64,
//    }
//    impl <'a,> ::capnp::traits::HasStructSize for DynamicBuilder<'a,>  {
//        #[inline]
//        fn struct_size() -> ::capnp::private::layout::StructSize { _private::STRUCT_SIZE }
//    }
//    impl <'a,> ::capnp::traits::HasTypeId for DynamicBuilder<'a,>  {
//        #[inline]
//        fn type_id() -> u64 { _private::TYPE_ID }
//    }
//    // #fromStructBuilder
////    impl <'a,> ::capnp::traits::FromStructBuilder<'a> for DynamicBuilder<'a,>  {
////        fn new(builder: ::capnp::private::layout::StructBuilder<'a>) -> DynamicBuilder<'a, > {
////            builder.
////            DynamicBuilder { builder: builder,  }
////        }
////    }
//
//    impl <'a,> ::capnp::traits::ImbueMut<'a> for DynamicBuilder<'a,>  {
//        fn imbue_mut(&mut self, cap_table: &'a mut ::capnp::private::layout::CapTable) {
//            self.builder.imbue(::capnp::private::layout::CapTableBuilder::Plain(cap_table))
//        }
//    }
//
//    impl <'a,> ::capnp::traits::FromPointerBuilder<'a> for DynamicBuilder<'a,>  {
//        fn init_pointer(builder: ::capnp::private::layout::PointerBuilder<'a>, _size: u32) -> DynamicBuilder<'a,> {
//            // calls #fromStructBuilder
//            ::capnp::traits::FromStructBuilder::new(builder.init_struct(_private::STRUCT_SIZE))
//        }
//        fn get_from_pointer(builder: ::capnp::private::layout::PointerBuilder<'a>, default: Option<&'a [::capnp::Word]>) -> ::capnp::Result<DynamicBuilder<'a,>> {
//            ::std::result::Result::Ok(::capnp::traits::FromStructBuilder::new(builder.get_struct(_private::STRUCT_SIZE, default)?))
//        }
//    }
//}




#[pyclass]
struct Builder {
    struct_size: ::capnp::private::layout::StructSize,
    type_id: u64,
}

#[pymethods]
impl Builder {
    //fn goofy(&mut self) -> PyResult<bool> {
//        use capnp::private::arena::BuilderArena;
//        use capnp::any_pointer;
//
//        //let root = self.i.init_root().unwrap();
//
//        if self.arena.len() == 0 {
//            self.arena.allocate_segment(1).expect("allocate root pointer");
//            self.arena.allocate(0, 1).expect("allocate root pointer");
//        }
//        let (seg_start, _seg_len) = self.arena.get_segment_mut(0);
//        let location: *mut Word = seg_start;
//        let Builder { ref mut arena, struct_size, type_id } = *self;
//
//        let pointer_builder = layout::PointerBuilder::get_root(arena, 0, location);
//
//        let root = any_pointer::Builder::new(pointer_builder);
//
//        let struct_inited = pointer_builder.init_struct(self.struct_size);
//
//        self.arena.get_segments_for_output();
//
//        return Ok(true);
    //}
}

#[pyclass]
struct CompileFun {}

#[pymethods]
impl CompileFun {
    #[call]
    #[args(files = "*", no_standard_import = false)]
    fn compile(
        &self,
        files: &PyTuple,
        src_prefixes: Option<&PyList>,
        import_paths: Option<&PyList>,
        no_standard_import: bool,
    ) -> PyResult<Definition> {
        let mut _files: Vec<PathBuf> = Vec::new();
        let mut _src_prefixes: Vec<PathBuf> = Vec::new();
        let mut _import_paths: Vec<PathBuf> = Vec::new();

        for f in files {
            let f: String = f.downcast_ref::<PyString>()?.to_string()?.to_string();
            _files.push(Path::new(&f).to_path_buf());
        }

        if let Some(xs) = src_prefixes {
            for x in xs {
                let f = x.downcast_ref::<PyString>()?.to_string()?.to_string();

                _src_prefixes.push(Path::new(&f).to_path_buf());
            }
        }

        if let Some(xs) = import_paths {
            for x in xs {
                let f = x.downcast_ref::<PyString>()?.to_string()?.to_string();

                _import_paths.push(Path::new(&f).to_path_buf());
            }
        }

        Ok(
            CompilerCommand::new(
                _files,
                _src_prefixes,
                _import_paths,
                no_standard_import,
            ).compile()?
        )
    }
}


#[pymodule]
fn wrapper(_py: Python, m: &PyModule) -> PyResult<()> {
    //m.add_class::<CompileFun>()?;
    m.add("compile", PyRef::new(_py, CompileFun {})?)?;
    m.add_class::<Definition>()?;
    m.add_class::<NodePy>()?;
    Ok(())
}
