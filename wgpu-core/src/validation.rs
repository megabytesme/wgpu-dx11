use alloc::{
    boxed::Box,
    string::{String, ToString as _},
    sync::Arc,
    vec::Vec,
};
use core::fmt;

use arrayvec::ArrayVec;
use hashbrown::{hash_map::Entry, HashSet};
use shader_io_deductions::{display_deductions_as_optional_list, MaxVertexShaderOutputDeduction};
use thiserror::Error;
use wgt::{
    error::{ErrorType, WebGpuError},
    BindGroupLayoutEntry, BindingType,
};

use crate::{
    command::ColorAttachmentError, device::bgl, resource::InvalidResourceError,
    validation::shader_io_deductions::MaxFragmentShaderInputDeduction, FastHashMap, FastHashSet,
};

pub mod shader_io_deductions;

#[derive(Debug)]
enum ResourceType {
    Buffer {
        size: wgt::BufferSize,
    },
    Texture {
        dim: naga::ImageDimension,
        arrayed: bool,
        class: naga::ImageClass,
    },
    Sampler {
        comparison: bool,
    },
    AccelerationStructure {
        vertex_return: bool,
    },
}

#[derive(Clone, Debug)]
pub enum BindingTypeName {
    Buffer,
    Texture,
    Sampler,
    AccelerationStructure,
    ExternalTexture,
}

impl From<&ResourceType> for BindingTypeName {
    fn from(ty: &ResourceType) -> BindingTypeName {
        match ty {
            ResourceType::Buffer { .. } => BindingTypeName::Buffer,
            ResourceType::Texture {
                class: naga::ImageClass::External,
                ..
            } => BindingTypeName::ExternalTexture,
            ResourceType::Texture { .. } => BindingTypeName::Texture,
            ResourceType::Sampler { .. } => BindingTypeName::Sampler,
            ResourceType::AccelerationStructure { .. } => BindingTypeName::AccelerationStructure,
        }
    }
}

impl From<&BindingType> for BindingTypeName {
    fn from(ty: &BindingType) -> BindingTypeName {
        match ty {
            BindingType::Buffer { .. } => BindingTypeName::Buffer,
            BindingType::Texture { .. } => BindingTypeName::Texture,
            BindingType::StorageTexture { .. } => BindingTypeName::Texture,
            BindingType::Sampler { .. } => BindingTypeName::Sampler,
            BindingType::AccelerationStructure { .. } => BindingTypeName::AccelerationStructure,
            BindingType::ExternalTexture => BindingTypeName::ExternalTexture,
        }
    }
}

#[derive(Debug)]
struct Resource {
    #[allow(unused)]
    name: Option<String>,
    bind: naga::ResourceBinding,
    ty: ResourceType,
    class: naga::AddressSpace,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NumericDimension {
    Scalar,
    Vector(naga::VectorSize),
    Matrix(naga::VectorSize, naga::VectorSize),
}

impl fmt::Display for NumericDimension {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Self::Scalar => write!(f, ""),
            Self::Vector(size) => write!(f, "x{}", size as u8),
            Self::Matrix(columns, rows) => write!(f, "x{}{}", columns as u8, rows as u8),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NumericType {
    dim: NumericDimension,
    scalar: naga::Scalar,
}

impl fmt::Display for NumericType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{:?}{}{}",
            self.scalar.kind,
            self.scalar.width * 8,
            self.dim
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InterfaceVar {
    pub ty: NumericType,
    interpolation: Option<naga::Interpolation>,
    sampling: Option<naga::Sampling>,
    per_primitive: bool,
}

impl InterfaceVar {
    pub fn vertex_attribute(format: wgt::VertexFormat) -> Self {
        InterfaceVar {
            ty: NumericType::from_vertex_format(format),
            interpolation: None,
            sampling: None,
            per_primitive: false,
        }
    }
}

impl fmt::Display for InterfaceVar {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{} interpolated as {:?} with sampling {:?}",
            self.ty, self.interpolation, self.sampling
        )
    }
}

