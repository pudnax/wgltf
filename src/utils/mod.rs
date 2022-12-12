use std::num::NonZeroU64;

use gltf::accessor::{
    DataType::{self, F32, I16, I8, U16, U32, U8},
    Dimensions::{Mat2, Mat3, Mat4, Scalar, Vec2, Vec3, Vec4},
};
use wgpu::{PrimitiveTopology, VertexFormat::*};

mod frame_counter;
mod input;
pub use frame_counter::FrameCounter;
pub use input::Input;

pub trait NonZeroSized: Sized {
    const SIZE: NonZeroU64 = unsafe { NonZeroU64::new_unchecked(std::mem::size_of::<Self>() as _) };
}
impl<T> NonZeroSized for T where T: Sized {}

pub fn component_type_to_index_format(ty: gltf::accessor::DataType) -> wgpu::IndexFormat {
    match ty {
        DataType::U16 => wgpu::IndexFormat::Uint16,
        DataType::U32 => wgpu::IndexFormat::Uint32,
        _ => panic!("Unsupported index format!"),
    }
}

pub fn size_of_component_type(ty: gltf::accessor::DataType) -> usize {
    match ty {
        DataType::I8 => std::mem::size_of::<u8>(),
        DataType::U8 => std::mem::size_of::<i8>(),
        DataType::I16 => std::mem::size_of::<i16>(),
        DataType::U16 => std::mem::size_of::<u16>(),
        DataType::U32 => std::mem::size_of::<u32>(),
        DataType::F32 => std::mem::size_of::<f32>(),
    }
}

pub fn align_of_component_type(dims: gltf::accessor::Dimensions) -> usize {
    match dims {
        Scalar => 1,
        Vec2 => 2,
        Vec3 => 3,
        Vec4 => 4,
        Mat2 => 4,
        Mat3 => 16,
        Mat4 => 16,
    }
}

pub fn stride_of_component_type(accessor: &gltf::accessor::Accessor) -> usize {
    size_of_component_type(accessor.data_type()) * align_of_component_type(accessor.dimensions())
}

pub fn accessor_type_to_format(accessor: &gltf::accessor::Accessor) -> wgpu::VertexFormat {
    let normalized = accessor.normalized();
    let dims = accessor.dimensions();
    let ty = accessor.data_type();
    match (normalized, dims, ty) {
        (true, Vec2, I8) => Snorm8x2,
        (true, Vec2, U8) => Unorm8x2,
        (true, Vec4, I8) => Snorm8x4,
        (true, Vec4, U8) => Unorm8x4,
        (false, Vec2, I8) => Sint8x2,
        (false, Vec2, U8) => Uint8x2,
        (false, Vec4, I8) => Sint8x4,
        (false, Vec4, U8) => Sint8x4,
        (true, Vec2, I16) => Snorm16x2,
        (true, Vec2, U16) => Unorm16x2,
        (true, Vec4, I16) => Snorm16x4,
        (true, Vec4, U16) => Unorm16x4,
        (false, Vec2, I16) => Sint16x2,
        (false, Vec2, U16) => Uint16x2,
        (false, Vec4, I16) => Sint16x4,
        (false, Vec4, U16) => Uint16x4,
        (_, Scalar, F32) => Float32,
        (_, Vec2, F32) => Float32x2,
        (_, Vec3, F32) => Float32x3,
        (_, Vec4, F32) => Float32x4,
        (_, Scalar, U32) => Uint32,
        (_, Vec2, U32) => Uint32x2,
        (_, Vec3, U32) => Uint32x3,
        (_, Vec4, U32) => Uint32x4,
        _ => panic!("Unsupported vertex format!"),
    }
}

pub fn mesh_mode_to_topology(mode: gltf::mesh::Mode) -> wgpu::PrimitiveTopology {
    use gltf::mesh::Mode;
    match mode {
        Mode::Triangles => PrimitiveTopology::TriangleList,
        Mode::TriangleStrip | Mode::TriangleFan => PrimitiveTopology::TriangleStrip,
        Mode::Lines => PrimitiveTopology::LineList,
        Mode::LineStrip => PrimitiveTopology::LineStrip,
        Mode::Points => PrimitiveTopology::PointList,
        Mode::LineLoop => todo!("Line Loop!"),
    }
}