#[derive(Debug, Eq, PartialEq)]
enum Varying {
    Local { location: u32, iv: InterfaceVar },
    BuiltIn(BuiltIn),
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum BuiltIn {
    Position { invariant: bool },
    ViewIndex,
    BaseInstance,
    BaseVertex,
    ClipDistances { array_size: u32 },
    CullDistance,
    InstanceIndex,
    PointSize,
    VertexIndex,
    DrawIndex,
    FragDepth,
    PointCoord,
    FrontFacing,
    PrimitiveIndex,
    Barycentric { perspective: bool },
    SampleIndex,
    SampleMask,
    GlobalInvocationId,
    LocalInvocationId,
    LocalInvocationIndex,
    WorkGroupId,
    WorkGroupSize,
    NumWorkGroups,
    NumSubgroups,
    SubgroupId,
    SubgroupSize,
    SubgroupInvocationId,
    MeshTaskSize,
    CullPrimitive,
    PointIndex,
    LineIndices,
    TriangleIndices,
    VertexCount,
    Vertices,
    PrimitiveCount,
    Primitives,
    RayInvocationId,
    NumRayInvocations,
    InstanceCustomData,
    GeometryIndex,
    WorldRayOrigin,
    WorldRayDirection,
    ObjectRayOrigin,
    ObjectRayDirection,
    RayTmin,
    RayTCurrentMax,
    ObjectToWorld,
    WorldToObject,
    HitKind,
}

impl BuiltIn {
    pub fn to_naga(&self) -> naga::BuiltIn {
        match self {
            &Self::Position { invariant } => naga::BuiltIn::Position { invariant },
            Self::ViewIndex => naga::BuiltIn::ViewIndex,
            Self::BaseInstance => naga::BuiltIn::BaseInstance,
            Self::BaseVertex => naga::BuiltIn::BaseVertex,
            Self::ClipDistances { .. } => naga::BuiltIn::ClipDistances,
            Self::CullDistance => naga::BuiltIn::CullDistance,
            Self::InstanceIndex => naga::BuiltIn::InstanceIndex,
            Self::PointSize => naga::BuiltIn::PointSize,
            Self::VertexIndex => naga::BuiltIn::VertexIndex,
            Self::DrawIndex => naga::BuiltIn::DrawIndex,
            Self::FragDepth => naga::BuiltIn::FragDepth,
            Self::PointCoord => naga::BuiltIn::PointCoord,
            Self::FrontFacing => naga::BuiltIn::FrontFacing,
            Self::PrimitiveIndex => naga::BuiltIn::PrimitiveIndex,
            Self::Barycentric { perspective } => naga::BuiltIn::Barycentric {
                perspective: *perspective,
            },
            Self::SampleIndex => naga::BuiltIn::SampleIndex,
            Self::SampleMask => naga::BuiltIn::SampleMask,
            Self::GlobalInvocationId => naga::BuiltIn::GlobalInvocationId,
            Self::LocalInvocationId => naga::BuiltIn::LocalInvocationId,
            Self::LocalInvocationIndex => naga::BuiltIn::LocalInvocationIndex,
            Self::WorkGroupId => naga::BuiltIn::WorkGroupId,
            Self::WorkGroupSize => naga::BuiltIn::WorkGroupSize,
            Self::NumWorkGroups => naga::BuiltIn::NumWorkGroups,
            Self::NumSubgroups => naga::BuiltIn::NumSubgroups,
            Self::SubgroupId => naga::BuiltIn::SubgroupId,
            Self::SubgroupSize => naga::BuiltIn::SubgroupSize,
            Self::SubgroupInvocationId => naga::BuiltIn::SubgroupInvocationId,
            Self::MeshTaskSize => naga::BuiltIn::MeshTaskSize,
            Self::CullPrimitive => naga::BuiltIn::CullPrimitive,
            Self::PointIndex => naga::BuiltIn::PointIndex,
            Self::LineIndices => naga::BuiltIn::LineIndices,
            Self::TriangleIndices => naga::BuiltIn::TriangleIndices,
            Self::VertexCount => naga::BuiltIn::VertexCount,
            Self::Vertices => naga::BuiltIn::Vertices,
            Self::PrimitiveCount => naga::BuiltIn::PrimitiveCount,
            Self::Primitives => naga::BuiltIn::Primitives,
            Self::RayInvocationId => naga::BuiltIn::RayInvocationId,
            Self::NumRayInvocations => naga::BuiltIn::NumRayInvocations,
            Self::InstanceCustomData => naga::BuiltIn::InstanceCustomData,
            Self::GeometryIndex => naga::BuiltIn::GeometryIndex,
            Self::WorldRayOrigin => naga::BuiltIn::WorldRayOrigin,
            Self::WorldRayDirection => naga::BuiltIn::WorldRayDirection,
            Self::ObjectRayOrigin => naga::BuiltIn::ObjectRayOrigin,
            Self::ObjectRayDirection => naga::BuiltIn::ObjectRayDirection,
            Self::RayTmin => naga::BuiltIn::RayTmin,
            Self::RayTCurrentMax => naga::BuiltIn::RayTCurrentMax,
            Self::ObjectToWorld => naga::BuiltIn::ObjectToWorld,
            Self::WorldToObject => naga::BuiltIn::WorldToObject,
            Self::HitKind => naga::BuiltIn::HitKind,
        }
    }
}

#[allow(unused)]
#[derive(Debug)]
struct SpecializationConstant {
    id: u32,
    ty: NumericType,
}

#[derive(Debug)]
struct EntryPointMeshInfo {
    max_vertices: u32,
    max_primitives: u32,
    primitive_topology: wgt::PrimitiveTopology,
}

#[derive(Debug, Default)]
struct EntryPoint {
    inputs: Vec<Varying>,
    outputs: Vec<Varying>,
    fragment_position_used: bool,
    texture_load_used: bool,
    unsupported_runtime_integer_semantics: bool,
    unsupported_matrix_aggregate_store: bool,
    texture_query_used: bool,
    texture_sample_base_clamp_to_edge_used: bool,
    resources: Vec<naga::Handle<Resource>>,
    uniform_constant_vectors: u32,
    #[allow(unused)]
    spec_constants: Vec<SpecializationConstant>,
    sampling_pairs: FastHashSet<(naga::Handle<Resource>, naga::Handle<Resource>)>,
    texture_sample_biases: Vec<TextureSampleBias>,
    fragment_derivatives_used: bool,
    texture_sample_level_used: bool,
    texture_gather_used: bool,
    workgroup_size: [u32; 3],
    dual_source_blending: bool,
    task_payload_size: Option<u32>,
    mesh_info: Option<EntryPointMeshInfo>,
    immediate_slots_required: naga::valid::ImmediateSlots,
}

#[derive(Clone, Copy, Debug)]
struct TextureSampleBias {
    literal: Option<f32>,
    has_offset: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub(crate) struct SamplingPair {
    pub texture_group: u32,
    pub texture_binding: u32,
    pub sampler_group: u32,
    pub sampler_binding: u32,
}

#[derive(Debug)]
pub struct Interface {
    limits: wgt::Limits,
    resources: naga::Arena<Resource>,
    entry_points: FastHashMap<(naga::ShaderStage, String), EntryPoint>,
    pub(crate) immediate_size: u32,
}

fn collect_call_arguments(
    block: &naga::Block,
    module: &naga::Module,
    module_info: &naga::valid::ModuleInfo,
    arguments: &mut FastHashMap<naga::Handle<naga::Expression>, (usize, usize)>,
) {
    for statement in block.iter() {
        match statement {
            naga::Statement::Call {
                function,
                arguments: call_arguments,
                ..
            } => {
                for (index, &argument) in call_arguments.iter().enumerate() {
                    let use_count = arguments.entry(argument).or_default();
                    use_count.0 += 1;
                    if function_parameter_uses_fragment_position(
                        module,
                        *function,
                        module_info,
                        index,
                    ) {
                        use_count.1 += 1;
                    }
                }
            }
            naga::Statement::Block(block) => {
                collect_call_arguments(block, module, module_info, arguments)
            }
            naga::Statement::If { accept, reject, .. } => {
                collect_call_arguments(accept, module, module_info, arguments);
                collect_call_arguments(reject, module, module_info, arguments);
            }
            naga::Statement::Switch { cases, .. } => {
                for case in cases {
                    collect_call_arguments(&case.body, module, module_info, arguments);
                }
            }
            naga::Statement::Loop {
                body, continuing, ..
            } => {
                collect_call_arguments(body, module, module_info, arguments);
                collect_call_arguments(continuing, module, module_info, arguments);
            }
            _ => {}
        }
    }
}

fn collect_called_functions(
    block: &naga::Block,
    called_functions: &mut Vec<naga::Handle<naga::Function>>,
) {
    let mut blocks = alloc::vec![block];
    while let Some(block) = blocks.pop() {
        for statement in block.iter() {
            match statement {
                naga::Statement::Call { function, .. } => called_functions.push(*function),
                naga::Statement::Block(block) => blocks.push(block),
                naga::Statement::If { accept, reject, .. } => {
                    blocks.push(accept);
                    blocks.push(reject);
                }
                naga::Statement::Switch { cases, .. } => {
                    blocks.extend(cases.iter().map(|case| &case.body));
                }
                naga::Statement::Loop {
                    body, continuing, ..
                } => {
                    blocks.push(body);
                    blocks.push(continuing);
                }
                _ => {}
            }
        }
    }
}

fn type_may_contain_integer(
    module: &naga::Module,
    root: &naga::TypeInner,
    budget: &mut usize,
) -> bool {
    enum Work<'a> {
        Inspect(&'a naga::TypeInner),
        Enter(naga::Handle<naga::Type>),
        Exit(naga::Handle<naga::Type>),
    }
    let mut stack = alloc::vec![Work::Inspect(root)];
    let mut active = FastHashSet::default();
    let mut visited = FastHashSet::default();
    while let Some(work) = stack.pop() {
        if *budget == 0 {
            return true;
        }
        *budget -= 1;
        let inner = match work {
            Work::Inspect(inner) => inner,
            Work::Enter(handle) => {
                if visited.contains(&handle) {
                    continue;
                }
                if !active.insert(handle) {
                    return true;
                }
                stack.push(Work::Exit(handle));
                stack.push(Work::Inspect(&module.types[handle].inner));
                continue;
            }
            Work::Exit(handle) => {
                active.remove(&handle);
                visited.insert(handle);
                continue;
            }
        };
        match inner {
            naga::TypeInner::Scalar(scalar)
            | naga::TypeInner::Atomic(scalar)
            | naga::TypeInner::Vector { scalar, .. }
            | naga::TypeInner::Matrix { scalar, .. }
            | naga::TypeInner::CooperativeMatrix { scalar, .. }
            | naga::TypeInner::ValuePointer { scalar, .. } => {
                if !matches!(
                    scalar.kind,
                    naga::ScalarKind::Float | naga::ScalarKind::Bool
                ) {
                    return true;
                }
            }
            naga::TypeInner::Array {
                size: naga::ArraySize::Pending(_),
                ..
            }
            | naga::TypeInner::BindingArray {
                size: naga::ArraySize::Pending(_),
                ..
            } => return true,
            naga::TypeInner::Pointer { base, .. }
            | naga::TypeInner::Array { base, .. }
            | naga::TypeInner::BindingArray { base, .. } => stack.push(Work::Enter(*base)),
            naga::TypeInner::Struct { members, .. } => {
                if members.len() > *budget {
                    return true;
                }
                stack.extend(members.iter().map(|member| Work::Enter(member.ty)));
            }
            naga::TypeInner::Image {
                class: naga::ImageClass::Sampled { kind, .. },
                ..
            } if *kind != naga::ScalarKind::Float => return true,
            naga::TypeInner::Image {
                class: naga::ImageClass::Sampled { .. },
                ..
            } => {}
            naga::TypeInner::Sampler { .. } => {}
            _ => return true,
        }
    }
    false
}

fn global_initializer_is_simple(
    module: &naga::Module,
    root: naga::Handle<naga::Expression>,
    budget: &mut usize,
) -> bool {
    let mut stack = alloc::vec![root];
    let mut visited = FastHashSet::default();
    while let Some(handle) = stack.pop() {
        if *budget == 0 {
            return false;
        }
        *budget -= 1;
        if !visited.insert(handle) {
            continue;
        }
        let Ok(expression) = module.global_expressions.try_get(handle) else {
            return false;
        };
        match expression {
            naga::Expression::Literal(_) | naga::Expression::ZeroValue(_) => {}
            naga::Expression::Constant(constant) => {
                let Ok(constant) = module.constants.try_get(*constant) else {
                    return false;
                };
                stack.push(constant.init);
            }
            naga::Expression::Compose { components, .. } => {
                if components.len() > *budget {
                    return false;
                }
                stack.extend(components.iter().copied());
            }
            naga::Expression::Splat { value, .. } => stack.push(*value),
            naga::Expression::Swizzle { vector, .. } => stack.push(*vector),
            naga::Expression::AccessIndex { base, .. } => stack.push(*base),
            _ => return false,
        }
    }
    true
}

fn supported_integer_vertex_argument(
    module: &naga::Module,
    function: &naga::Function,
    index: usize,
) -> bool {
    let Some(argument) = function.arguments.get(index) else {
        return false;
    };
    matches!(
        argument.binding.as_ref(),
        Some(naga::Binding::Location { .. })
    ) && matches!(
        module.types[argument.ty].inner,
        naga::TypeInner::Vector {
            size: naga::VectorSize::Bi | naga::VectorSize::Quad,
            scalar: naga::Scalar {
                kind: naga::ScalarKind::Sint | naga::ScalarKind::Uint,
                width: 4,
            },
        }
    )
}

fn expression_is_supported_integer_vertex_argument(
    module: &naga::Module,
    function: &naga::Function,
    expression: naga::Handle<naga::Expression>,
) -> bool {
    match function.expressions[expression] {
        naga::Expression::FunctionArgument(index) => {
            supported_integer_vertex_argument(module, function, index as usize)
        }
        _ => false,
    }
}

fn expression_may_contain_integer(
    module: &naga::Module,
    function_info: &naga::valid::FunctionInfo,
    expression: naga::Handle<naga::Expression>,
    budget: &mut usize,
) -> bool {
    type_may_contain_integer(
        module,
        function_info[expression].ty.inner_with(&module.types),
        budget,
    )
}

fn function_uses_unsupported_integer_semantics(
    module: &naga::Module,
    function: &naga::Function,
    function_info: &naga::valid::FunctionInfo,
    vertex_entry: bool,
    budget: &mut usize,
) -> bool {
    if function.arguments.len() > *budget {
        return true;
    }
    *budget -= function.arguments.len();
    for (index, argument) in function.arguments.iter().enumerate() {
        if type_may_contain_integer(module, &module.types[argument.ty].inner, budget)
            && !(vertex_entry && supported_integer_vertex_argument(module, function, index))
        {
            return true;
        }
    }
    if function.result.as_ref().is_some_and(|result| {
        type_may_contain_integer(module, &module.types[result.ty].inner, budget)
    }) {
        return true;
    }
    if function.expressions.len() > *budget {
        return true;
    }
    *budget -= function.expressions.len();
    let kinds = naga::proc::ExpressionKindTracker::from_arena(&function.expressions);
    function.expressions.iter().any(|(handle, expression)| {
        let integer_operation = match *expression {
            naga::Expression::Binary { left, right, .. } => {
                expression_may_contain_integer(module, function_info, left, budget)
                    || expression_may_contain_integer(module, function_info, right, budget)
            }
            naga::Expression::Unary { expr, .. } => {
                expression_may_contain_integer(module, function_info, expr, budget)
                    || expression_may_contain_integer(module, function_info, handle, budget)
            }
            naga::Expression::As {
                expr,
                kind,
                convert,
            } => {
                let supported_vertex_conversion = vertex_entry
                    && kind == naga::ScalarKind::Float
                    && convert == Some(4)
                    && expression_is_supported_integer_vertex_argument(module, function, expr);
                !supported_vertex_conversion
                    && (expression_may_contain_integer(module, function_info, expr, budget)
                        || expression_may_contain_integer(module, function_info, handle, budget))
            }
            naga::Expression::Math {
                arg,
                arg1,
                arg2,
                arg3,
                ..
            } => {
                expression_may_contain_integer(module, function_info, handle, budget)
                    || expression_may_contain_integer(module, function_info, arg, budget)
                    || [arg1, arg2, arg3].into_iter().flatten().any(|argument| {
                        expression_may_contain_integer(module, function_info, argument, budget)
                    })
            }
            naga::Expression::Select { accept, reject, .. } => {
                expression_may_contain_integer(module, function_info, accept, budget)
                    || expression_may_contain_integer(module, function_info, reject, budget)
            }
            naga::Expression::Constant(constant) => {
                module.constants.try_get(constant).map_or(true, |constant| {
                    !global_initializer_is_simple(module, constant.init, budget)
                })
            }
            naga::Expression::Override(_) => {
                expression_may_contain_integer(module, function_info, handle, budget)
            }
            naga::Expression::ImageSample {
                offset: Some(offset),
                ..
            } => !global_initializer_is_simple(module, offset, budget),
            _ => false,
        };
        integer_operation
            || (!kinds.is_const(handle)
                && expression_may_contain_integer(module, function_info, handle, budget)
                && !(vertex_entry
                    && expression_is_supported_integer_vertex_argument(module, function, handle)))
    })
}

fn block_has_switch(block: &naga::Block, budget: &mut usize) -> bool {
    let mut blocks = alloc::vec![block];
    while let Some(block) = blocks.pop() {
        if block.len() > *budget {
            return true;
        }
        *budget -= block.len();
        for statement in block.iter() {
            match statement {
                naga::Statement::Block(block) => blocks.push(block),
                naga::Statement::If { accept, reject, .. } => {
                    blocks.push(accept);
                    blocks.push(reject);
                }
                naga::Statement::Switch { .. } => return true,
                naga::Statement::Loop {
                    body, continuing, ..
                } => {
                    blocks.push(body);
                    blocks.push(continuing);
                }
                _ => {}
            }
        }
    }
    false
}

fn entry_point_uses_unsupported_integer_semantics(
    module: &naga::Module,
    module_info: &naga::valid::ModuleInfo,
    entry_point: &naga::EntryPoint,
    entry_point_info: &naga::valid::FunctionInfo,
) -> bool {
    let vertex_entry = entry_point.stage == naga::ShaderStage::Vertex;
    let mut budget = 65_536usize;
    if module.global_variables.iter().any(|(handle, global)| {
        !entry_point_info[handle].is_empty()
            && global.space == naga::AddressSpace::Uniform
            && type_may_contain_integer(module, &module.types[global.ty].inner, &mut budget)
    }) || function_uses_unsupported_integer_semantics(
        module,
        &entry_point.function,
        entry_point_info,
        vertex_entry,
        &mut budget,
    ) || block_has_switch(&entry_point.function.body, &mut budget)
        || module.global_variables.iter().any(|(handle, global)| {
            !entry_point_info[handle].is_empty()
                && global
                    .init
                    .is_some_and(|init| !global_initializer_is_simple(module, init, &mut budget))
        })
    {
        return true;
    }
    let mut called = Vec::new();
    collect_called_functions(&entry_point.function.body, &mut called);
    let mut visited = FastHashSet::default();
    while let Some(handle) = called.pop() {
        if !visited.insert(handle) {
            continue;
        }
        if visited.len() > 1024 {
            return true;
        }
        let function = &module.functions[handle];
        let info = &module_info[handle];
        if block_has_switch(&function.body, &mut budget)
            || function_uses_unsupported_integer_semantics(
                module,
                function,
                info,
                false,
                &mut budget,
            )
        {
            return true;
        }
        collect_called_functions(&function.body, &mut called);
    }
    budget == 0
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
enum MatrixStoreProvenance {
    Ordinary,
    UnsupportedAggregate,
    Unknown,
}

fn type_contains_fl9_matrix(
    module: &naga::Module,
    root: naga::Handle<naga::Type>,
    budget: &mut usize,
) -> bool {
    let mut stack = alloc::vec![root];
    let mut visited = FastHashSet::default();
    while let Some(handle) = stack.pop() {
        if *budget == 0 {
            return true;
        }
        *budget -= 1;
        if !visited.insert(handle) {
            continue;
        }
        match &module.types[handle].inner {
            naga::TypeInner::Matrix {
                rows: naga::VectorSize::Bi,
                ..
            } => return true,
            naga::TypeInner::Struct { members, .. } => {
                if members.len() > *budget {
                    return true;
                }
                stack.extend(members.iter().map(|member| member.ty));
            }
            naga::TypeInner::Array { base, .. } => stack.push(*base),
            _ => {}
        }
    }
    false
}

fn matrix_store_provenance(
    module: &naga::Module,
    function: &naga::Function,
    mut expression: naga::Handle<naga::Expression>,
    arguments: &[MatrixStoreProvenance],
    budget: &mut usize,
) -> MatrixStoreProvenance {
    while *budget != 0 {
        *budget -= 1;
        match function.expressions[expression] {
            naga::Expression::Access { base, .. } | naga::Expression::AccessIndex { base, .. } => {
                expression = base
            }
            naga::Expression::LocalVariable(handle) => {
                let ty = function.local_variables[handle].ty;
                return match module.types[ty].inner {
                    naga::TypeInner::Struct { .. } | naga::TypeInner::Array { .. }
                        if type_contains_fl9_matrix(module, ty, budget) =>
                    {
                        MatrixStoreProvenance::UnsupportedAggregate
                    }
                    _ => MatrixStoreProvenance::Ordinary,
                };
            }
            naga::Expression::GlobalVariable(handle) => {
                let ty = module.global_variables[handle].ty;
                return match module.types[ty].inner {
                    naga::TypeInner::Struct { .. } | naga::TypeInner::Array { .. }
                        if type_contains_fl9_matrix(module, ty, budget) =>
                    {
                        MatrixStoreProvenance::UnsupportedAggregate
                    }
                    _ => MatrixStoreProvenance::Ordinary,
                };
            }
            naga::Expression::FunctionArgument(index) => {
                return arguments
                    .get(index as usize)
                    .copied()
                    .unwrap_or(MatrixStoreProvenance::Unknown);
            }
            _ => return MatrixStoreProvenance::Unknown,
        }
    }
    MatrixStoreProvenance::Unknown
}

fn scan_matrix_store_block(
    module: &naga::Module,
    module_info: &naga::valid::ModuleInfo,
    function: &naga::Function,
    function_info: &naga::valid::FunctionInfo,
    block: &naga::Block,
    arguments: &[MatrixStoreProvenance],
    visited: &mut FastHashSet<(naga::Handle<naga::Function>, Vec<MatrixStoreProvenance>)>,
    budget: &mut usize,
    depth: usize,
) -> bool {
    if depth >= 256 || block.len() > *budget {
        return true;
    }
    *budget -= block.len();
    block.iter().any(|statement| match statement {
        naga::Statement::Store { pointer, .. } => !matches!(
            matrix_store_provenance(module, function, *pointer, arguments, budget),
            MatrixStoreProvenance::Ordinary
        ),
        naga::Statement::Block(block) => scan_matrix_store_block(
            module,
            module_info,
            function,
            function_info,
            block,
            arguments,
            visited,
            budget,
            depth + 1,
        ),
        naga::Statement::If { accept, reject, .. } => {
            scan_matrix_store_block(
                module,
                module_info,
                function,
                function_info,
                accept,
                arguments,
                visited,
                budget,
                depth + 1,
            ) || scan_matrix_store_block(
                module,
                module_info,
                function,
                function_info,
                reject,
                arguments,
                visited,
                budget,
                depth + 1,
            )
        }
        naga::Statement::Switch { cases, .. } => cases.iter().any(|case| {
            scan_matrix_store_block(
                module,
                module_info,
                function,
                function_info,
                &case.body,
                arguments,
                visited,
                budget,
                depth + 1,
            )
        }),
        naga::Statement::Loop {
            body, continuing, ..
        } => {
            scan_matrix_store_block(
                module,
                module_info,
                function,
                function_info,
                body,
                arguments,
                visited,
                budget,
                depth + 1,
            ) || scan_matrix_store_block(
                module,
                module_info,
                function,
                function_info,
                continuing,
                arguments,
                visited,
                budget,
                depth + 1,
            )
        }
        naga::Statement::Call {
            function: callee,
            arguments: actual,
            ..
        } => {
            if actual.len() > *budget {
                return true;
            }
            *budget -= actual.len();
            let mapped: Vec<_> = actual
                .iter()
                .map(
                    |&expression| match function_info[expression].ty.inner_with(&module.types) {
                        naga::TypeInner::Pointer { .. } | naga::TypeInner::ValuePointer { .. } => {
                            matrix_store_provenance(module, function, expression, arguments, budget)
                        }
                        _ => MatrixStoreProvenance::Ordinary,
                    },
                )
                .collect();
            if mapped.contains(&MatrixStoreProvenance::Unknown) {
                return true;
            }
            let key = (*callee, mapped.clone());
            if !visited.insert(key) {
                return false;
            }
            if visited.len() > 1024 {
                return true;
            }
            let callee_fn = &module.functions[*callee];
            scan_matrix_store_block(
                module,
                module_info,
                callee_fn,
                &module_info[*callee],
                &callee_fn.body,
                &mapped,
                visited,
                budget,
                depth + 1,
            )
        }
        _ => false,
    })
}

fn entry_point_uses_unsupported_matrix_aggregate_store(
    module: &naga::Module,
    module_info: &naga::valid::ModuleInfo,
    entry_point: &naga::EntryPoint,
    entry_point_info: &naga::valid::FunctionInfo,
) -> bool {
    let mut budget = 65_536usize;
    scan_matrix_store_block(
        module,
        module_info,
        &entry_point.function,
        entry_point_info,
        &entry_point.function.body,
        &[],
        &mut FastHashSet::default(),
        &mut budget,
        0,
    ) || budget == 0
}

fn expression_literal_f32(
    function: &naga::Function,
    expression: naga::Handle<naga::Expression>,
) -> Option<f32> {
    match function.expressions[expression] {
        naga::Expression::Literal(naga::Literal::F32(value)) => Some(value),
        naga::Expression::Unary {
            op: naga::UnaryOperator::Negate,
            expr,
        } => match function.expressions[expr] {
            naga::Expression::Literal(naga::Literal::F32(value)) => Some(-value),
            _ => None,
        },
        _ => None,
    }
}

fn collect_function_texture_sample_biases(
    function: &naga::Function,
    biases: &mut Vec<TextureSampleBias>,
) {
    for (_, expression) in function.expressions.iter() {
        let naga::Expression::ImageSample {
            level: naga::SampleLevel::Bias(bias),
            offset,
            ..
        } = *expression
        else {
            continue;
        };
        biases.push(TextureSampleBias {
            literal: expression_literal_f32(function, bias),
            has_offset: offset.is_some(),
        });
    }
}

fn entry_point_texture_sample_biases(
    module: &naga::Module,
    entry_point: &naga::EntryPoint,
) -> Vec<TextureSampleBias> {
    let mut biases = Vec::new();
    collect_function_texture_sample_biases(&entry_point.function, &mut biases);

    let mut called_functions = Vec::new();
    collect_called_functions(&entry_point.function.body, &mut called_functions);
    let mut visited = FastHashSet::default();
    while let Some(handle) = called_functions.pop() {
        if !visited.insert(handle) {
            continue;
        }
        let function = &module.functions[handle];
        collect_function_texture_sample_biases(function, &mut biases);
        collect_called_functions(&function.body, &mut called_functions);
    }
    biases
}

fn function_uses_fragment_derivatives(function: &naga::Function) -> bool {
    function.expressions.iter().any(|(_, expression)| {
        matches!(
            *expression,
            naga::Expression::Derivative { .. }
                | naga::Expression::ImageSample {
                    level: naga::SampleLevel::Gradient { .. },
                    ..
                }
        )
    })
}

fn entry_point_uses_fragment_derivatives(
    module: &naga::Module,
    entry_point: &naga::EntryPoint,
) -> bool {
    if function_uses_fragment_derivatives(&entry_point.function) {
        return true;
    }

    let mut called_functions = Vec::new();
    collect_called_functions(&entry_point.function.body, &mut called_functions);
    let mut visited = FastHashSet::default();
    while let Some(handle) = called_functions.pop() {
        if !visited.insert(handle) {
            continue;
        }
        let function = &module.functions[handle];
        if function_uses_fragment_derivatives(function) {
            return true;
        }
        collect_called_functions(&function.body, &mut called_functions);
    }
    false
}

fn function_uses_texture_sample_level(function: &naga::Function) -> bool {
    function.expressions.iter().any(|(_, expression)| {
        matches!(
            *expression,
            naga::Expression::ImageSample {
                level: naga::SampleLevel::Exact(_),
                ..
            }
        )
    })
}

fn entry_point_uses_texture_sample_level(
    module: &naga::Module,
    entry_point: &naga::EntryPoint,
) -> bool {
    if function_uses_texture_sample_level(&entry_point.function) {
        return true;
    }

    let mut called_functions = Vec::new();
    collect_called_functions(&entry_point.function.body, &mut called_functions);
    let mut visited = FastHashSet::default();
    while let Some(handle) = called_functions.pop() {
        if !visited.insert(handle) {
            continue;
        }
        let function = &module.functions[handle];
        if function_uses_texture_sample_level(function) {
            return true;
        }
        collect_called_functions(&function.body, &mut called_functions);
    }
    false
}

fn function_uses_texture_gather(function: &naga::Function) -> bool {
    function.expressions.iter().any(|(_, expression)| {
        matches!(
            *expression,
            naga::Expression::ImageSample {
                gather: Some(_),
                ..
            }
        )
    })
}

fn entry_point_uses_texture_gather(module: &naga::Module, entry_point: &naga::EntryPoint) -> bool {
    if function_uses_texture_gather(&entry_point.function) {
        return true;
    }

    let mut called_functions = Vec::new();
    collect_called_functions(&entry_point.function.body, &mut called_functions);
    let mut visited = FastHashSet::default();
    while let Some(handle) = called_functions.pop() {
        if !visited.insert(handle) {
            continue;
        }
        let function = &module.functions[handle];
        if function_uses_texture_gather(function) {
            return true;
        }
        collect_called_functions(&function.body, &mut called_functions);
    }
    false
}

fn entry_point_uses_texture_load(module: &naga::Module, entry_point: &naga::EntryPoint) -> bool {
    if entry_point
        .function
        .expressions
        .iter()
        .any(|(_, expression)| matches!(expression, naga::Expression::ImageLoad { .. }))
    {
        return true;
    }

    let mut called_functions = Vec::new();
    collect_called_functions(&entry_point.function.body, &mut called_functions);
    let mut visited = FastHashSet::default();
    while let Some(handle) = called_functions.pop() {
        if !visited.insert(handle) {
            continue;
        }
        let function = &module.functions[handle];
        if function
            .expressions
            .iter()
            .any(|(_, expression)| matches!(expression, naga::Expression::ImageLoad { .. }))
        {
            return true;
        }
        collect_called_functions(&function.body, &mut called_functions);
    }
    false
}

fn entry_point_uses_texture_query(module: &naga::Module, entry_point: &naga::EntryPoint) -> bool {
    let uses_query = |function: &naga::Function| {
        function
            .expressions
            .iter()
            .any(|(_, expression)| matches!(expression, naga::Expression::ImageQuery { .. }))
    };
    if uses_query(&entry_point.function) {
        return true;
    }

    let mut called_functions = Vec::new();
    collect_called_functions(&entry_point.function.body, &mut called_functions);
    let mut visited = FastHashSet::default();
    while let Some(handle) = called_functions.pop() {
        if !visited.insert(handle) {
            continue;
        }
        let function = &module.functions[handle];
        if uses_query(function) {
            return true;
        }
        collect_called_functions(&function.body, &mut called_functions);
    }
    false
}

fn entry_point_uses_texture_sample_base_clamp_to_edge(
    module: &naga::Module,
    entry_point: &naga::EntryPoint,
) -> bool {
    let uses_base_clamp = |function: &naga::Function| {
        function.expressions.iter().any(|(_, expression)| {
            matches!(
                expression,
                naga::Expression::ImageSample {
                    clamp_to_edge: true,
                    ..
                }
            )
        })
    };
    if uses_base_clamp(&entry_point.function) {
        return true;
    }

    let mut called_functions = Vec::new();
    collect_called_functions(&entry_point.function.body, &mut called_functions);
    let mut visited = FastHashSet::default();
    while let Some(handle) = called_functions.pop() {
        if !visited.insert(handle) {
            continue;
        }
        let function = &module.functions[handle];
        if uses_base_clamp(function) {
            return true;
        }
        collect_called_functions(&function.body, &mut called_functions);
    }
    false
}

fn root_function_argument(
    function: &naga::Function,
    expression: naga::Handle<naga::Expression>,
) -> Option<u32> {
    match function.expressions[expression] {
        naga::Expression::FunctionArgument(index) => Some(index),
        naga::Expression::Access { base, .. } | naga::Expression::AccessIndex { base, .. } => {
            root_function_argument(function, base)
        }
        _ => None,
    }
}

fn function_parameter_uses_fragment_position(
    module: &naga::Module,
    function_handle: naga::Handle<naga::Function>,
    module_info: &naga::valid::ModuleInfo,
    argument_index: usize,
) -> bool {
    let function = &module.functions[function_handle];
    let info = &module_info[function_handle];
    let Some(argument) = function.arguments.get(argument_index) else {
        return false;
    };
    let mut call_arguments = FastHashMap::default();
    collect_call_arguments(&function.body, module, module_info, &mut call_arguments);
    if matches!(
        &module.types[argument.ty].inner,
        naga::TypeInner::Struct { .. }
    ) {
        return function.expressions.iter().any(|(handle, expression)| {
            let (call_count, uses_position_count) =
                call_arguments.get(&handle).copied().unwrap_or_default();
            let direct_use = info[handle].ref_count.saturating_sub(call_count) > 0;
            match *expression {
                naga::Expression::FunctionArgument(index) if index as usize == argument_index => {
                    uses_position_count > 0
                }
                naga::Expression::AccessIndex { base, index }
                    if root_function_argument(function, base) == Some(argument_index as u32) =>
                {
                    let naga::TypeInner::Struct { members, .. } =
                        info[base].ty.inner_with(&module.types)
                    else {
                        return false;
                    };
                    members.get(index as usize).is_some_and(|member| {
                        matches!(
                            member.binding,
                            Some(naga::Binding::BuiltIn(naga::BuiltIn::Position { .. }))
                        ) && (direct_use || uses_position_count > 0)
                    })
                }
                _ => false,
            }
        });
    }
    function.expressions.iter().any(|(handle, expression)| {
        let naga::Expression::FunctionArgument(index) = expression else {
            return false;
        };
        if *index as usize != argument_index {
            return false;
        }
        let (call_count, uses_position_count) =
            call_arguments.get(&handle).copied().unwrap_or_default();
        info[handle].ref_count.saturating_sub(call_count) > 0 || uses_position_count > 0
    })
}

fn type_contains_fragment_position(module: &naga::Module, ty: naga::Handle<naga::Type>) -> bool {
    match &module.types[ty].inner {
        naga::TypeInner::Struct { members, .. } => members.iter().any(|member| {
            matches!(
                member.binding,
                Some(naga::Binding::BuiltIn(naga::BuiltIn::Position { .. }))
            ) || type_contains_fragment_position(module, member.ty)
        }),
        _ => false,
    }
}

fn entry_point_uses_fragment_position(
    module: &naga::Module,
    entry_point: &naga::EntryPoint,
    info: &naga::valid::FunctionInfo,
    module_info: &naga::valid::ModuleInfo,
) -> bool {
    let mut call_arguments = FastHashMap::default();
    collect_call_arguments(
        &entry_point.function.body,
        module,
        module_info,
        &mut call_arguments,
    );

    entry_point
        .function
        .expressions
        .iter()
        .any(|(handle, expression)| {
            let (call_count, uses_position_count) =
                call_arguments.get(&handle).copied().unwrap_or_default();
            let uses_position = uses_position_count > 0;
            if info[handle].ref_count.saturating_sub(call_count) == 0 && !uses_position {
                return false;
            }

            match *expression {
                naga::Expression::FunctionArgument(index) => entry_point
                    .function
                    .arguments
                    .get(index as usize)
                    .is_some_and(|argument| {
                        matches!(
                            argument.binding,
                            Some(naga::Binding::BuiltIn(naga::BuiltIn::Position { .. }))
                        ) || (uses_position && type_contains_fragment_position(module, argument.ty))
                    }),
                naga::Expression::AccessIndex { base, index } => {
                    let naga::TypeInner::Struct { members, .. } =
                        info[base].ty.inner_with(&module.types)
                    else {
                        return false;
                    };
                    members.get(index as usize).is_some_and(|member| {
                        matches!(
                            member.binding,
                            Some(naga::Binding::BuiltIn(naga::BuiltIn::Position { .. }))
                        )
                    })
                }
                _ => false,
            }
        })
}

#[derive(Debug)]
pub struct PassthroughInterface {
    pub entry_point_names: HashSet<String>,
}

// Most shaders will use a standard interface which is very large.
// Passthrough shaders have a much smaller interface. No reason to
// box the standard interface though.
#[expect(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum ShaderMetaData {
    Interface(Interface),
    Passthrough(PassthroughInterface),
}
impl ShaderMetaData {
    pub fn interface(&self) -> Option<&Interface> {
        match self {
            Self::Interface(i) => Some(i),
            Self::Passthrough(_) => None,
        }
    }
}

#[derive(Clone, Debug, Error)]
#[non_exhaustive]
pub enum BindingError {
    #[error("Binding is missing from the pipeline layout")]
    Missing,
    #[error("Visibility flags don't include the shader stage")]
    Invisible,
    #[error(
        "Type on the shader side ({shader:?}) does not match the pipeline binding ({binding:?})"
    )]
    WrongType {
        binding: BindingTypeName,
        shader: BindingTypeName,
    },
    #[error("Storage class {binding:?} doesn't match the shader {shader:?}")]
    WrongAddressSpace {
        binding: naga::AddressSpace,
        shader: naga::AddressSpace,
    },
    #[error("Address space {space:?} is not a valid Buffer address space")]
    WrongBufferAddressSpace { space: naga::AddressSpace },
    #[error("Buffer structure size {buffer_size}, added to one element of an unbound array, if it's the last field, ended up greater than the given `min_binding_size`, which is {min_binding_size}")]
    WrongBufferSize {
        buffer_size: wgt::BufferSize,
        min_binding_size: wgt::BufferSize,
    },
    #[error("View dimension {dim:?} (is array: {is_array}) doesn't match the binding {binding:?}")]
    WrongTextureViewDimension {
        dim: naga::ImageDimension,
        is_array: bool,
        binding: BindingType,
    },
    #[error("Texture class {binding:?} doesn't match the shader {shader:?}")]
    WrongTextureClass {
        binding: naga::ImageClass,
        shader: naga::ImageClass,
    },
    #[error("Comparison flag doesn't match the shader")]
    WrongSamplerComparison,
    #[error("Derived bind group layout type is not consistent between stages")]
    InconsistentlyDerivedType,
    #[error("Texture format {0:?} is not supported for storage use")]
    BadStorageFormat(wgt::TextureFormat),
}

impl WebGpuError for BindingError {
    fn webgpu_error_type(&self) -> ErrorType {
        ErrorType::Validation
    }
}

#[derive(Clone, Debug, Error)]
#[non_exhaustive]
pub enum FilteringError {
    #[error("Integer textures can't be sampled with a filtering sampler")]
    Integer,
    #[error("Non-filterable float textures can't be sampled with a filtering sampler")]
    Float,
}

impl WebGpuError for FilteringError {
    fn webgpu_error_type(&self) -> ErrorType {
        ErrorType::Validation
    }
}

#[derive(Clone, Debug, Error)]
#[non_exhaustive]
pub enum InputError {
    #[error("Input is not provided by the earlier stage in the pipeline")]
    Missing,
    #[error("Input type is not compatible with the provided {0}")]
    WrongType(NumericType),
    #[error("Input interpolation doesn't match provided {0:?}")]
    InterpolationMismatch(Option<naga::Interpolation>),
    #[error("Input sampling doesn't match provided {0:?}")]
    SamplingMismatch(Option<naga::Sampling>),
    #[error("Pipeline input has per_primitive={pipeline_input}, but shader expects per_primitive={shader}")]
    WrongPerPrimitive { pipeline_input: bool, shader: bool },
}

impl WebGpuError for InputError {
    fn webgpu_error_type(&self) -> ErrorType {
        ErrorType::Validation
    }
}

/// Errors produced when validating a programmable stage of a pipeline.
#[derive(Clone, Debug, Error)]
#[non_exhaustive]
pub enum StageError {
    #[error(transparent)]
    InvalidWorkgroupSize(#[from] InvalidWorkgroupSizeError),
    #[error("Unable to find entry point '{0}'")]
    MissingEntryPoint(String),
    #[error("Shader global {0:?} is not available in the pipeline layout")]
    Binding(naga::ResourceBinding, #[source] BindingError),
    #[error("Unable to filter the texture ({texture:?}) by the sampler ({sampler:?})")]
    Filtering {
        texture: naga::ResourceBinding,
        sampler: naga::ResourceBinding,
        #[source]
        error: FilteringError,
    },
    #[error("Location[{location}] {var} is not provided by the previous stage outputs")]
    Input {
        location: wgt::ShaderLocation,
        var: InterfaceVar,
        #[source]
        error: InputError,
    },
    #[error(
        "Unable to select an entry point: no entry point was found in the provided shader module"
    )]
    NoEntryPointFound,
    #[error(
        "Unable to select an entry point: \
        multiple entry points were found in the provided shader module, \
        but no entry point was specified"
    )]
    MultipleEntryPointsFound,
    #[error(transparent)]
    InvalidResource(#[from] InvalidResourceError),
    #[error(
        "vertex shader output location Location[{location}] ({var}) exceeds the \
        `max_inter_stage_shader_variables` limit ({}, 0-based){}",
        // NOTE: Remember: the limit is 0-based for indices.
        i64::from(*limit) - 1,
        display_deductions_as_optional_list(deductions, |d| d.for_location())
    )]
    VertexOutputLocationTooLarge {
        location: u32,
        var: InterfaceVar,
        limit: u32,
        deductions: Vec<MaxVertexShaderOutputDeduction>,
    },
    #[error(
        "found {num_found} user-defined vertex shader output variables, which exceeds the \
        `max_inter_stage_shader_variables` limit ({limit}){}",
        display_deductions_as_optional_list(deductions, |d| d.for_variables())
    )]
    TooManyUserDefinedVertexOutputs {
        num_found: u32,
        limit: u32,
        deductions: Vec<MaxVertexShaderOutputDeduction>,
    },
    #[error(
        "fragment shader input location Location[{location}] ({var}) exceeds the \
        `max_inter_stage_shader_variables` limit ({}, 0-based){}",
        // NOTE: Remember: the limit is 0-based for indices.
        i64::from(*limit) - 1,
        // NOTE: WebGPU spec. validation for fragment inputs is expressed in terms of variables
        // (unlike vertex outputs), so we use `MaxFragmentShaderInputDeduction::for_variables` here
        // (and not a non-existent `for_locations`).
        display_deductions_as_optional_list(deductions, |d| d.for_variables())
    )]
    FragmentInputLocationTooLarge {
        location: u32,
        var: InterfaceVar,
        limit: u32,
        deductions: Vec<MaxFragmentShaderInputDeduction>,
    },
    #[error(
        "found {num_found} user-defined fragment shader input variables, which exceeds the \
        `max_inter_stage_shader_variables` limit ({limit}){}",
        display_deductions_as_optional_list(deductions, |d| d.for_variables())
    )]
    TooManyUserDefinedFragmentInputs {
        num_found: u32,
        limit: u32,
        deductions: Vec<MaxFragmentShaderInputDeduction>,
    },
    #[error(
        "Location[{location}] {var}'s index exceeds the `max_color_attachments` limit ({limit})"
    )]
    ColorAttachmentLocationTooLarge {
        location: u32,
        var: InterfaceVar,
        limit: u32,
    },
    #[error("Mesh shaders are limited to {limit} output vertices by `Limits::max_mesh_output_vertices`, but the shader has a maximum number of {value}")]
    TooManyMeshVertices { limit: u32, value: u32 },
    #[error("Mesh shaders are limited to {limit} output primitives by `Limits::max_mesh_output_primitives`, but the shader has a maximum number of {value}")]
    TooManyMeshPrimitives { limit: u32, value: u32 },
    #[error("Mesh or task shaders are limited to {limit} bytes of task payload by `Limits::max_task_payload_size`, but the shader has a task payload of size {value}")]
    TaskPayloadTooLarge { limit: u32, value: u32 },
    #[error("Mesh shader's task payload has size ({shader:?}), which doesn't match the payload declared in the task stage ({input:?})")]
    TaskPayloadMustMatch {
        input: Option<u32>,
        shader: Option<u32>,
    },
    #[error("Primitive index can only be used in a fragment shader if the preceding shader was a vertex shader or a mesh shader that writes to primitive index.")]
    InvalidPrimitiveIndex,
    #[error("If a mesh shader writes to primitive index, it must be read by the fragment shader.")]
    MissingPrimitiveIndex,
    #[error("DrawId cannot be used in a mesh shader in a pipeline with a task shader")]
    DrawIdError,
    #[error("Pipeline uses dual-source blending, but the shader does not support it")]
    InvalidDualSourceBlending,
    #[error("Fragment shader writes depth, but pipeline does not have a depth attachment")]
    MissingFragDepthAttachment,
    #[error("Per vertex fragment inputs can only be used in triangle primitive pipelines")]
    PerVertexNotTriangles,
    #[error("Mesh shader pipelines must have primitive topology of TriangleList, LineList or PointList, and this must match with what the mesh shader declares.")]
    MeshTopologyMismatch,
}

impl WebGpuError for StageError {
    fn webgpu_error_type(&self) -> ErrorType {
        match self {
            Self::Binding(_, e) => e.webgpu_error_type(),
            Self::InvalidResource(e) => e.webgpu_error_type(),
            Self::Filtering {
                texture: _,
                sampler: _,
                error,
            } => error.webgpu_error_type(),
            Self::Input {
                location: _,
                var: _,
                error,
            } => error.webgpu_error_type(),
            Self::InvalidWorkgroupSize { .. }
            | Self::MissingEntryPoint(..)
            | Self::NoEntryPointFound
            | Self::MultipleEntryPointsFound
            | Self::VertexOutputLocationTooLarge { .. }
            | Self::TooManyUserDefinedVertexOutputs { .. }
            | Self::FragmentInputLocationTooLarge { .. }
            | Self::TooManyUserDefinedFragmentInputs { .. }
            | Self::ColorAttachmentLocationTooLarge { .. }
            | Self::TooManyMeshVertices { .. }
            | Self::TooManyMeshPrimitives { .. }
            | Self::TaskPayloadTooLarge { .. }
            | Self::TaskPayloadMustMatch { .. }
            | Self::InvalidPrimitiveIndex
            | Self::MissingPrimitiveIndex
            | Self::DrawIdError
            | Self::InvalidDualSourceBlending
            | Self::MissingFragDepthAttachment
            | Self::PerVertexNotTriangles
            | Self::MeshTopologyMismatch => ErrorType::Validation,
        }
    }
}

pub use wgpu_naga_bridge::map_storage_format_from_naga;
pub use wgpu_naga_bridge::map_storage_format_to_naga;

impl Resource {
    fn check_binding_use(&self, entry: &BindGroupLayoutEntry) -> Result<(), BindingError> {
        match self.ty {
            ResourceType::Buffer { size } => {
                let min_size = match entry.ty {
                    BindingType::Buffer {
                        ty,
                        has_dynamic_offset: _,
                        min_binding_size,
                    } => {
                        let class = match ty {
                            wgt::BufferBindingType::Uniform => naga::AddressSpace::Uniform,
                            wgt::BufferBindingType::Storage { read_only } => {
                                let mut naga_access = naga::StorageAccess::LOAD;
                                naga_access.set(naga::StorageAccess::STORE, !read_only);
                                naga::AddressSpace::Storage {
                                    access: naga_access,
                                }
                            }
                        };
                        if self.class != class {
                            return Err(BindingError::WrongAddressSpace {
                                binding: class,
                                shader: self.class,
                            });
                        }
                        min_binding_size
                    }
                    _ => {
                        return Err(BindingError::WrongType {
                            binding: (&entry.ty).into(),
                            shader: (&self.ty).into(),
                        })
                    }
                };
                match min_size {
                    Some(non_zero) if non_zero < size => {
                        return Err(BindingError::WrongBufferSize {
                            buffer_size: size,
                            min_binding_size: non_zero,
                        })
                    }
                    _ => (),
                }
            }
            ResourceType::Sampler { comparison } => match entry.ty {
                BindingType::Sampler(ty) => {
                    if (ty == wgt::SamplerBindingType::Comparison) != comparison {
                        return Err(BindingError::WrongSamplerComparison);
                    }
                }
                _ => {
                    return Err(BindingError::WrongType {
                        binding: (&entry.ty).into(),
                        shader: (&self.ty).into(),
                    })
                }
            },
            ResourceType::Texture {
                dim,
                arrayed,
                class: shader_class,
            } => {
                let view_dimension = match entry.ty {
                    BindingType::Texture { view_dimension, .. }
                    | BindingType::StorageTexture { view_dimension, .. } => view_dimension,
                    BindingType::ExternalTexture => wgt::TextureViewDimension::D2,
                    _ => {
                        return Err(BindingError::WrongTextureViewDimension {
                            dim,
                            is_array: false,
                            binding: entry.ty,
                        })
                    }
                };
                if arrayed {
                    match (dim, view_dimension) {
                        (naga::ImageDimension::D2, wgt::TextureViewDimension::D2Array) => (),
                        (naga::ImageDimension::Cube, wgt::TextureViewDimension::CubeArray) => (),
                        _ => {
                            return Err(BindingError::WrongTextureViewDimension {
                                dim,
                                is_array: true,
                                binding: entry.ty,
                            })
                        }
                    }
                } else {
                    match (dim, view_dimension) {
                        (naga::ImageDimension::D1, wgt::TextureViewDimension::D1) => (),
                        (naga::ImageDimension::D2, wgt::TextureViewDimension::D2) => (),
                        (naga::ImageDimension::D3, wgt::TextureViewDimension::D3) => (),
                        (naga::ImageDimension::Cube, wgt::TextureViewDimension::Cube) => (),
                        _ => {
                            return Err(BindingError::WrongTextureViewDimension {
                                dim,
                                is_array: false,
                                binding: entry.ty,
                            })
                        }
                    }
                }
                match entry.ty {
                    BindingType::Texture {
                        sample_type,
                        view_dimension: _,
                        multisampled: multi,
                    } => {
                        let binding_class = match sample_type {
                            wgt::TextureSampleType::Float { .. } => naga::ImageClass::Sampled {
                                kind: naga::ScalarKind::Float,
                                multi,
                            },
                            wgt::TextureSampleType::Sint => naga::ImageClass::Sampled {
                                kind: naga::ScalarKind::Sint,
                                multi,
                            },
                            wgt::TextureSampleType::Uint => naga::ImageClass::Sampled {
                                kind: naga::ScalarKind::Uint,
                                multi,
                            },
                            wgt::TextureSampleType::Depth => naga::ImageClass::Depth { multi },
                        };
                        if shader_class == binding_class {
                            Ok(())
                        } else {
                            Err(binding_class)
                        }
                    }
                    BindingType::StorageTexture {
                        access: wgt_binding_access,
                        format: wgt_binding_format,
                        view_dimension: _,
                    } => {
                        const LOAD_STORE: naga::StorageAccess =
                            naga::StorageAccess::LOAD.union(naga::StorageAccess::STORE);
                        let binding_format = map_storage_format_to_naga(wgt_binding_format)
                            .ok_or(BindingError::BadStorageFormat(wgt_binding_format))?;
                        let binding_access = match wgt_binding_access {
                            wgt::StorageTextureAccess::ReadOnly => naga::StorageAccess::LOAD,
                            wgt::StorageTextureAccess::WriteOnly => naga::StorageAccess::STORE,
                            wgt::StorageTextureAccess::ReadWrite => LOAD_STORE,
                            wgt::StorageTextureAccess::Atomic => {
                                naga::StorageAccess::ATOMIC | LOAD_STORE
                            }
                        };
                        match shader_class {
                            // Formats must match exactly. A write-only shader (but not a
                            // read-only shader) is compatible with a read-write binding.
                            naga::ImageClass::Storage {
                                format: shader_format,
                                access: shader_access,
                            } if shader_format == binding_format
                                && (shader_access == binding_access
                                    || shader_access == naga::StorageAccess::STORE
                                        && binding_access == LOAD_STORE) =>
                            {
                                Ok(())
                            }
                            _ => Err(naga::ImageClass::Storage {
                                format: binding_format,
                                access: binding_access,
                            }),
                        }
                    }
                    BindingType::ExternalTexture => {
                        let binding_class = naga::ImageClass::External;
                        if shader_class == binding_class {
                            Ok(())
                        } else {
                            Err(binding_class)
                        }
                    }
                    _ => {
                        return Err(BindingError::WrongType {
                            binding: (&entry.ty).into(),
                            shader: (&self.ty).into(),
                        })
                    }
                }
                .map_err(|binding_class| BindingError::WrongTextureClass {
                    binding: binding_class,
                    shader: shader_class,
                })?;
            }
            ResourceType::AccelerationStructure { vertex_return } => match entry.ty {
                BindingType::AccelerationStructure {
                    vertex_return: entry_vertex_return,
                } if vertex_return == entry_vertex_return => (),
                _ => {
                    return Err(BindingError::WrongType {
                        binding: (&entry.ty).into(),
                        shader: (&self.ty).into(),
                    })
                }
            },
        };

        Ok(())
    }

    fn derive_binding_type(
        &self,
        is_reffed_by_sampler_in_entrypoint: bool,
    ) -> Result<BindingType, BindingError> {
        Ok(match self.ty {
            ResourceType::Buffer { size } => BindingType::Buffer {
                ty: match self.class {
                    naga::AddressSpace::Uniform => wgt::BufferBindingType::Uniform,
                    naga::AddressSpace::Storage { access } => wgt::BufferBindingType::Storage {
                        read_only: access == naga::StorageAccess::LOAD,
                    },
                    _ => return Err(BindingError::WrongBufferAddressSpace { space: self.class }),
                },
                has_dynamic_offset: false,
                min_binding_size: Some(size),
            },
            ResourceType::Sampler { comparison } => BindingType::Sampler(if comparison {
                wgt::SamplerBindingType::Comparison
            } else {
                wgt::SamplerBindingType::Filtering
            }),
            ResourceType::Texture {
                dim,
                arrayed,
                class,
            } => {
                let view_dimension = match dim {
                    naga::ImageDimension::D1 => wgt::TextureViewDimension::D1,
                    naga::ImageDimension::D2 if arrayed => wgt::TextureViewDimension::D2Array,
                    naga::ImageDimension::D2 => wgt::TextureViewDimension::D2,
                    naga::ImageDimension::D3 => wgt::TextureViewDimension::D3,
                    naga::ImageDimension::Cube if arrayed => wgt::TextureViewDimension::CubeArray,
                    naga::ImageDimension::Cube => wgt::TextureViewDimension::Cube,
                };
                match class {
                    naga::ImageClass::Sampled { multi, kind } => BindingType::Texture {
                        sample_type: match kind {
                            naga::ScalarKind::Float => wgt::TextureSampleType::Float {
                                filterable: is_reffed_by_sampler_in_entrypoint,
                            },
                            naga::ScalarKind::Sint => wgt::TextureSampleType::Sint,
                            naga::ScalarKind::Uint => wgt::TextureSampleType::Uint,
                            naga::ScalarKind::AbstractInt
                            | naga::ScalarKind::AbstractFloat
                            | naga::ScalarKind::Bool => unreachable!(),
                        },
                        view_dimension,
                        multisampled: multi,
                    },
                    naga::ImageClass::Depth { multi } => BindingType::Texture {
                        sample_type: wgt::TextureSampleType::Depth,
                        view_dimension,
                        multisampled: multi,
                    },
                    naga::ImageClass::Storage { format, access } => BindingType::StorageTexture {
                        access: {
                            const LOAD_STORE: naga::StorageAccess =
                                naga::StorageAccess::LOAD.union(naga::StorageAccess::STORE);
                            match access {
                                naga::StorageAccess::LOAD => wgt::StorageTextureAccess::ReadOnly,
                                naga::StorageAccess::STORE => wgt::StorageTextureAccess::WriteOnly,
                                LOAD_STORE => wgt::StorageTextureAccess::ReadWrite,
                                _ if access.contains(naga::StorageAccess::ATOMIC) => {
                                    wgt::StorageTextureAccess::Atomic
                                }
                                _ => unreachable!(),
                            }
                        },
                        view_dimension,
                        format: {
                            let f = map_storage_format_from_naga(format);
                            let original = map_storage_format_to_naga(f)
                                .ok_or(BindingError::BadStorageFormat(f))?;
                            debug_assert_eq!(format, original);
                            f
                        },
                    },
                    naga::ImageClass::External => BindingType::ExternalTexture,
                }
            }
            ResourceType::AccelerationStructure { vertex_return } => {
                BindingType::AccelerationStructure { vertex_return }
            }
        })
    }
}

impl NumericType {
    fn from_vertex_format(format: wgt::VertexFormat) -> Self {
        use naga::{Scalar, VectorSize as Vs};
        use wgt::VertexFormat as Vf;

        let (dim, scalar) = match format {
            Vf::Uint8 | Vf::Uint16 | Vf::Uint32 => (NumericDimension::Scalar, Scalar::U32),
            Vf::Uint8x2 | Vf::Uint16x2 | Vf::Uint32x2 => {
                (NumericDimension::Vector(Vs::Bi), Scalar::U32)
            }
            Vf::Uint32x3 => (NumericDimension::Vector(Vs::Tri), Scalar::U32),
            Vf::Uint8x4 | Vf::Uint16x4 | Vf::Uint32x4 => {
                (NumericDimension::Vector(Vs::Quad), Scalar::U32)
            }
            Vf::Sint8 | Vf::Sint16 | Vf::Sint32 => (NumericDimension::Scalar, Scalar::I32),
            Vf::Sint8x2 | Vf::Sint16x2 | Vf::Sint32x2 => {
                (NumericDimension::Vector(Vs::Bi), Scalar::I32)
            }
            Vf::Sint32x3 => (NumericDimension::Vector(Vs::Tri), Scalar::I32),
            Vf::Sint8x4 | Vf::Sint16x4 | Vf::Sint32x4 => {
                (NumericDimension::Vector(Vs::Quad), Scalar::I32)
            }
            Vf::Unorm8 | Vf::Unorm16 | Vf::Snorm8 | Vf::Snorm16 | Vf::Float16 | Vf::Float32 => {
                (NumericDimension::Scalar, Scalar::F32)
            }
            Vf::Unorm8x2
            | Vf::Snorm8x2
            | Vf::Unorm16x2
            | Vf::Snorm16x2
            | Vf::Float16x2
            | Vf::Float32x2 => (NumericDimension::Vector(Vs::Bi), Scalar::F32),
            Vf::Float32x3 => (NumericDimension::Vector(Vs::Tri), Scalar::F32),
            Vf::Unorm8x4
            | Vf::Snorm8x4
            | Vf::Unorm16x4
            | Vf::Snorm16x4
            | Vf::Float16x4
            | Vf::Float32x4
            | Vf::Unorm10_10_10_2
            | Vf::Unorm8x4Bgra => (NumericDimension::Vector(Vs::Quad), Scalar::F32),
            Vf::Float64 => (NumericDimension::Scalar, Scalar::F64),
            Vf::Float64x2 => (NumericDimension::Vector(Vs::Bi), Scalar::F64),
            Vf::Float64x3 => (NumericDimension::Vector(Vs::Tri), Scalar::F64),
            Vf::Float64x4 => (NumericDimension::Vector(Vs::Quad), Scalar::F64),
        };

        NumericType {
            dim,
            //Note: Shader always sees data as int, uint, or float.
            // It doesn't know if the original is normalized in a tighter form.
            scalar,
        }
    }

    fn from_texture_format(format: wgt::TextureFormat) -> Self {
        use naga::{Scalar, VectorSize as Vs};
        use wgt::TextureFormat as Tf;

        let (dim, scalar) = match format {
            Tf::R8Unorm | Tf::R8Snorm | Tf::R16Float | Tf::R32Float => {
                (NumericDimension::Scalar, Scalar::F32)
            }
            Tf::R8Uint | Tf::R16Uint | Tf::R32Uint => (NumericDimension::Scalar, Scalar::U32),
            Tf::R8Sint | Tf::R16Sint | Tf::R32Sint => (NumericDimension::Scalar, Scalar::I32),
            Tf::Rg8Unorm | Tf::Rg8Snorm | Tf::Rg16Float | Tf::Rg32Float => {
                (NumericDimension::Vector(Vs::Bi), Scalar::F32)
            }
            Tf::R64Uint => (NumericDimension::Scalar, Scalar::U64),
            Tf::Rg8Uint | Tf::Rg16Uint | Tf::Rg32Uint => {
                (NumericDimension::Vector(Vs::Bi), Scalar::U32)
            }
            Tf::Rg8Sint | Tf::Rg16Sint | Tf::Rg32Sint => {
                (NumericDimension::Vector(Vs::Bi), Scalar::I32)
            }
            Tf::R16Snorm | Tf::R16Unorm => (NumericDimension::Scalar, Scalar::F32),
            Tf::Rg16Snorm | Tf::Rg16Unorm => (NumericDimension::Vector(Vs::Bi), Scalar::F32),
            Tf::Rgba16Snorm | Tf::Rgba16Unorm => (NumericDimension::Vector(Vs::Quad), Scalar::F32),
            Tf::Rgba8Unorm
            | Tf::Rgba8UnormSrgb
            | Tf::Rgba8Snorm
            | Tf::Bgra8Unorm
            | Tf::Bgra8UnormSrgb
            | Tf::Rgb10a2Unorm
            | Tf::Rgba16Float
            | Tf::Rgba32Float => (NumericDimension::Vector(Vs::Quad), Scalar::F32),
            Tf::Rgba8Uint | Tf::Rgba16Uint | Tf::Rgba32Uint | Tf::Rgb10a2Uint => {
                (NumericDimension::Vector(Vs::Quad), Scalar::U32)
            }
            Tf::Rgba8Sint | Tf::Rgba16Sint | Tf::Rgba32Sint => {
                (NumericDimension::Vector(Vs::Quad), Scalar::I32)
            }
            Tf::Rg11b10Ufloat => (NumericDimension::Vector(Vs::Tri), Scalar::F32),
            Tf::Stencil8
            | Tf::Depth16Unorm
            | Tf::Depth32Float
            | Tf::Depth32FloatStencil8
            | Tf::Depth24Plus
            | Tf::Depth24PlusStencil8 => {
                panic!("Unexpected depth format")
            }
            Tf::NV12 => panic!("Unexpected nv12 format"),
            Tf::P010 => panic!("Unexpected p010 format"),
            Tf::Rgb9e5Ufloat | Tf::Rgb5g6b5Unorm => {
                (NumericDimension::Vector(Vs::Tri), Scalar::F32)
            }
            Tf::Rgb5a1Unorm | Tf::Rgba4Unorm => (NumericDimension::Vector(Vs::Quad), Scalar::F32),
            Tf::Bc1RgbaUnorm
            | Tf::Bc1RgbaUnormSrgb
            | Tf::Bc2RgbaUnorm
            | Tf::Bc2RgbaUnormSrgb
            | Tf::Bc3RgbaUnorm
            | Tf::Bc3RgbaUnormSrgb
            | Tf::Bc7RgbaUnorm
            | Tf::Bc7RgbaUnormSrgb
            | Tf::Etc2Rgb8A1Unorm
            | Tf::Etc2Rgb8A1UnormSrgb
            | Tf::Etc2Rgba8Unorm
            | Tf::Etc2Rgba8UnormSrgb => (NumericDimension::Vector(Vs::Quad), Scalar::F32),
            Tf::Bc4RUnorm | Tf::Bc4RSnorm | Tf::EacR11Unorm | Tf::EacR11Snorm => {
                (NumericDimension::Scalar, Scalar::F32)
            }
            Tf::Bc5RgUnorm | Tf::Bc5RgSnorm | Tf::EacRg11Unorm | Tf::EacRg11Snorm => {
                (NumericDimension::Vector(Vs::Bi), Scalar::F32)
            }
            Tf::Bc6hRgbUfloat | Tf::Bc6hRgbFloat | Tf::Etc2Rgb8Unorm | Tf::Etc2Rgb8UnormSrgb => {
                (NumericDimension::Vector(Vs::Tri), Scalar::F32)
            }
            Tf::Astc {
                block: _,
                channel: _,
            } => (NumericDimension::Vector(Vs::Quad), Scalar::F32),
        };

        NumericType {
            dim,
            //Note: Shader always sees data as int, uint, or float.
            // It doesn't know if the original is normalized in a tighter form.
            scalar,
        }
    }

    fn is_subtype_of(&self, other: &NumericType) -> bool {
        if self.scalar.width > other.scalar.width {
            return false;
        }
        if self.scalar.kind != other.scalar.kind {
            return false;
        }
        match (self.dim, other.dim) {
            (NumericDimension::Scalar, NumericDimension::Scalar) => true,
            (NumericDimension::Scalar, NumericDimension::Vector(_)) => true,
            (NumericDimension::Vector(s0), NumericDimension::Vector(s1)) => s0 <= s1,
            (NumericDimension::Matrix(c0, r0), NumericDimension::Matrix(c1, r1)) => {
                c0 == c1 && r0 == r1
            }
            _ => false,
        }
    }
}

/// Return true if the fragment `format` is covered by the provided `output`.
pub fn check_texture_format(
    format: wgt::TextureFormat,
    output: &NumericType,
) -> Result<(), NumericType> {
    let nt = NumericType::from_texture_format(format);
    if nt.is_subtype_of(output) {
        Ok(())
    } else {
        Err(nt)
    }
}

pub enum BindingLayoutSource {
    /// The binding layout is derived from the pipeline layout.
    ///
    /// This will be filled in by the shader binding validation, as it iterates the shader's interfaces.
    Derived(Box<ArrayVec<bgl::EntryMap, { hal::MAX_BIND_GROUPS }>>),
    /// The binding layout is provided by the user in BGLs.
    ///
    /// This will be validated against the shader's interfaces.
    Provided(Arc<crate::binding_model::PipelineLayout>),
}

impl BindingLayoutSource {
    pub fn new_derived(limits: &wgt::Limits) -> Self {
        let mut array = ArrayVec::new();
        for _ in 0..limits.max_bind_groups {
            array.push(Default::default());
        }
        BindingLayoutSource::Derived(Box::new(array))
    }
}

#[derive(Debug, Clone, Default)]
pub struct StageIo {
    pub varyings: FastHashMap<wgt::ShaderLocation, InterfaceVar>,
    /// This must match between mesh & task shaders
    pub task_payload_size: Option<u32>,
    /// Fragment shaders cannot input primitive index on mesh shaders that don't output it on DX12.
    /// Therefore, we track between shader stages if primitive index is written (or if vertex shader
    /// is used).
    ///
    /// This is Some if it was a mesh shader.
    pub primitive_index: Option<bool>,
}

impl Interface {
    fn populate(
        list: &mut Vec<Varying>,
        binding: Option<&naga::Binding>,
        ty: naga::Handle<naga::Type>,
        arena: &naga::UniqueArena<naga::Type>,
    ) {
        let numeric_ty = match arena[ty].inner {
            naga::TypeInner::Scalar(scalar) => NumericType {
                dim: NumericDimension::Scalar,
                scalar,
            },
            naga::TypeInner::Vector { size, scalar } => NumericType {
                dim: NumericDimension::Vector(size),
                scalar,
            },
            naga::TypeInner::Matrix {
                columns,
                rows,
                scalar,
            } => NumericType {
                dim: NumericDimension::Matrix(columns, rows),
                scalar,
            },
            naga::TypeInner::Struct { ref members, .. } => {
                for member in members {
                    Self::populate(list, member.binding.as_ref(), member.ty, arena);
                }
                return;
            }
            naga::TypeInner::Array { base, size, stride }
                if matches!(
                    binding,
                    Some(naga::Binding::BuiltIn(naga::BuiltIn::ClipDistances)),
                ) =>
            {
                // NOTE: We should already have validated these in `naga`.
                debug_assert_eq!(
                    &arena[base].inner,
                    &naga::TypeInner::Scalar(naga::Scalar::F32)
                );
                debug_assert_eq!(stride, 4);

                let naga::ArraySize::Constant(array_size) = size else {
                    // NOTE: Based on the
                    // [spec](https://gpuweb.github.io/gpuweb/wgsl/#fixed-footprint-types):
                    //
                    // > The only valid use of a fixed-size array with an element count that is an
                    // > override-expression that is not a const-expression is as a memory view in
                    // > the workgroup address space.
                    unreachable!("non-constant array size for `clip_distances`")
                };
                let array_size = array_size.get();

                list.push(Varying::BuiltIn(BuiltIn::ClipDistances { array_size }));
                return;
            }
            ref other => {
                //Note: technically this should be at least `log::error`, but
                // the reality is - every shader coming from `glslc` outputs an array
                // of clip distances and hits this path :(
                // So we lower it to `log::debug` to be less annoying as
                // there's nothing the user can do about it.
                log::debug!("Unexpected varying type: {other:?}");
                return;
            }
        };

        let varying = match binding {
            Some(&naga::Binding::Location {
                location,
                interpolation,
                sampling,
                per_primitive,
                blend_src: _,
            }) => Varying::Local {
                location,
                iv: InterfaceVar {
                    ty: numeric_ty,
                    interpolation,
                    sampling,
                    per_primitive,
                },
            },
            Some(&naga::Binding::BuiltIn(built_in)) => Varying::BuiltIn(match built_in {
                naga::BuiltIn::Position { invariant } => BuiltIn::Position { invariant },
                naga::BuiltIn::ViewIndex => BuiltIn::ViewIndex,
                naga::BuiltIn::BaseInstance => BuiltIn::BaseInstance,
                naga::BuiltIn::BaseVertex => BuiltIn::BaseVertex,
                naga::BuiltIn::ClipDistances => unreachable!(),
                naga::BuiltIn::CullDistance => BuiltIn::CullDistance,
                naga::BuiltIn::InstanceIndex => BuiltIn::InstanceIndex,
                naga::BuiltIn::PointSize => BuiltIn::PointSize,
                naga::BuiltIn::VertexIndex => BuiltIn::VertexIndex,
                naga::BuiltIn::DrawIndex => BuiltIn::DrawIndex,
                naga::BuiltIn::FragDepth => BuiltIn::FragDepth,
                naga::BuiltIn::PointCoord => BuiltIn::PointCoord,
                naga::BuiltIn::FrontFacing => BuiltIn::FrontFacing,
                naga::BuiltIn::PrimitiveIndex => BuiltIn::PrimitiveIndex,
                naga::BuiltIn::Barycentric { perspective } => BuiltIn::Barycentric { perspective },
                naga::BuiltIn::SampleIndex => BuiltIn::SampleIndex,
                naga::BuiltIn::SampleMask => BuiltIn::SampleMask,
                naga::BuiltIn::GlobalInvocationId => BuiltIn::GlobalInvocationId,
                naga::BuiltIn::LocalInvocationId => BuiltIn::LocalInvocationId,
                naga::BuiltIn::LocalInvocationIndex => BuiltIn::LocalInvocationIndex,
                naga::BuiltIn::WorkGroupId => BuiltIn::WorkGroupId,
                naga::BuiltIn::WorkGroupSize => BuiltIn::WorkGroupSize,
                naga::BuiltIn::NumWorkGroups => BuiltIn::NumWorkGroups,
                naga::BuiltIn::NumSubgroups => BuiltIn::NumSubgroups,
                naga::BuiltIn::SubgroupId => BuiltIn::SubgroupId,
                naga::BuiltIn::SubgroupSize => BuiltIn::SubgroupSize,
                naga::BuiltIn::SubgroupInvocationId => BuiltIn::SubgroupInvocationId,
                naga::BuiltIn::MeshTaskSize => BuiltIn::MeshTaskSize,
                naga::BuiltIn::CullPrimitive => BuiltIn::CullPrimitive,
                naga::BuiltIn::PointIndex => BuiltIn::PointIndex,
                naga::BuiltIn::LineIndices => BuiltIn::LineIndices,
                naga::BuiltIn::TriangleIndices => BuiltIn::TriangleIndices,
                naga::BuiltIn::VertexCount => BuiltIn::VertexCount,
                naga::BuiltIn::Vertices => BuiltIn::Vertices,
                naga::BuiltIn::PrimitiveCount => BuiltIn::PrimitiveCount,
                naga::BuiltIn::Primitives => BuiltIn::Primitives,
                naga::BuiltIn::RayInvocationId => BuiltIn::RayInvocationId,
                naga::BuiltIn::NumRayInvocations => BuiltIn::NumRayInvocations,
                naga::BuiltIn::InstanceCustomData => BuiltIn::InstanceCustomData,
                naga::BuiltIn::GeometryIndex => BuiltIn::GeometryIndex,
                naga::BuiltIn::WorldRayOrigin => BuiltIn::WorldRayOrigin,
                naga::BuiltIn::WorldRayDirection => BuiltIn::WorldRayDirection,
                naga::BuiltIn::ObjectRayOrigin => BuiltIn::ObjectRayOrigin,
                naga::BuiltIn::ObjectRayDirection => BuiltIn::ObjectRayDirection,
                naga::BuiltIn::RayTmin => BuiltIn::RayTmin,
                naga::BuiltIn::RayTCurrentMax => BuiltIn::RayTCurrentMax,
                naga::BuiltIn::ObjectToWorld => BuiltIn::ObjectToWorld,
                naga::BuiltIn::WorldToObject => BuiltIn::WorldToObject,
                naga::BuiltIn::HitKind => BuiltIn::HitKind,
            }),
            None => {
                log::error!("Missing binding for a varying");
                return;
            }
        };
        list.push(varying);
    }

    pub fn new(
        module: &naga::Module,
        module_info: &naga::valid::ModuleInfo,
        limits: wgt::Limits,
    ) -> Self {
        let mut resources = naga::Arena::new();
        let mut resource_mapping = FastHashMap::default();
        for (var_handle, var) in module.global_variables.iter() {
            let bind = match var.binding {
                Some(br) => br,
                _ => continue,
            };
            let naga_ty = &module.types[var.ty].inner;

            let inner_ty = match *naga_ty {
                naga::TypeInner::BindingArray { base, .. } => &module.types[base].inner,
                ref ty => ty,
            };

            let ty = match *inner_ty {
                naga::TypeInner::Image {
                    dim,
                    arrayed,
                    class,
                } => ResourceType::Texture {
                    dim,
                    arrayed,
                    class,
                },
                naga::TypeInner::Sampler { comparison } => ResourceType::Sampler { comparison },
                naga::TypeInner::AccelerationStructure { vertex_return } => {
                    ResourceType::AccelerationStructure { vertex_return }
                }
                ref other => ResourceType::Buffer {
                    size: wgt::BufferSize::new(other.size(module.to_ctx()) as u64).unwrap(),
                },
            };
            let handle = resources.append(
                Resource {
                    name: var.name.clone(),
                    bind,
                    ty,
                    class: var.space,
                },
                Default::default(),
            );
            resource_mapping.insert(var_handle, handle);
        }

        let immediate_size = naga::valid::ImmediateSlots::size_for_module(module);

        let mut entry_points = FastHashMap::default();
        entry_points.reserve(module.entry_points.len());
        for (index, entry_point) in module.entry_points.iter().enumerate() {
            let info = module_info.get_entry_point(index);
            let mut ep = EntryPoint {
                fragment_position_used: entry_point_uses_fragment_position(
                    module,
                    entry_point,
                    info,
                    module_info,
                ),
                texture_load_used: entry_point_uses_texture_load(module, entry_point),
                unsupported_runtime_integer_semantics:
                    entry_point_uses_unsupported_integer_semantics(
                        module,
                        module_info,
                        entry_point,
                        info,
                    ),
                unsupported_matrix_aggregate_store:
                    entry_point_uses_unsupported_matrix_aggregate_store(
                        module,
                        module_info,
                        entry_point,
                        info,
                    ),
                texture_query_used: entry_point_uses_texture_query(module, entry_point),
                texture_sample_base_clamp_to_edge_used:
                    entry_point_uses_texture_sample_base_clamp_to_edge(module, entry_point),
                texture_sample_biases: entry_point_texture_sample_biases(module, entry_point),
                fragment_derivatives_used: entry_point_uses_fragment_derivatives(
                    module,
                    entry_point,
                ),
                texture_sample_level_used: entry_point_uses_texture_sample_level(
                    module,
                    entry_point,
                ),
                texture_gather_used: entry_point_uses_texture_gather(module, entry_point),
                ..Default::default()
            };
            for arg in entry_point.function.arguments.iter() {
                Self::populate(&mut ep.inputs, arg.binding.as_ref(), arg.ty, &module.types);
            }
            if let Some(ref result) = entry_point.function.result {
                Self::populate(
                    &mut ep.outputs,
                    result.binding.as_ref(),
                    result.ty,
                    &module.types,
                );
            }

            for (var_handle, var) in module.global_variables.iter() {
                let usage = info[var_handle];
                if !usage.is_empty() && var.binding.is_some() {
                    ep.resources.push(resource_mapping[&var_handle]);
                }
            }

            ep.uniform_constant_vectors = ep
                .resources
                .iter()
                .filter_map(|&handle| {
                    let resource = &resources[handle];
                    if resource.class != naga::AddressSpace::Uniform {
                        return None;
                    }
                    match &resource.ty {
                        ResourceType::Buffer { size } => {
                            Some(size.get().div_ceil(16).min(u64::from(u32::MAX)) as u32)
                        }
                        _ => None,
                    }
                })
                .fold(0u32, u32::saturating_add);

            for key in info.sampling_set.iter() {
                ep.sampling_pairs
                    .insert((resource_mapping[&key.image], resource_mapping[&key.sampler]));
            }
            ep.dual_source_blending = info.dual_source_blending;
            ep.workgroup_size = entry_point.workgroup_size;
            ep.immediate_slots_required = info.immediate_slots_used;

            if let Some(task_payload) = entry_point.task_payload {
                ep.task_payload_size = Some(
                    module.types[module.global_variables[task_payload].ty]
                        .inner
                        .size(module.to_ctx()),
                );
            }
            if let Some(ref mesh_info) = entry_point.mesh_info {
                ep.mesh_info = Some(EntryPointMeshInfo {
                    max_vertices: mesh_info.max_vertices,
                    max_primitives: mesh_info.max_primitives,
                    primitive_topology: match mesh_info.topology {
                        naga::MeshOutputTopology::Triangles => wgt::PrimitiveTopology::TriangleList,
                        naga::MeshOutputTopology::Lines => wgt::PrimitiveTopology::LineList,
                        naga::MeshOutputTopology::Points => wgt::PrimitiveTopology::PointList,
                    },
                });
                Self::populate(
                    &mut ep.outputs,
                    None,
                    mesh_info.vertex_output_type,
                    &module.types,
                );
                Self::populate(
                    &mut ep.outputs,
                    None,
                    mesh_info.primitive_output_type,
                    &module.types,
                );
            }

            entry_points.insert((entry_point.stage, entry_point.name.clone()), ep);
        }

        Self {
            limits,
            resources,
            entry_points,
            immediate_size,
        }
    }

    pub fn immediate_slots_required(
        &self,
        stage: naga::ShaderStage,
        entry_point_name: &str,
    ) -> naga::valid::ImmediateSlots {
        self.entry_points
            .get(&(stage, entry_point_name.to_string()))
            .map_or(Default::default(), |ep| ep.immediate_slots_required)
    }

    pub fn sampled_textures_used(
        &self,
        stage: naga::ShaderStage,
        entry_point_name: &str,
    ) -> Result<u32, StageError> {
        let key = (stage, entry_point_name.to_string());
        let Some(entry_point) = self.entry_points.get(&key) else {
            return Err(StageError::MissingEntryPoint(entry_point_name.to_string()));
        };
        let count = entry_point
            .resources
            .iter()
            .filter(|&&handle| {
                matches!(
                    &self.resources[handle].ty,
                    ResourceType::Texture {
                        class: naga::ImageClass::Sampled { .. } | naga::ImageClass::Depth { .. },
                        ..
                    }
                )
            })
            .count();
        Ok(u32::try_from(count).unwrap_or(u32::MAX))
    }

    pub(crate) fn entry_point_uniform_constant_vectors(
        &self,
        stage: naga::ShaderStage,
        entry_point_name: &str,
    ) -> Result<u32, StageError> {
        let key = (stage, entry_point_name.to_string());
        let Some(entry_point) = self.entry_points.get(&key) else {
            return Err(StageError::MissingEntryPoint(entry_point_name.to_string()));
        };
        Ok(entry_point.uniform_constant_vectors)
    }

    pub(crate) fn sampling_pairs_used(
        &self,
        stage: naga::ShaderStage,
        entry_point_name: &str,
    ) -> Result<Vec<SamplingPair>, StageError> {
        let key = (stage, entry_point_name.to_string());
        let Some(entry_point) = self.entry_points.get(&key) else {
            return Err(StageError::MissingEntryPoint(entry_point_name.to_string()));
        };
        let mut pairs = entry_point
            .sampling_pairs
            .iter()
            .map(|&(texture, sampler)| {
                let texture = self.resources[texture].bind;
                let sampler = self.resources[sampler].bind;
                SamplingPair {
                    texture_group: texture.group,
                    texture_binding: texture.binding,
                    sampler_group: sampler.group,
                    sampler_binding: sampler.binding,
                }
            })
            .collect::<Vec<_>>();
        pairs.sort_unstable();
        Ok(pairs)
    }

    pub(crate) fn entry_point_writes_frag_depth(
        &self,
        stage: naga::ShaderStage,
        entry_point_name: &str,
    ) -> Result<bool, StageError> {
        let key = (stage, entry_point_name.to_string());
        let Some(entry_point) = self.entry_points.get(&key) else {
            return Err(StageError::MissingEntryPoint(entry_point_name.to_string()));
        };
        Ok(entry_point
            .outputs
            .contains(&Varying::BuiltIn(BuiltIn::FragDepth)))
    }

    pub(crate) fn entry_point_uses_unsupported_fragment_depth_without_color_target_0(
        &self,
        stage: naga::ShaderStage,
        entry_point_name: &str,
        limits: &wgt::DownlevelLimits,
    ) -> Result<bool, StageError> {
        if limits.fragment_depth_without_color_target_0 {
            return Ok(false);
        }
        let key = (stage, entry_point_name.to_string());
        let Some(entry_point) = self.entry_points.get(&key) else {
            return Err(StageError::MissingEntryPoint(entry_point_name.to_string()));
        };
        let writes_depth = entry_point
            .outputs
            .contains(&Varying::BuiltIn(BuiltIn::FragDepth));
        let writes_color_target_0 = entry_point
            .outputs
            .iter()
            .any(|output| matches!(output, Varying::Local { location: 0, .. }));
        Ok(writes_depth && !writes_color_target_0)
    }

    pub(crate) fn entry_point_uses_unsupported_fragment_sample_mask_output(
        &self,
        stage: naga::ShaderStage,
        entry_point_name: &str,
        limits: &wgt::DownlevelLimits,
    ) -> Result<bool, StageError> {
        if limits.fragment_sample_mask_output {
            return Ok(false);
        }
        let key = (stage, entry_point_name.to_string());
        let Some(entry_point) = self.entry_points.get(&key) else {
            return Err(StageError::MissingEntryPoint(entry_point_name.to_string()));
        };
        Ok(entry_point
            .outputs
            .contains(&Varying::BuiltIn(BuiltIn::SampleMask)))
    }

    pub(crate) fn entry_point_uses_unsupported_fragment_non_perspective_interpolation(
        &self,
        stage: naga::ShaderStage,
        entry_point_name: &str,
        limits: &wgt::DownlevelLimits,
    ) -> Result<bool, StageError> {
        if limits.fragment_non_perspective_interpolation || stage != naga::ShaderStage::Fragment {
            return Ok(false);
        }
        let key = (stage, entry_point_name.to_string());
        let Some(entry_point) = self.entry_points.get(&key) else {
            return Err(StageError::MissingEntryPoint(entry_point_name.to_string()));
        };
        Ok(entry_point.inputs.iter().any(|varying| {
            matches!(
                varying,
                Varying::Local {
                    iv: InterfaceVar {
                        interpolation: Some(
                            naga::Interpolation::Flat | naga::Interpolation::Linear
                        ),
                        ..
                    },
                    ..
                }
            )
        }))
    }

    pub(crate) fn entry_point_uses_front_facing(
        &self,
        stage: naga::ShaderStage,
        entry_point_name: &str,
    ) -> Result<bool, StageError> {
        self.entry_point_uses_input_builtin(stage, entry_point_name, BuiltIn::FrontFacing)
    }

    pub(crate) fn entry_point_uses_vertex_index(
        &self,
        entry_point_name: &str,
    ) -> Result<bool, StageError> {
        self.entry_point_uses_input_builtin(
            naga::ShaderStage::Vertex,
            entry_point_name,
            BuiltIn::VertexIndex,
        )
    }

    pub(crate) fn entry_point_uses_instance_index(
        &self,
        entry_point_name: &str,
    ) -> Result<bool, StageError> {
        self.entry_point_uses_input_builtin(
            naga::ShaderStage::Vertex,
            entry_point_name,
            BuiltIn::InstanceIndex,
        )
    }

    fn entry_point_uses_input_builtin(
        &self,
        stage: naga::ShaderStage,
        entry_point_name: &str,
        builtin: BuiltIn,
    ) -> Result<bool, StageError> {
        let key = (stage, entry_point_name.to_string());
        let Some(entry_point) = self.entry_points.get(&key) else {
            return Err(StageError::MissingEntryPoint(entry_point_name.to_string()));
        };
        Ok(entry_point.inputs.contains(&Varying::BuiltIn(builtin)))
    }

    pub(crate) fn entry_point_uses_fragment_position(
        &self,
        stage: naga::ShaderStage,
        entry_point_name: &str,
    ) -> Result<bool, StageError> {
        let key = (stage, entry_point_name.to_string());
        let Some(entry_point) = self.entry_points.get(&key) else {
            return Err(StageError::MissingEntryPoint(entry_point_name.to_string()));
        };
        Ok(entry_point.fragment_position_used)
    }

    pub(crate) fn entry_point_uses_texture_load(
        &self,
        stage: naga::ShaderStage,
        entry_point_name: &str,
    ) -> Result<bool, StageError> {
        let key = (stage, entry_point_name.to_string());
        let Some(entry_point) = self.entry_points.get(&key) else {
            return Err(StageError::MissingEntryPoint(entry_point_name.to_string()));
        };
        Ok(entry_point.texture_load_used)
    }

    pub(crate) fn entry_point_uses_unsupported_runtime_integer_semantics(
        &self,
        stage: naga::ShaderStage,
        entry_point_name: &str,
        limits: &wgt::DownlevelLimits,
    ) -> Result<bool, StageError> {
        let key = (stage, entry_point_name.to_string());
        let Some(entry_point) = self.entry_points.get(&key) else {
            return Err(StageError::MissingEntryPoint(entry_point_name.to_string()));
        };
        Ok(entry_point.unsupported_runtime_integer_semantics
            && !limits.runtime_integer_shader_operations)
    }

    pub(crate) fn entry_point_uses_unsupported_matrix_aggregate_store(
        &self,
        stage: naga::ShaderStage,
        entry_point_name: &str,
        limits: &wgt::DownlevelLimits,
    ) -> Result<bool, StageError> {
        let key = (stage, entry_point_name.to_string());
        let Some(entry_point) = self.entry_points.get(&key) else {
            return Err(StageError::MissingEntryPoint(entry_point_name.to_string()));
        };
        Ok(entry_point.unsupported_matrix_aggregate_store && !limits.matrix_aggregate_stores)
    }

    pub(crate) fn entry_point_uses_unsupported_texture_query(
        &self,
        stage: naga::ShaderStage,
        entry_point_name: &str,
        limits: &wgt::DownlevelLimits,
    ) -> Result<bool, StageError> {
        let key = (stage, entry_point_name.to_string());
        let Some(entry_point) = self.entry_points.get(&key) else {
            return Err(StageError::MissingEntryPoint(entry_point_name.to_string()));
        };
        Ok(entry_point.texture_query_used && !limits.texture_query)
    }

    pub(crate) fn entry_point_uses_unsupported_texture_sample_base_clamp_to_edge(
        &self,
        stage: naga::ShaderStage,
        entry_point_name: &str,
        limits: &wgt::DownlevelLimits,
    ) -> Result<bool, StageError> {
        let key = (stage, entry_point_name.to_string());
        let Some(entry_point) = self.entry_points.get(&key) else {
            return Err(StageError::MissingEntryPoint(entry_point_name.to_string()));
        };
        Ok(entry_point.texture_sample_base_clamp_to_edge_used
            && !limits.texture_sample_base_clamp_to_edge)
    }

    pub(crate) fn entry_point_uses_unsupported_texture_sample_bias(
        &self,
        stage: naga::ShaderStage,
        entry_point_name: &str,
        limits: &wgt::DownlevelLimits,
    ) -> Result<bool, StageError> {
        let key = (stage, entry_point_name.to_string());
        let Some(entry_point) = self.entry_points.get(&key) else {
            return Err(StageError::MissingEntryPoint(entry_point_name.to_string()));
        };
        Ok(entry_point.texture_sample_biases.iter().any(|bias| {
            (bias.has_offset && !limits.texture_sample_bias_offset)
                || match bias.literal {
                    Some(value) => {
                        limits.max_texture_sample_bias != u32::MAX
                            && (!value.is_finite()
                                || value.abs() > limits.max_texture_sample_bias as f32)
                    }
                    None => !limits.texture_sample_bias_dynamic,
                }
        }))
    }

    pub(crate) fn entry_point_uses_unsupported_fragment_derivatives(
        &self,
        stage: naga::ShaderStage,
        entry_point_name: &str,
        limits: &wgt::DownlevelLimits,
    ) -> Result<bool, StageError> {
        let key = (stage, entry_point_name.to_string());
        let Some(entry_point) = self.entry_points.get(&key) else {
            return Err(StageError::MissingEntryPoint(entry_point_name.to_string()));
        };
        Ok(stage == naga::ShaderStage::Fragment
            && entry_point.fragment_derivatives_used
            && !limits.fragment_derivatives)
    }

    pub(crate) fn entry_point_uses_unsupported_texture_sample_level(
        &self,
        stage: naga::ShaderStage,
        entry_point_name: &str,
        limits: &wgt::DownlevelLimits,
    ) -> Result<bool, StageError> {
        let key = (stage, entry_point_name.to_string());
        let Some(entry_point) = self.entry_points.get(&key) else {
            return Err(StageError::MissingEntryPoint(entry_point_name.to_string()));
        };
        Ok(entry_point.texture_sample_level_used && !limits.texture_sample_level)
    }

    pub(crate) fn entry_point_uses_unsupported_texture_gather(
        &self,
        stage: naga::ShaderStage,
        entry_point_name: &str,
        limits: &wgt::DownlevelLimits,
    ) -> Result<bool, StageError> {
        let key = (stage, entry_point_name.to_string());
        let Some(entry_point) = self.entry_points.get(&key) else {
            return Err(StageError::MissingEntryPoint(entry_point_name.to_string()));
        };
        Ok(entry_point.texture_gather_used && !limits.texture_gather)
    }

    pub fn finalize_entry_point_name(
        &self,
        stage: naga::ShaderStage,
        entry_point_name: Option<&str>,
    ) -> Result<String, StageError> {
        entry_point_name
            .map(|ep| ep.to_string())
            .map(Ok)
            .unwrap_or_else(|| {
                let mut entry_points = self
                    .entry_points
                    .keys()
                    .filter_map(|(ep_stage, name)| (ep_stage == &stage).then_some(name));
                let first = entry_points.next().ok_or(StageError::NoEntryPointFound)?;
                if entry_points.next().is_some() {
                    return Err(StageError::MultipleEntryPointsFound);
                }
                Ok(first.clone())
            })
    }

    /// Among other things, this implements some validation logic defined by the WebGPU spec. at
    /// <https://www.w3.org/TR/webgpu/#abstract-opdef-validating-inter-stage-interfaces>.
    pub fn check_stage(
        &self,
        layouts: &mut BindingLayoutSource,
        shader_binding_sizes: &mut FastHashMap<naga::ResourceBinding, wgt::BufferSize>,
        entry_point_name: &str,
        shader_stage: ShaderStageForValidation,
        inputs: StageIo,
        primitive_topology: Option<wgt::PrimitiveTopology>,
    ) -> Result<StageIo, StageError> {
        // Since a shader module can have multiple entry points with the same name,
        // we need to look for one with the right execution model.
        let pair = (shader_stage.to_naga(), entry_point_name.to_string());
        let entry_point = match self.entry_points.get(&pair) {
            Some(some) => some,
            None => return Err(StageError::MissingEntryPoint(pair.1)),
        };
        let (_, entry_point_name) = pair;

        let stage_bit = shader_stage.to_wgt_bit();

        // check resources visibility
        for &handle in entry_point.resources.iter() {
            let res = &self.resources[handle];
            let result = 'err: {
                match layouts {
                    BindingLayoutSource::Provided(pipeline_layout) => {
                        // update the required binding size for this buffer
                        if let ResourceType::Buffer { size } = res.ty {
                            match shader_binding_sizes.entry(res.bind) {
                                Entry::Occupied(e) => {
                                    *e.into_mut() = size.max(*e.get());
                                }
                                Entry::Vacant(e) => {
                                    e.insert(size);
                                }
                            }
                        }

                        let Some(entry) =
                            pipeline_layout.get_bgl_entry(res.bind.group, res.bind.binding)
                        else {
                            break 'err Err(BindingError::Missing);
                        };

                        if !entry.visibility.contains(stage_bit) {
                            break 'err Err(BindingError::Invisible);
                        }

                        res.check_binding_use(entry)
                    }
                    BindingLayoutSource::Derived(layouts) => {
                        let Some(map) = layouts.get_mut(res.bind.group as usize) else {
                            break 'err Err(BindingError::Missing);
                        };

                        let ty = match res.derive_binding_type(
                            entry_point
                                .sampling_pairs
                                .iter()
                                .any(|&(im, _samp)| im == handle),
                        ) {
                            Ok(ty) => ty,
                            Err(error) => break 'err Err(error),
                        };

                        match map.entry(res.bind.binding) {
                            indexmap::map::Entry::Occupied(e) if e.get().ty != ty => {
                                break 'err Err(BindingError::InconsistentlyDerivedType)
                            }
                            indexmap::map::Entry::Occupied(e) => {
                                e.into_mut().visibility |= stage_bit;
                            }
                            indexmap::map::Entry::Vacant(e) => {
                                e.insert(BindGroupLayoutEntry {
                                    binding: res.bind.binding,
                                    ty,
                                    visibility: stage_bit,
                                    count: None,
                                });
                            }
                        }
                        Ok(())
                    }
                }
            };
            if let Err(error) = result {
                return Err(StageError::Binding(res.bind, error));
            }
        }

        // Check the compatibility between textures and samplers
        //
        // We only need to do this if the binding layout is provided by the user, as derived
        // layouts will inherently be correctly tagged.
        if let BindingLayoutSource::Provided(pipeline_layout) = layouts {
            for &(texture_handle, sampler_handle) in entry_point.sampling_pairs.iter() {
                let texture_bind = &self.resources[texture_handle].bind;
                let sampler_bind = &self.resources[sampler_handle].bind;
                let texture_layout = pipeline_layout
                    .get_bgl_entry(texture_bind.group, texture_bind.binding)
                    .unwrap();
                let sampler_layout = pipeline_layout
                    .get_bgl_entry(sampler_bind.group, sampler_bind.binding)
                    .unwrap();
                assert!(texture_layout.visibility.contains(stage_bit));
                assert!(sampler_layout.visibility.contains(stage_bit));

                let sampler_filtering = matches!(
                    sampler_layout.ty,
                    BindingType::Sampler(wgt::SamplerBindingType::Filtering)
                );
                let texture_sample_type = match texture_layout.ty {
                    BindingType::Texture { sample_type, .. } => sample_type,
                    BindingType::ExternalTexture => {
                        wgt::TextureSampleType::Float { filterable: true }
                    }
                    _ => unreachable!(),
                };

                let error = match (sampler_filtering, texture_sample_type) {
                    (true, wgt::TextureSampleType::Float { filterable: false }) => {
                        Some(FilteringError::Float)
                    }
                    (true, wgt::TextureSampleType::Sint) => Some(FilteringError::Integer),
                    (true, wgt::TextureSampleType::Uint) => Some(FilteringError::Integer),
                    _ => None,
                };

                if let Some(error) = error {
                    return Err(StageError::Filtering {
                        texture: *texture_bind,
                        sampler: *sampler_bind,
                        error,
                    });
                }
            }
        }

        // check workgroup size limits
        if shader_stage.to_naga().compute_like() {
            let workgroup_size_check = match shader_stage.to_naga() {
                naga::ShaderStage::Compute => WorkgroupSizeCheck {
                    dimensions: &entry_point.workgroup_size,
                    per_dimension_limits: &[
                        self.limits.max_compute_workgroup_size_x,
                        self.limits.max_compute_workgroup_size_y,
                        self.limits.max_compute_workgroup_size_z,
                    ],
                    per_dimension_limits_desc: "max_compute_workgroup_size_*",

                    total_limit: self.limits.max_compute_invocations_per_workgroup,
                    total_limit_desc: "max_compute_invocations_per_workgroup",
                },
                naga::ShaderStage::Task => WorkgroupSizeCheck {
                    dimensions: &entry_point.workgroup_size,
                    per_dimension_limits: &[self.limits.max_task_invocations_per_dimension; 3],
                    per_dimension_limits_desc: "max_task_invocations_per_dimension",

                    total_limit: self.limits.max_task_invocations_per_workgroup,
                    total_limit_desc: "max_task_invocations_per_workgroup",
                },
                naga::ShaderStage::Mesh => WorkgroupSizeCheck {
                    dimensions: &entry_point.workgroup_size,
                    per_dimension_limits: &[self.limits.max_mesh_invocations_per_dimension; 3],
                    per_dimension_limits_desc: "max_mesh_invocations_per_dimension",

                    total_limit: self.limits.max_mesh_invocations_per_workgroup,
                    total_limit_desc: "max_mesh_invocations_per_workgroup",
                },
                _ => unreachable!(),
            };
            let total = workgroup_size_check.check_and_compute_total_invocations()?;
            if total == 0 {
                return Err(StageError::InvalidWorkgroupSize(
                    InvalidWorkgroupSizeError::Zero {
                        dimensions: entry_point.workgroup_size,
                    },
                ));
            }
        }

        let mut this_stage_primitive_index = false;
        let mut has_draw_id = false;
        let mut has_per_vertex = false;

        // check inputs compatibility
        for input in entry_point.inputs.iter() {
            match *input {
                Varying::Local { location, ref iv } => {
                    let result = inputs
                        .varyings
                        .get(&location)
                        .ok_or(InputError::Missing)
                        .and_then(|provided| {
                            let (compatible, per_primitive_correct) = match shader_stage.to_naga() {
                                // For vertex attributes, there are defaults filled out
                                // by the driver if data is not provided.
                                naga::ShaderStage::Vertex => {
                                    let is_compatible =
                                        iv.ty.scalar.kind == provided.ty.scalar.kind;
                                    // vertex inputs don't count towards inter-stage
                                    (is_compatible, !iv.per_primitive)
                                }
                                naga::ShaderStage::Fragment => {
                                    if iv.interpolation != provided.interpolation {
                                        return Err(InputError::InterpolationMismatch(
                                            provided.interpolation,
                                        ));
                                    }
                                    if iv.sampling != provided.sampling {
                                        return Err(InputError::SamplingMismatch(
                                            provided.sampling,
                                        ));
                                    }
                                    (
                                        iv.ty.is_subtype_of(&provided.ty),
                                        iv.per_primitive == provided.per_primitive,
                                    )
                                }
                                // These can't have varying inputs
                                naga::ShaderStage::Compute
                                | naga::ShaderStage::Task
                                | naga::ShaderStage::Mesh => (false, false),
                                naga::ShaderStage::RayGeneration
                                | naga::ShaderStage::AnyHit
                                | naga::ShaderStage::ClosestHit
                                | naga::ShaderStage::Miss => {
                                    unreachable!()
                                }
                            };
                            if !compatible {
                                return Err(InputError::WrongType(provided.ty));
                            } else if !per_primitive_correct {
                                return Err(InputError::WrongPerPrimitive {
                                    pipeline_input: provided.per_primitive,
                                    shader: iv.per_primitive,
                                });
                            }
                            Ok(())
                        });

                    if let Err(error) = result {
                        return Err(StageError::Input {
                            location,
                            var: iv.clone(),
                            error,
                        });
                    }
                    has_per_vertex |= iv.interpolation == Some(naga::Interpolation::PerVertex);
                }
                Varying::BuiltIn(BuiltIn::PrimitiveIndex) => {
                    this_stage_primitive_index = true;
                }
                Varying::BuiltIn(BuiltIn::DrawIndex) => {
                    has_draw_id = true;
                }
                Varying::BuiltIn(_) => {}
            }
        }

        match shader_stage {
            ShaderStageForValidation::Vertex {
                topology,
                compare_function,
            } => {
                let mut max_vertex_shader_output_variables =
                    i64::from(self.limits.max_inter_stage_shader_variables);
                let mut max_vertex_shader_output_location = max_vertex_shader_output_variables - 1;

                let point_list_deduction = if topology == wgt::PrimitiveTopology::PointList {
                    Some(MaxVertexShaderOutputDeduction::PointListPrimitiveTopology)
                } else {
                    None
                };

                let clip_distance_deductions = entry_point.outputs.iter().filter_map(|output| {
                    if let &Varying::BuiltIn(BuiltIn::ClipDistances { array_size }) = output {
                        Some(MaxVertexShaderOutputDeduction::ClipDistances { array_size })
                    } else {
                        None
                    }
                });
                debug_assert!(
                    clip_distance_deductions.clone().count() <= 1,
                    "multiple `clip_distances` outputs found"
                );

                let deductions = point_list_deduction
                    .into_iter()
                    .chain(clip_distance_deductions);

                for deduction in deductions.clone() {
                    max_vertex_shader_output_variables -= i64::from(deduction.for_variables());
                    max_vertex_shader_output_location -= i64::from(deduction.for_location());
                }

                let mut num_user_defined_outputs = 0u32;

                for output in entry_point.outputs.iter() {
                    match *output {
                        Varying::Local { ref iv, location } => {
                            if i64::from(location) > max_vertex_shader_output_location {
                                return Err(StageError::VertexOutputLocationTooLarge {
                                    location,
                                    var: iv.clone(),
                                    limit: self.limits.max_inter_stage_shader_variables,
                                    deductions: deductions.collect(),
                                });
                            }
                            num_user_defined_outputs += 1;
                        }
                        Varying::BuiltIn(_) => {}
                    };

                    if let Some(
                        cmp @ wgt::CompareFunction::Equal | cmp @ wgt::CompareFunction::NotEqual,
                    ) = compare_function
                    {
                        if let Varying::BuiltIn(BuiltIn::Position { invariant: false }) = *output {
                            log::warn!(
                                concat!(
                                    "Vertex shader with entry point {} outputs a ",
                                    "@builtin(position) without the @invariant attribute and ",
                                    "is used in a pipeline with {cmp:?}. On some machines, ",
                                    "this can cause bad artifacting as {cmp:?} assumes the ",
                                    "values output from the vertex shader exactly match the ",
                                    "value in the depth buffer. The @invariant attribute on the ",
                                    "@builtin(position) vertex output ensures that the exact ",
                                    "same pixel depths are used every render."
                                ),
                                entry_point_name,
                                cmp = cmp
                            );
                        }
                    }
                }

                if i64::from(num_user_defined_outputs) > max_vertex_shader_output_variables {
                    return Err(StageError::TooManyUserDefinedVertexOutputs {
                        num_found: num_user_defined_outputs,
                        limit: self.limits.max_inter_stage_shader_variables,
                        deductions: deductions.collect(),
                    });
                }
            }
            ShaderStageForValidation::Fragment {
                dual_source_blending,
                has_depth_attachment,
            } => {
                let mut max_fragment_shader_input_variables =
                    i64::from(self.limits.max_inter_stage_shader_variables);

                let deductions = entry_point.inputs.iter().filter_map(|output| match output {
                    Varying::Local { .. } => None,
                    Varying::BuiltIn(builtin) => {
                        MaxFragmentShaderInputDeduction::from_inter_stage_builtin(builtin.to_naga())
                            .or_else(|| {
                                unreachable!(
                                    concat!(
                                        "unexpected built-in provided; ",
                                        "{:?} is not used for fragment stage input",
                                    ),
                                    builtin
                                )
                            })
                    }
                });

                for deduction in deductions.clone() {
                    max_fragment_shader_input_variables -= i64::from(deduction.for_variables());
                }

                let mut num_user_defined_inputs = 0u32;

                for output in entry_point.inputs.iter() {
                    match *output {
                        Varying::Local { ref iv, location } => {
                            if location >= self.limits.max_inter_stage_shader_variables {
                                return Err(StageError::FragmentInputLocationTooLarge {
                                    location,
                                    var: iv.clone(),
                                    limit: self.limits.max_inter_stage_shader_variables,
                                    deductions: deductions.collect(),
                                });
                            }
                            num_user_defined_inputs += 1;
                        }
                        Varying::BuiltIn(_) => {}
                    };
                }

                if i64::from(num_user_defined_inputs) > max_fragment_shader_input_variables {
                    return Err(StageError::TooManyUserDefinedFragmentInputs {
                        num_found: num_user_defined_inputs,
                        limit: self.limits.max_inter_stage_shader_variables,
                        deductions: deductions.collect(),
                    });
                }

                for output in &entry_point.outputs {
                    let &Varying::Local { location, ref iv } = output else {
                        continue;
                    };
                    if location >= self.limits.max_color_attachments {
                        return Err(StageError::ColorAttachmentLocationTooLarge {
                            location,
                            var: iv.clone(),
                            limit: self.limits.max_color_attachments,
                        });
                    }
                }

                // If the pipeline uses dual-source blending, then the shader
                // must configure appropriate I/O, but it is not an error to
                // use a shader that defines the I/O in a pipeline that only
                // uses one blend source.
                if dual_source_blending && !entry_point.dual_source_blending {
                    return Err(StageError::InvalidDualSourceBlending);
                }

                if entry_point
                    .outputs
                    .contains(&Varying::BuiltIn(BuiltIn::FragDepth))
                    && !has_depth_attachment
                {
                    return Err(StageError::MissingFragDepthAttachment);
                }
            }
            ShaderStageForValidation::Mesh => {
                for output in &entry_point.outputs {
                    if matches!(output, Varying::BuiltIn(BuiltIn::PrimitiveIndex)) {
                        this_stage_primitive_index = true;
                    }
                }
            }
            _ => (),
        }

        if let Some(ref mesh_info) = entry_point.mesh_info {
            if mesh_info.max_vertices > self.limits.max_mesh_output_vertices {
                return Err(StageError::TooManyMeshVertices {
                    limit: self.limits.max_mesh_output_vertices,
                    value: mesh_info.max_vertices,
                });
            }
            if mesh_info.max_primitives > self.limits.max_mesh_output_primitives {
                return Err(StageError::TooManyMeshPrimitives {
                    limit: self.limits.max_mesh_output_primitives,
                    value: mesh_info.max_primitives,
                });
            }
            if primitive_topology != Some(mesh_info.primitive_topology) {
                return Err(StageError::MeshTopologyMismatch);
            }
        }
        if let Some(task_payload_size) = entry_point.task_payload_size {
            if task_payload_size > self.limits.max_task_payload_size {
                return Err(StageError::TaskPayloadTooLarge {
                    limit: self.limits.max_task_payload_size,
                    value: task_payload_size,
                });
            }
        }
        if shader_stage.to_naga() == naga::ShaderStage::Mesh
            && entry_point.task_payload_size != inputs.task_payload_size
        {
            return Err(StageError::TaskPayloadMustMatch {
                input: inputs.task_payload_size,
                shader: entry_point.task_payload_size,
            });
        }

        // Fragment shader primitive index is treated like a varying
        if shader_stage.to_naga() == naga::ShaderStage::Fragment
            && this_stage_primitive_index
            && inputs.primitive_index == Some(false)
        {
            return Err(StageError::InvalidPrimitiveIndex);
        } else if shader_stage.to_naga() == naga::ShaderStage::Fragment
            && !this_stage_primitive_index
            && inputs.primitive_index == Some(true)
        {
            return Err(StageError::MissingPrimitiveIndex);
        }
        if shader_stage.to_naga() == naga::ShaderStage::Mesh
            && inputs.task_payload_size.is_some()
            && has_draw_id
        {
            return Err(StageError::DrawIdError);
        }

        if primitive_topology.is_none_or(|e| !e.is_triangles()) && has_per_vertex {
            return Err(StageError::PerVertexNotTriangles);
        }

        let outputs = entry_point
            .outputs
            .iter()
            .filter_map(|output| match *output {
                Varying::Local { location, ref iv } => Some((location, iv.clone())),
                Varying::BuiltIn(_) => None,
            })
            .collect();

        Ok(StageIo {
            task_payload_size: entry_point.task_payload_size,
            varyings: outputs,
            primitive_index: if shader_stage.to_naga() == naga::ShaderStage::Mesh {
                Some(this_stage_primitive_index)
            } else {
                None
            },
        })
    }

    pub fn fragment_uses_dual_source_blending(
        &self,
        entry_point_name: &str,
    ) -> Result<bool, StageError> {
        let pair = (naga::ShaderStage::Fragment, entry_point_name.to_string());
        self.entry_points
            .get(&pair)
            .ok_or(StageError::MissingEntryPoint(pair.1))
            .map(|ep| ep.dual_source_blending)
    }
}

pub fn check_color_attachment_count(
    num_attachments: usize,
    limit: u32,
) -> Result<(), ColorAttachmentError> {
    let limit = usize::try_from(limit).unwrap();
    if num_attachments > limit {
        return Err(ColorAttachmentError::TooMany {
            given: num_attachments,
            limit,
        });
    }

    Ok(())
}

/// Validate a list of color attachment formats against `maxColorAttachmentBytesPerSample`.
///
/// The color attachments can be from a render pass descriptor or a pipeline descriptor.
///
/// Implements <https://gpuweb.github.io/gpuweb/#abstract-opdef-calculating-color-attachment-bytes-per-sample>.
pub fn validate_color_attachment_bytes_per_sample(
    attachment_formats: impl IntoIterator<Item = wgt::TextureFormat>,
    limit: u32,
) -> Result<(), ColorAttachmentError> {
    let mut total_bytes_per_sample: u32 = 0;
    for format in attachment_formats {
        let byte_cost = format.target_pixel_byte_cost().unwrap();
        let alignment = format.target_component_alignment().unwrap();

        total_bytes_per_sample = total_bytes_per_sample.next_multiple_of(alignment);
        total_bytes_per_sample += byte_cost;
    }

    if total_bytes_per_sample > limit {
        return Err(ColorAttachmentError::TooManyBytesPerSample {
            total: total_bytes_per_sample,
            limit,
        });
    }

    Ok(())
}

#[derive(Clone, Debug, Error)]
pub enum InvalidWorkgroupSizeError {
    #[error(
        "Workgroup size {dimensions:?} ({total} total invocations) must be less or equal to \
        the per-dimension limit `Limits::{per_dimension_limits_desc}` of {per_dimension_limits:?} \
        and the total invocation limit `Limits::{total_limit_desc}` of {total_limit}"
    )]
    LimitExceeded {
        dimensions: [u32; 3],
        per_dimension_limits: [u32; 3],
        per_dimension_limits_desc: &'static str,
        total: u32,
        total_limit: u32,
        total_limit_desc: &'static str,
    },
    #[error("Workgroup sizes {dimensions:?} must be positive")]
    Zero { dimensions: [u32; 3] },
}

/// A helper type for avoiding argument order mistakes when calling
/// [`Self::check_and_compute_total_invocations`].
#[derive(Clone, Debug)]
pub(crate) struct WorkgroupSizeCheck<'a> {
    pub dimensions: &'a [u32; 3],
    pub per_dimension_limits: &'a [u32; 3],
    pub per_dimension_limits_desc: &'static str,
    pub total_limit: u32,
    pub total_limit_desc: &'static str,
}

impl WorkgroupSizeCheck<'_> {
    /// Check X/Y/Z workgroup sizes against per-dimension and overall limits.
    ///
    /// This function does not check that the sizes are non-zero. In a dispatch, it is legal for
    /// the size to be zero. In shader or pipeline creation, it is an error for the size to be
    /// zero, and the caller must check that.
    pub(crate) fn check_and_compute_total_invocations(
        self,
    ) -> Result<u32, InvalidWorkgroupSizeError> {
        let Self {
            dimensions,
            per_dimension_limits,
            per_dimension_limits_desc,
            total_limit,
            total_limit_desc,
        } = self;

        let total = dimensions
            .iter()
            .fold(1u32, |total, &dim| total.saturating_mul(dim));

        let invalid_total_invocations = total > total_limit;

        let dimension_too_large = dimensions
            .iter()
            .zip(per_dimension_limits.iter())
            .any(|(dim, limit)| dim > limit);

        if invalid_total_invocations || dimension_too_large {
            Err(InvalidWorkgroupSizeError::LimitExceeded {
                dimensions: *dimensions,
                per_dimension_limits: *per_dimension_limits,
                per_dimension_limits_desc,
                total,
                total_limit,
                total_limit_desc,
            })
        } else {
            Ok(total)
        }
    }
}

pub enum ShaderStageForValidation {
    Vertex {
        topology: wgt::PrimitiveTopology,
        compare_function: Option<wgt::CompareFunction>,
    },
    Mesh,
    Fragment {
        dual_source_blending: bool,
        has_depth_attachment: bool,
    },
    Compute,
    Task,
}

impl ShaderStageForValidation {
    pub fn to_naga(&self) -> naga::ShaderStage {
        match self {
            Self::Vertex { .. } => naga::ShaderStage::Vertex,
            Self::Mesh => naga::ShaderStage::Mesh,
            Self::Fragment { .. } => naga::ShaderStage::Fragment,
            Self::Compute => naga::ShaderStage::Compute,
            Self::Task => naga::ShaderStage::Task,
        }
    }

    pub fn to_wgt_bit(&self) -> wgt::ShaderStages {
        match self {
            Self::Vertex { .. } => wgt::ShaderStages::VERTEX,
            Self::Mesh => wgt::ShaderStages::MESH,
            Self::Fragment { .. } => wgt::ShaderStages::FRAGMENT,
            Self::Compute => wgt::ShaderStages::COMPUTE,
            Self::Task => wgt::ShaderStages::TASK,
        }
    }
}

#[cfg(test)]
mod tests {
    use alloc::{string::ToString, vec::Vec};

    use super::{Interface, InterfaceVar, SamplingPair, StageError};

    #[test]
    fn fragment_location_error_formats_zero_limit() {
        let error = StageError::FragmentInputLocationTooLarge {
            location: 0,
            var: InterfaceVar::vertex_attribute(wgt::VertexFormat::Float32),
            limit: 0,
            deductions: Vec::new(),
        };
        assert!(error.to_string().contains("-1, 0-based"));
    }

    #[test]
    fn sampled_texture_count_uses_the_selected_entry_point() {
        let module = naga::front::wgsl::parse_str(
            r#"
            @group(0) @binding(0) var image: texture_2d<f32>;
            @group(0) @binding(1) var smp: sampler;
            fn sample_helper() -> vec4f {
                return textureSampleLevel(image, smp, vec2f(0.5), 0.0);
            }
            fn load_helper() -> vec4f {
                return textureLoad(image, vec2i(0), 0);
            }
            @vertex fn sampled() -> @builtin(position) vec4f {
                return sample_helper();
            }
            @vertex fn loaded() -> @builtin(position) vec4f {
                return textureLoad(image, vec2i(0), 0);
            }
            @vertex fn loaded_helper() -> @builtin(position) vec4f {
                return load_helper();
            }
            @vertex fn unused() -> @builtin(position) vec4f {
                return vec4f(0.0, 0.0, 0.0, 1.0);
            }
            @fragment fn fragment() -> @location(0) vec4f {
                return textureSample(image, smp, vec2f(0.5));
            }
            "#,
        )
        .unwrap();
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .unwrap();
        let interface = Interface::new(&module, &info, wgt::Limits::defaults());

        assert_eq!(
            interface
                .sampled_textures_used(naga::ShaderStage::Vertex, "sampled")
                .unwrap(),
            1
        );
        assert_eq!(
            interface
                .sampled_textures_used(naga::ShaderStage::Vertex, "loaded")
                .unwrap(),
            1
        );
        assert_eq!(
            interface
                .sampled_textures_used(naga::ShaderStage::Vertex, "unused")
                .unwrap(),
            0
        );
        assert_eq!(
            interface
                .sampled_textures_used(naga::ShaderStage::Fragment, "fragment")
                .unwrap(),
            1
        );
        for entry in ["loaded", "loaded_helper"] {
            assert!(interface
                .entry_point_uses_texture_load(naga::ShaderStage::Vertex, entry)
                .unwrap());
        }
        for entry in ["sampled", "unused"] {
            assert!(!interface
                .entry_point_uses_texture_load(naga::ShaderStage::Vertex, entry)
                .unwrap());
        }
        assert!(!interface
            .entry_point_uses_texture_load(naga::ShaderStage::Fragment, "fragment")
            .unwrap());
        assert!(interface
            .entry_point_uses_texture_load(naga::ShaderStage::Vertex, "missing")
            .is_err());
        let expected_pair = SamplingPair {
            texture_group: 0,
            texture_binding: 0,
            sampler_group: 0,
            sampler_binding: 1,
        };
        assert_eq!(
            interface
                .sampling_pairs_used(naga::ShaderStage::Vertex, "sampled")
                .unwrap(),
            [expected_pair]
        );
        assert!(interface
            .sampling_pairs_used(naga::ShaderStage::Vertex, "unused")
            .unwrap()
            .is_empty());
        assert!(interface
            .sampled_textures_used(naga::ShaderStage::Vertex, "missing")
            .is_err());
    }

    #[test]
    fn uniform_constant_vectors_use_selected_entry_point_resources() {
        let module = naga::front::wgsl::parse_str(
            r#"
            @group(0) @binding(3) var<uniform> first: array<vec4f, 17>;
            @group(0) @binding(42) var<uniform> second: array<vec4f, 15>;
            @group(0) @binding(99) var<uniform> unused: array<vec4f, 64>;

            fn read_first() -> vec4f {
                return first[16];
            }

            @vertex fn vertex_main() -> @builtin(position) vec4f {
                return first[0];
            }

            @fragment fn fragment_main() -> @location(0) vec4f {
                return read_first() + second[14];
            }

            @fragment fn second_only() -> @location(0) vec4f {
                return second[0];
            }

            @fragment fn no_uniforms() -> @location(0) vec4f {
                return vec4f(1.0);
            }
            "#,
        )
        .unwrap();
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .unwrap();
        let interface = Interface::new(&module, &info, wgt::Limits::defaults());

        assert_eq!(
            interface
                .entry_point_uniform_constant_vectors(naga::ShaderStage::Fragment, "fragment_main")
                .unwrap(),
            32
        );
        assert_eq!(
            interface
                .entry_point_uniform_constant_vectors(naga::ShaderStage::Vertex, "vertex_main")
                .unwrap(),
            17
        );
        assert_eq!(
            interface
                .entry_point_uniform_constant_vectors(naga::ShaderStage::Fragment, "second_only")
                .unwrap(),
            15
        );
        assert_eq!(
            interface
                .entry_point_uniform_constant_vectors(naga::ShaderStage::Fragment, "no_uniforms")
                .unwrap(),
            0
        );
        assert!(interface
            .entry_point_uniform_constant_vectors(naga::ShaderStage::Fragment, "missing")
            .is_err());
    }

    #[test]
    fn texture_sample_bias_limits_follow_selected_entry_and_helper_calls() {
        let module = naga::front::wgsl::parse_str(
            r#"
            @group(0) @binding(0) var image: texture_2d<f32>;
            @group(0) @binding(1) var smp: sampler;
            fn dynamic_helper(uv: vec2f, bias: f32) -> vec4f {
                return textureSampleBias(image, smp, uv, bias);
            }
            fn constant_helper(uv: vec2f) -> vec4f {
                return textureSampleBias(image, smp, uv, 1.0);
            }
            @fragment fn min_boundary(@location(0) uv: vec2f) -> @location(0) vec4f {
                return textureSampleBias(image, smp, uv, -3.0);
            }
            @fragment fn max_boundary(@location(0) uv: vec2f) -> @location(0) vec4f {
                return textureSampleBias(image, smp, uv, 3.0);
            }
            @fragment fn fractional(@location(0) uv: vec2f) -> @location(0) vec4f {
                return textureSampleBias(image, smp, uv, 0.5);
            }
            @fragment fn too_large(@location(0) uv: vec2f) -> @location(0) vec4f {
                return textureSampleBias(image, smp, uv, 3.01);
            }
            @fragment fn dynamic(@location(0) uv: vec2f) -> @location(0) vec4f {
                return textureSampleBias(image, smp, uv, uv.x);
            }
            @fragment fn offset(@location(0) uv: vec2f) -> @location(0) vec4f {
                return textureSampleBias(image, smp, uv, 1.0, vec2i(0, 0));
            }
            @fragment fn helper_dynamic(@location(0) uv: vec2f) -> @location(0) vec4f {
                return dynamic_helper(uv, uv.x);
            }
            @fragment fn helper_forwarded_constant(@location(0) uv: vec2f) -> @location(0) vec4f {
                return dynamic_helper(uv, 1.0);
            }
            @fragment fn helper_constant(@location(0) uv: vec2f) -> @location(0) vec4f {
                return constant_helper(uv);
            }
            @fragment fn safe() -> @location(0) vec4f { return vec4f(1.0); }
            "#,
        )
        .unwrap();
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .unwrap();
        let interface = Interface::new(&module, &info, wgt::Limits::defaults());
        let fl91 = wgt::DownlevelLimits {
            max_texture_sample_bias: 3,
            texture_sample_bias_dynamic: false,
            texture_sample_bias_offset: false,
            ..Default::default()
        };

        for entry in [
            "min_boundary",
            "max_boundary",
            "fractional",
            "helper_constant",
            "safe",
        ] {
            assert!(
                !interface
                    .entry_point_uses_unsupported_texture_sample_bias(
                        naga::ShaderStage::Fragment,
                        entry,
                        &fl91,
                    )
                    .unwrap(),
                "{entry} should be allowed"
            );
        }
        for entry in [
            "too_large",
            "dynamic",
            "offset",
            "helper_dynamic",
            "helper_forwarded_constant",
        ] {
            assert!(
                interface
                    .entry_point_uses_unsupported_texture_sample_bias(
                        naga::ShaderStage::Fragment,
                        entry,
                        &fl91,
                    )
                    .unwrap(),
                "{entry} should be rejected"
            );
        }

        for entry in [
            "too_large",
            "dynamic",
            "offset",
            "helper_dynamic",
            "helper_forwarded_constant",
        ] {
            assert!(
                !interface
                    .entry_point_uses_unsupported_texture_sample_bias(
                        naga::ShaderStage::Fragment,
                        entry,
                        &wgt::DownlevelLimits::default(),
                    )
                    .unwrap(),
                "unrestricted capabilities must allow {entry}"
            );
        }
        assert!(interface
            .entry_point_uses_unsupported_texture_sample_bias(
                naga::ShaderStage::Fragment,
                "missing",
                &fl91,
            )
            .is_err());
    }

    #[test]
    fn fragment_derivative_limits_follow_selected_entry_and_helper_calls() {
        let module = naga::front::wgsl::parse_str(
            r#"
            @group(0) @binding(0) var image: texture_2d<f32>;
            @group(0) @binding(1) var smp: sampler;
            fn derivative_helper(uv: vec2f) -> vec2f { return dpdy(uv); }
            fn gradient_helper(uv: vec2f) -> vec4f {
                return textureSampleGrad(image, smp, uv, vec2f(0.01, 0.0), vec2f(0.0, 0.01));
            }
            @fragment fn direct_derivative(@location(0) uv: vec2f) -> @location(0) vec4f {
                return vec4f(dpdx(uv), 0.0, 1.0);
            }
            @fragment fn helper_derivative(@location(0) uv: vec2f) -> @location(0) vec4f {
                return vec4f(derivative_helper(uv), 0.0, 1.0);
            }
            @fragment fn explicit_gradient(@location(0) uv: vec2f) -> @location(0) vec4f {
                return textureSampleGrad(image, smp, uv, vec2f(0.01, 0.0), vec2f(0.0, 0.01));
            }
            @fragment fn helper_gradient(@location(0) uv: vec2f) -> @location(0) vec4f {
                return gradient_helper(uv);
            }
            @fragment fn safe() -> @location(0) vec4f { return vec4f(1.0); }
            @vertex fn vertex_safe() -> @builtin(position) vec4f { return vec4f(0.0, 0.0, 0.0, 1.0); }
            "#,
        )
        .unwrap();
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .unwrap();
        let interface = Interface::new(&module, &info, wgt::Limits::defaults());
        let fl91 = wgt::DownlevelLimits {
            fragment_derivatives: false,
            ..Default::default()
        };

        for entry in [
            "direct_derivative",
            "helper_derivative",
            "explicit_gradient",
            "helper_gradient",
        ] {
            assert!(
                interface
                    .entry_point_uses_unsupported_fragment_derivatives(
                        naga::ShaderStage::Fragment,
                        entry,
                        &fl91,
                    )
                    .unwrap(),
                "{entry} should be rejected"
            );
            assert!(
                !interface
                    .entry_point_uses_unsupported_fragment_derivatives(
                        naga::ShaderStage::Fragment,
                        entry,
                        &wgt::DownlevelLimits::default(),
                    )
                    .unwrap(),
                "unrestricted capabilities must allow {entry}"
            );
        }
        assert!(!interface
            .entry_point_uses_unsupported_fragment_derivatives(
                naga::ShaderStage::Fragment,
                "safe",
                &fl91,
            )
            .unwrap());
        assert!(!interface
            .entry_point_uses_unsupported_fragment_derivatives(
                naga::ShaderStage::Vertex,
                "vertex_safe",
                &fl91,
            )
            .unwrap());
        assert!(interface
            .entry_point_uses_unsupported_fragment_derivatives(
                naga::ShaderStage::Fragment,
                "missing",
                &fl91,
            )
            .is_err());
    }

    #[test]
    fn explicit_texture_sample_level_limits_follow_selected_entry_and_helpers() {
        let module = naga::front::wgsl::parse_str(
            r#"
            @group(0) @binding(0) var image: texture_2d<f32>;
            @group(0) @binding(1) var smp: sampler;
            fn sample_exact(uv: vec2f, level: f32) -> vec4f {
                return textureSampleLevel(image, smp, uv, level);
            }
            @fragment fn direct(@location(0) uv: vec2f) -> @location(0) vec4f {
                return textureSampleLevel(image, smp, uv, 0.0);
            }
            @fragment fn helper(@location(0) uv: vec2f) -> @location(0) vec4f {
                return sample_exact(uv, uv.x);
            }
            @fragment fn safe(@location(0) uv: vec2f) -> @location(0) vec4f {
                return textureSample(image, smp, uv);
            }
            @fragment fn sibling_safe() -> @location(0) vec4f { return vec4f(1.0); }
            @vertex fn vertex_safe() -> @builtin(position) vec4f {
                return vec4f(0.0, 0.0, 0.0, 1.0);
            }
            "#,
        )
        .unwrap();
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .unwrap();
        let interface = Interface::new(&module, &info, wgt::Limits::defaults());
        let fl91 = wgt::DownlevelLimits {
            texture_sample_level: false,
            ..Default::default()
        };

        for entry in ["direct", "helper"] {
            assert!(interface
                .entry_point_uses_unsupported_texture_sample_level(
                    naga::ShaderStage::Fragment,
                    entry,
                    &fl91,
                )
                .unwrap());
            assert!(!interface
                .entry_point_uses_unsupported_texture_sample_level(
                    naga::ShaderStage::Fragment,
                    entry,
                    &wgt::DownlevelLimits::default(),
                )
                .unwrap());
        }

        for (stage, entry) in [
            (naga::ShaderStage::Fragment, "safe"),
            (naga::ShaderStage::Fragment, "sibling_safe"),
            (naga::ShaderStage::Vertex, "vertex_safe"),
        ] {
            assert!(!interface
                .entry_point_uses_unsupported_texture_sample_level(stage, entry, &fl91)
                .unwrap());
        }
        assert!(interface
            .entry_point_uses_unsupported_texture_sample_level(
                naga::ShaderStage::Fragment,
                "missing",
                &fl91,
            )
            .is_err());
    }

    #[test]
    fn texture_gather_limits_follow_selected_entry_and_helpers() {
        let module = naga::front::wgsl::parse_str(
            r#"
            @group(0) @binding(0) var image: texture_2d<f32>;
            @group(0) @binding(1) var smp: sampler;
            fn gather_texture(uv: vec2f) -> vec4f {
                return textureGather(0, image, smp, uv);
            }
            @fragment fn direct(@location(0) uv: vec2f) -> @location(0) vec4f {
                return textureGather(0, image, smp, uv);
            }
            @fragment fn helper(@location(0) uv: vec2f) -> @location(0) vec4f {
                return gather_texture(uv);
            }
            @fragment fn safe(@location(0) uv: vec2f) -> @location(0) vec4f {
                return textureSample(image, smp, uv);
            }
            @fragment fn sibling_safe() -> @location(0) vec4f { return vec4f(1.0); }
            @compute @workgroup_size(1) fn compute_direct() {
                let value = textureGather(0, image, smp, vec2f(0.5));
                if value.x < 0.0 { return; }
            }
            @compute @workgroup_size(1) fn compute_helper() {
                let value = gather_texture(vec2f(0.5));
                if value.x < 0.0 { return; }
            }
            @vertex fn vertex_safe() -> @builtin(position) vec4f {
                return vec4f(0.0, 0.0, 0.0, 1.0);
            }
            "#,
        )
        .unwrap();
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .unwrap();
        let interface = Interface::new(&module, &info, wgt::Limits::defaults());
        let fl91 = wgt::DownlevelLimits {
            texture_gather: false,
            ..Default::default()
        };

        for entry in ["direct", "helper"] {
            assert!(interface
                .entry_point_uses_unsupported_texture_gather(
                    naga::ShaderStage::Fragment,
                    entry,
                    &fl91,
                )
                .unwrap());
            assert!(!interface
                .entry_point_uses_unsupported_texture_gather(
                    naga::ShaderStage::Fragment,
                    entry,
                    &wgt::DownlevelLimits::default(),
                )
                .unwrap());
        }

        for (stage, entry) in [
            (naga::ShaderStage::Fragment, "safe"),
            (naga::ShaderStage::Fragment, "sibling_safe"),
            (naga::ShaderStage::Vertex, "vertex_safe"),
        ] {
            assert!(!interface
                .entry_point_uses_unsupported_texture_gather(stage, entry, &fl91)
                .unwrap());
        }
        for entry in ["compute_direct", "compute_helper"] {
            assert!(interface
                .entry_point_uses_unsupported_texture_gather(
                    naga::ShaderStage::Compute,
                    entry,
                    &fl91,
                )
                .unwrap());
            assert!(!interface
                .entry_point_uses_unsupported_texture_gather(
                    naga::ShaderStage::Compute,
                    entry,
                    &wgt::DownlevelLimits::default(),
                )
                .unwrap());
        }
        assert!(interface
            .entry_point_uses_unsupported_texture_gather(
                naga::ShaderStage::Fragment,
                "missing",
                &fl91,
            )
            .is_err());
    }

    #[test]
    fn texture_queries_follow_selected_entry_and_helper_calls() {
        let module = naga::front::wgsl::parse_str(
            r#"
            @group(0) @binding(0) var image: texture_2d<f32>;
            fn query_dimensions() -> vec2u { return textureDimensions(image); }
            @fragment fn direct() -> @location(0) vec4f {
                return vec4f(vec2f(textureDimensions(image)), 0.0, 1.0);
            }
            @fragment fn helper() -> @location(0) vec4f {
                return vec4f(vec2f(query_dimensions()), 0.0, 1.0);
            }
            @fragment fn levels() -> @location(0) vec4f {
                return vec4f(f32(textureNumLevels(image)), 0.0, 0.0, 1.0);
            }
            @fragment fn safe() -> @location(0) vec4f { return vec4f(1.0); }
            @vertex fn vertex_query() -> @builtin(position) vec4f {
                return vec4f(vec2f(textureDimensions(image)), 0.0, 1.0);
            }
            "#,
        )
        .unwrap();
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .unwrap();
        let interface = Interface::new(&module, &info, wgt::Limits::defaults());
        let fl91 = wgt::DownlevelLimits {
            texture_query: false,
            ..Default::default()
        };

        for entry in ["direct", "helper", "levels"] {
            assert!(interface
                .entry_point_uses_unsupported_texture_query(
                    naga::ShaderStage::Fragment,
                    entry,
                    &fl91,
                )
                .unwrap());
            assert!(!interface
                .entry_point_uses_unsupported_texture_query(
                    naga::ShaderStage::Fragment,
                    entry,
                    &wgt::DownlevelLimits::default(),
                )
                .unwrap());
        }
        assert!(!interface
            .entry_point_uses_unsupported_texture_query(naga::ShaderStage::Fragment, "safe", &fl91,)
            .unwrap());
        assert!(interface
            .entry_point_uses_unsupported_texture_query(
                naga::ShaderStage::Vertex,
                "vertex_query",
                &fl91,
            )
            .unwrap());
        assert!(interface
            .entry_point_uses_unsupported_texture_query(
                naga::ShaderStage::Fragment,
                "missing",
                &fl91,
            )
            .is_err());
    }

    #[test]
    fn base_clamp_to_edge_sampling_follows_selected_entry_and_helper_calls() {
        let module = naga::front::wgsl::parse_str(
            r#"
            @group(0) @binding(0) var image: texture_2d<f32>;
            @group(0) @binding(1) var smp: sampler;
            fn sample_base_clamp(uv: vec2f) -> vec4f {
                return textureSampleBaseClampToEdge(image, smp, uv);
            }
            @fragment fn direct(@location(0) uv: vec2f) -> @location(0) vec4f {
                return textureSampleBaseClampToEdge(image, smp, uv);
            }
            @fragment fn helper(@location(0) uv: vec2f) -> @location(0) vec4f {
                return sample_base_clamp(uv);
            }
            @fragment fn safe() -> @location(0) vec4f { return vec4f(1.0); }
            "#,
        )
        .unwrap();
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .unwrap();
        let interface = Interface::new(&module, &info, wgt::Limits::defaults());
        let fl91 = wgt::DownlevelLimits {
            texture_sample_base_clamp_to_edge: false,
            ..Default::default()
        };

        for entry in ["direct", "helper"] {
            assert!(interface
                .entry_point_uses_unsupported_texture_sample_base_clamp_to_edge(
                    naga::ShaderStage::Fragment,
                    entry,
                    &fl91,
                )
                .unwrap());
            assert!(!interface
                .entry_point_uses_unsupported_texture_sample_base_clamp_to_edge(
                    naga::ShaderStage::Fragment,
                    entry,
                    &wgt::DownlevelLimits::default(),
                )
                .unwrap());
        }
        assert!(!interface
            .entry_point_uses_unsupported_texture_sample_base_clamp_to_edge(
                naga::ShaderStage::Fragment,
                "safe",
                &fl91,
            )
            .unwrap());
        assert!(interface
            .entry_point_uses_unsupported_texture_sample_base_clamp_to_edge(
                naga::ShaderStage::Fragment,
                "missing",
                &fl91,
            )
            .is_err());
    }

    #[test]
    fn fragment_depth_without_color_target_0_follows_selected_entry() {
        let module = naga::front::wgsl::parse_str(
            r#"
            struct DepthAndColor {
                @location(0) color: vec4f,
                @builtin(frag_depth) depth: f32,
            }
            struct LocationOneAndDepth {
                @location(1) color: vec4f,
                @builtin(frag_depth) depth: f32,
            }
            @fragment fn depth_only() -> @builtin(frag_depth) f32 { return 0.5; }
            @fragment fn depth_and_color() -> DepthAndColor {
                return DepthAndColor(vec4f(1.0), 0.5);
            }
            @fragment fn other_location_and_depth() -> LocationOneAndDepth {
                return LocationOneAndDepth(vec4f(1.0), 0.5);
            }
            @fragment fn color_only() -> @location(0) vec4f { return vec4f(1.0); }
            "#,
        )
        .unwrap();
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .unwrap();
        let interface = Interface::new(&module, &info, wgt::Limits::defaults());
        let fl91 = wgt::DownlevelLimits {
            fragment_depth_without_color_target_0: false,
            ..Default::default()
        };

        assert!(interface
            .entry_point_uses_unsupported_fragment_depth_without_color_target_0(
                naga::ShaderStage::Fragment,
                "depth_only",
                &fl91,
            )
            .unwrap());
        for entry in ["depth_and_color", "color_only"] {
            assert!(!interface
                .entry_point_uses_unsupported_fragment_depth_without_color_target_0(
                    naga::ShaderStage::Fragment,
                    entry,
                    &fl91,
                )
                .unwrap());
        }
        assert!(interface
            .entry_point_uses_unsupported_fragment_depth_without_color_target_0(
                naga::ShaderStage::Fragment,
                "other_location_and_depth",
                &fl91,
            )
            .unwrap());
        assert!(!interface
            .entry_point_uses_unsupported_fragment_depth_without_color_target_0(
                naga::ShaderStage::Fragment,
                "depth_only",
                &wgt::DownlevelLimits::default(),
            )
            .unwrap());
        assert!(interface
            .entry_point_uses_unsupported_fragment_depth_without_color_target_0(
                naga::ShaderStage::Fragment,
                "missing",
                &fl91,
            )
            .is_err());
    }

    #[test]
    fn fragment_sample_mask_output_follows_selected_entry() {
        let module = naga::front::wgsl::parse_str(
            r#"
            struct ColorAndMask {
                @location(0) color: vec4f,
                @builtin(sample_mask) mask: u32,
            }
            @fragment fn with_mask() -> ColorAndMask {
                return ColorAndMask(vec4f(1.0), 0xffffffffu);
            }
            @fragment fn color_only() -> @location(0) vec4f { return vec4f(1.0); }
            "#,
        )
        .unwrap();
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .unwrap();
        let interface = Interface::new(&module, &info, wgt::Limits::defaults());
        let fl91 = wgt::DownlevelLimits {
            fragment_sample_mask_output: false,
            ..Default::default()
        };

        assert!(interface
            .entry_point_uses_unsupported_fragment_sample_mask_output(
                naga::ShaderStage::Fragment,
                "with_mask",
                &fl91,
            )
            .unwrap());
        assert!(!interface
            .entry_point_uses_unsupported_fragment_sample_mask_output(
                naga::ShaderStage::Fragment,
                "color_only",
                &fl91,
            )
            .unwrap());
        assert!(!interface
            .entry_point_uses_unsupported_fragment_sample_mask_output(
                naga::ShaderStage::Fragment,
                "with_mask",
                &wgt::DownlevelLimits::default(),
            )
            .unwrap());
        assert!(interface
            .entry_point_uses_unsupported_fragment_sample_mask_output(
                naga::ShaderStage::Fragment,
                "missing",
                &fl91,
            )
            .is_err());
    }

    #[test]
    fn fragment_non_perspective_interpolation_follows_selected_entry() {
        let module = naga::front::wgsl::parse_str(
            r#"
            struct FlatOutput {
                @builtin(position) position: vec4f,
                @location(0) @interpolate(flat) value: f32,
            }
            @vertex fn flat_vertex() -> FlatOutput {
                return FlatOutput(vec4f(0.0, 0.0, 0.0, 1.0), 1.0);
            }
            @fragment fn flat(@location(0) @interpolate(flat) value: f32) -> @location(0) vec4f {
                return vec4f(value);
            }
            @fragment fn linear(@location(0) @interpolate(linear) value: f32) -> @location(0) vec4f {
                return vec4f(value);
            }
            @fragment fn centroid(@location(0) @interpolate(perspective, centroid) value: f32) -> @location(0) vec4f {
                return vec4f(value);
            }
            @fragment fn perspective(@location(0) @interpolate(perspective) value: f32) -> @location(0) vec4f {
                return vec4f(value);
            }
            @fragment fn no_varying() -> @location(0) vec4f { return vec4f(1.0); }
            "#,
        )
        .unwrap();
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .unwrap();
        let interface = Interface::new(&module, &info, wgt::Limits::defaults());
        let fl91 = wgt::DownlevelLimits {
            fragment_non_perspective_interpolation: false,
            ..Default::default()
        };

        for entry in ["flat", "linear"] {
            assert!(interface
                .entry_point_uses_unsupported_fragment_non_perspective_interpolation(
                    naga::ShaderStage::Fragment,
                    entry,
                    &fl91,
                )
                .unwrap());
            assert!(!interface
                .entry_point_uses_unsupported_fragment_non_perspective_interpolation(
                    naga::ShaderStage::Fragment,
                    entry,
                    &wgt::DownlevelLimits::default(),
                )
                .unwrap());
        }
        for entry in ["centroid", "perspective", "no_varying"] {
            assert!(!interface
                .entry_point_uses_unsupported_fragment_non_perspective_interpolation(
                    naga::ShaderStage::Fragment,
                    entry,
                    &fl91,
                )
                .unwrap());
        }
        assert!(!interface
            .entry_point_uses_unsupported_fragment_non_perspective_interpolation(
                naga::ShaderStage::Vertex,
                "flat_vertex",
                &fl91,
            )
            .unwrap());
        assert!(interface
            .entry_point_uses_unsupported_fragment_non_perspective_interpolation(
                naga::ShaderStage::Fragment,
                "missing",
                &fl91,
            )
            .is_err());
    }

    #[test]
    fn front_facing_detection_uses_the_selected_fragment_entry_point() {
        let module = naga::front::wgsl::parse_str(
            r#"
            struct FaceInput { @builtin(front_facing) front: bool }
            @fragment fn direct(@builtin(front_facing) front: bool) -> @location(0) vec4f {
                return select(vec4f(0.0), vec4f(1.0), front);
            }
            @fragment fn structured(input: FaceInput) -> @location(0) vec4f {
                return select(vec4f(0.0), vec4f(1.0), input.front);
            }
            @fragment fn safe() -> @location(0) vec4f { return vec4f(1.0); }
            "#,
        )
        .unwrap();
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .unwrap();
        let interface = Interface::new(&module, &info, wgt::Limits::defaults());

        for entry in ["direct", "structured"] {
            assert!(interface
                .entry_point_uses_front_facing(naga::ShaderStage::Fragment, entry)
                .unwrap());
        }
        assert!(!interface
            .entry_point_uses_front_facing(naga::ShaderStage::Fragment, "safe")
            .unwrap());
        assert!(interface
            .entry_point_uses_front_facing(naga::ShaderStage::Fragment, "missing")
            .is_err());
    }

    #[test]
    fn vertex_builtin_detection_uses_selected_entry_point_declarations() {
        let module = naga::front::wgsl::parse_str(
            r#"
            struct Input {
                @location(0) position: vec2f,
                @builtin(vertex_index) vertex_id: u32,
                @builtin(instance_index) instance_id: u32,
            }
            @vertex fn vertex_direct(
                @location(0) position: vec2f,
                @builtin(vertex_index) vertex_id: u32,
            ) -> @builtin(position) vec4f {
                return vec4f(position, f32(vertex_id), 1.0);
            }
            @vertex fn instance_direct(
                @location(0) position: vec2f,
                @builtin(instance_index) instance_id: u32,
            ) -> @builtin(position) vec4f {
                return vec4f(position, f32(instance_id), 1.0);
            }
            @vertex fn structured(input: Input) -> @builtin(position) vec4f {
                return vec4f(input.position, f32(input.vertex_id + input.instance_id), 1.0);
            }
            @vertex fn unused_vertex_index(
                @location(0) position: vec2f,
                @builtin(vertex_index) vertex_id: u32,
            ) -> @builtin(position) vec4f {
                return vec4f(position, 0.0, 1.0);
            }
            @vertex fn unused_instance_index(
                @location(0) position: vec2f,
                @builtin(instance_index) instance_id: u32,
            ) -> @builtin(position) vec4f {
                return vec4f(position, 0.0, 1.0);
            }
            @vertex fn safe(@location(0) position: vec2f) -> @builtin(position) vec4f {
                return vec4f(position, 0.0, 1.0);
            }
            "#,
        )
        .unwrap();
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .unwrap();
        let interface = Interface::new(&module, &info, wgt::Limits::defaults());

        for entry in ["vertex_direct", "structured", "unused_vertex_index"] {
            assert!(interface.entry_point_uses_vertex_index(entry).unwrap());
        }
        for entry in ["instance_direct", "structured", "unused_instance_index"] {
            assert!(interface.entry_point_uses_instance_index(entry).unwrap());
        }
        assert!(!interface.entry_point_uses_vertex_index("safe").unwrap());
        assert!(!interface.entry_point_uses_instance_index("safe").unwrap());
        assert!(interface.entry_point_uses_vertex_index("missing").is_err());
        assert!(interface
            .entry_point_uses_instance_index("missing")
            .is_err());
    }

    #[test]
    fn fragment_position_detection_uses_the_selected_fragment_entry_point() {
        let module = naga::front::wgsl::parse_str(
            r#"
            struct PositionInput { @builtin(position) position: vec4f, @location(0) color: vec4f }
            fn read_position(position: vec4f) -> f32 { return position.x; }
            fn read_position_struct(input: PositionInput) -> vec4f { return input.position; }
            fn ignore_position_struct(input: PositionInput) -> vec4f { return input.color; }
            fn forward_position_struct(input: PositionInput) -> vec4f { return read_position_struct(input); }
            fn forward_ignored_struct(input: PositionInput) -> vec4f { return ignore_position_struct(input); }
            fn forward_position(position: vec4f) -> f32 { return read_position(position); }
            @fragment fn direct(@builtin(position) position: vec4f) -> @location(0) vec4f {
                return position;
            }
            @fragment fn helper(@builtin(position) position: vec4f) -> @location(0) vec4f {
                return vec4f(read_position(position), 0.0, 0.0, 1.0);
            }
            @fragment fn helper_struct(input: PositionInput) -> @location(0) vec4f {
                return read_position_struct(input);
            }
            @fragment fn helper_struct_forwarded(input: PositionInput) -> @location(0) vec4f {
                return forward_position_struct(input);
            }
            @fragment fn helper_struct_forwarded_unused(input: PositionInput) -> @location(0) vec4f {
                return forward_ignored_struct(input);
            }
            @fragment fn helper_forwarded(@builtin(position) position: vec4f) -> @location(0) vec4f {
                return vec4f(forward_position(position), 0.0, 0.0, 1.0);
            }
            @fragment fn helper_unused(input: PositionInput) -> @location(0) vec4f {
                return ignore_position_struct(input);
            }
            @fragment fn structured(input: PositionInput) -> @location(0) vec4f {
                return input.position;
            }
            @fragment fn unused_direct(@builtin(position) position: vec4f) -> @location(0) vec4f {
                return vec4f(1.0);
            }
            @fragment fn unused_struct(input: PositionInput) -> @location(0) vec4f {
                return input.color;
            }
            @fragment fn safe() -> @location(0) vec4f { return vec4f(1.0); }
            "#,
        )
        .unwrap();
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .unwrap();
        let interface = Interface::new(&module, &info, wgt::Limits::defaults());

        for entry in [
            "direct",
            "helper",
            "helper_struct",
            "helper_struct_forwarded",
            "helper_forwarded",
            "structured",
        ] {
            assert!(interface
                .entry_point_uses_fragment_position(naga::ShaderStage::Fragment, entry)
                .unwrap());
        }
        for entry in [
            "unused_direct",
            "unused_struct",
            "helper_unused",
            "helper_struct_forwarded_unused",
            "safe",
        ] {
            assert!(!interface
                .entry_point_uses_fragment_position(naga::ShaderStage::Fragment, entry)
                .unwrap());
        }
        assert!(interface
            .entry_point_uses_fragment_position(naga::ShaderStage::Fragment, "missing")
            .is_err());
    }

    #[test]
    fn fl91_integer_semantics_gate_follows_selected_entry_and_helper_calls() {
        let module = naga::front::wgsl::parse_str(
            r#"
            struct Values { value: u32 }
            @group(0) @binding(0) var<uniform> values: Values;
            override tint: f32 = 0.25;
            fn increment(value: u32) -> u32 { return value + 1u; }
            @vertex fn bad_helper() -> @builtin(position) vec4f {
                return vec4f(f32(increment(values.value)), 0.0, 0.0, 1.0);
            }
            @vertex fn safe_sibling() -> @builtin(position) vec4f {
                return vec4f(0.0, 0.0, 0.0, 1.0);
            }
            @vertex fn safe_vertex_conversion(@location(0) value: vec2<u32>) -> @builtin(position) vec4f {
                return vec4f(vec2f(value), 0.0, 1.0);
            }
            @fragment fn float_override() -> @location(0) vec4f {
                return vec4f(tint, 0.0, 0.0, 1.0);
            }
            "#,
        )
        .unwrap();
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .unwrap();
        let interface = Interface::new(&module, &info, wgt::Limits::defaults());
        let fl91 = wgt::DownlevelLimits {
            runtime_integer_shader_operations: false,
            ..Default::default()
        };

        assert!(interface
            .entry_point_uses_unsupported_runtime_integer_semantics(
                naga::ShaderStage::Vertex,
                "bad_helper",
                &fl91,
            )
            .unwrap());
        for entry in ["safe_sibling", "safe_vertex_conversion"] {
            assert!(!interface
                .entry_point_uses_unsupported_runtime_integer_semantics(
                    naga::ShaderStage::Vertex,
                    entry,
                    &fl91,
                )
                .unwrap());
        }
        assert!(!interface
            .entry_point_uses_unsupported_runtime_integer_semantics(
                naga::ShaderStage::Fragment,
                "float_override",
                &fl91,
            )
            .unwrap());
        assert!(!interface
            .entry_point_uses_unsupported_runtime_integer_semantics(
                naga::ShaderStage::Vertex,
                "bad_helper",
                &wgt::DownlevelLimits::default(),
            )
            .unwrap());
        assert!(interface
            .entry_point_uses_unsupported_runtime_integer_semantics(
                naga::ShaderStage::Vertex,
                "missing",
                &fl91,
            )
            .is_err());
    }

    #[test]
    fn fl91_matrix_aggregate_store_gate_follows_pointer_helpers_and_entry_points() {
        let module = naga::front::wgsl::parse_str(
            r#"
            struct Aggregate { transform: mat3x2<f32>, value: f32 }
            fn write_scalar(value: ptr<function, Aggregate>) { (*value).value = 1.0; }
            @vertex fn bad_direct() -> @builtin(position) vec4f {
                var value: Aggregate;
                value.value = 1.0;
                return vec4f(0.0, 0.0, 0.0, 1.0);
            }
            @vertex fn bad_helper() -> @builtin(position) vec4f {
                var value: Aggregate;
                write_scalar(&value);
                return vec4f(0.0, 0.0, 0.0, 1.0);
            }
            @vertex fn safe_read() -> @builtin(position) vec4f {
                let value = Aggregate(mat3x2<f32>(), 1.0);
                return vec4f(value.transform[0], 0.0, 1.0);
            }
            @vertex fn safe_sibling() -> @builtin(position) vec4f {
                return vec4f(0.0, 0.0, 0.0, 1.0);
            }
            "#,
        )
        .unwrap();
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .unwrap();
        let interface = Interface::new(&module, &info, wgt::Limits::defaults());
        let fl91 = wgt::DownlevelLimits {
            matrix_aggregate_stores: false,
            ..Default::default()
        };

        for entry in ["bad_direct", "bad_helper"] {
            assert!(interface
                .entry_point_uses_unsupported_matrix_aggregate_store(
                    naga::ShaderStage::Vertex,
                    entry,
                    &fl91,
                )
                .unwrap());
        }
        for entry in ["safe_read", "safe_sibling"] {
            assert!(!interface
                .entry_point_uses_unsupported_matrix_aggregate_store(
                    naga::ShaderStage::Vertex,
                    entry,
                    &fl91,
                )
                .unwrap());
        }
        assert!(!interface
            .entry_point_uses_unsupported_matrix_aggregate_store(
                naga::ShaderStage::Vertex,
                "bad_direct",
                &wgt::DownlevelLimits::default(),
            )
            .unwrap());
        assert!(interface
            .entry_point_uses_unsupported_matrix_aggregate_store(
                naga::ShaderStage::Vertex,
                "missing",
                &fl91,
            )
            .is_err());
    }
}
