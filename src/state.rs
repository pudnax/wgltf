use std::collections::HashMap;

use color_eyre::{
    eyre::{eyre, ContextCompat},
    Result,
};
use pollster::FutureExt;
use wgpu::util::DeviceExt;
use winit::dpi::PhysicalSize;

use crate::{
    camera::{self, CameraBinding},
    utils::{
        accessor_type_to_format, component_type_to_index_format, mesh_mode_to_topology,
        stride_of_component_type, NonZeroSized,
    },
};
mod global_ubo;

use global_ubo::GlobalUniformBinding;
pub use global_ubo::Uniform;

pub struct ShaderLocation(u32);

impl ShaderLocation {
    fn new(s: gltf::Semantic) -> Option<Self> {
        Some(match s {
            gltf::Semantic::Positions => Self(0),
            gltf::Semantic::Normals => Self(1),
            _ => return None,
        })
    }
}

impl TryFrom<gltf::Semantic> for ShaderLocation {
    type Error = color_eyre::Report;
    fn try_from(v: gltf::Semantic) -> Result<Self, Self::Error> {
        Ok(match v {
            gltf::Semantic::Positions => Self(0),
            gltf::Semantic::Normals => Self(1),
            _ => return Err(eyre!("Unsupported primitive semantic")),
        })
    }
}

#[derive(Debug)]
pub enum DrawMode {
    Normal(u32),
    Indexed {
        buffer: wgpu::Buffer,
        offset: u64,
        ty: wgpu::IndexFormat,
        draw_count: u32,
    },
}

#[derive(Debug)]
pub struct GpuPrimitive {
    pub pipeline: wgpu::RenderPipeline,
    pub buffers: Vec<wgpu::Buffer>,
    pub draw_mode: DrawMode,
}

struct GltfScene {
    document: gltf::Document,
    buffers: Vec<gltf::buffer::Data>,
    images: Vec<gltf::image::Data>,
}

impl GltfScene {
    fn import<P: AsRef<std::path::Path>>(path: P) -> Result<Self> {
        let (document, buffers, images) = gltf::import(path)?;
        Ok(Self {
            document,
            buffers,
            images,
        })
    }

    fn data_of_accessor<'a>(&'a self, accessor: &gltf::Accessor<'a>) -> Result<&'a [u8]> {
        let buffer_view = accessor.view().context("Accessor has no buffer view")?;
        let buffer = buffer_view.buffer();
        let buffer_data = &self.buffers[buffer.index()];
        let buffer_view_data =
            &buffer_data[buffer_view.offset()..buffer_view.offset() + buffer_view.length()];
        let accessor_data = &buffer_view_data
            [accessor.offset()..accessor.offset() + accessor.count() * accessor.size()];
        Ok(accessor_data)
    }
}

pub struct State {
    adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub surface: wgpu::Surface,
    pub surface_config: wgpu::SurfaceConfiguration,
    pub surface_format: wgpu::TextureFormat,

    pub queue: wgpu::Queue,

    pub limits: wgpu::Limits,
    pub features: wgpu::Features,

    pub pipeline: wgpu::RenderPipeline,

    pub camera: camera::Camera,
    pub camera_binding: camera::CameraBinding,

    pub global_uniform: Uniform,
    pub global_uniform_binding: GlobalUniformBinding,

    depth_texture: wgpu::TextureView,
    depth_format: wgpu::TextureFormat,

    scene: GltfScene,
    node_data: HashMap<usize, wgpu::BindGroup>,
    primitive_data: HashMap<(usize, usize), GpuPrimitive>,
}

impl State {
    pub fn get_info(&self) -> RendererInfo {
        let info = self.adapter.get_info();
        RendererInfo {
            device_name: info.name,
            device_type: self.get_device_type().to_string(),
            vendor_name: self.get_vendor_name().to_string(),
            backend: self.get_backend().to_string(),
            screen_format: self.surface_config.format,
        }
    }

    fn get_vendor_name(&self) -> &str {
        match self.adapter.get_info().vendor {
            0x1002 => "AMD",
            0x1010 => "ImgTec",
            0x10DE => "NVIDIA Corporation",
            0x13B5 => "ARM",
            0x5143 => "Qualcomm",
            0x8086 => "INTEL Corporation",
            _ => "Unknown vendor",
        }
    }

    fn get_backend(&self) -> &str {
        match self.adapter.get_info().backend {
            wgpu::Backend::Empty => "Empty",
            wgpu::Backend::Vulkan => "Vulkan",
            wgpu::Backend::Metal => "Metal",
            wgpu::Backend::Dx12 => "Dx12",
            wgpu::Backend::Dx11 => "Dx11",
            wgpu::Backend::Gl => "GL",
            wgpu::Backend::BrowserWebGpu => "Browser WGPU",
        }
    }
    fn get_device_type(&self) -> &str {
        match self.adapter.get_info().device_type {
            wgpu::DeviceType::Other => "Other",
            wgpu::DeviceType::IntegratedGpu => "Integrated GPU",
            wgpu::DeviceType::DiscreteGpu => "Discrete GPU",
            wgpu::DeviceType::VirtualGpu => "Virtual GPU",
            wgpu::DeviceType::Cpu => "CPU",
        }
    }

    pub fn new(window: &winit::window::Window) -> Result<Self> {
        let instance = wgpu::Instance::new(wgpu::Backends::PRIMARY);
        let surface = unsafe { instance.create_surface(&window) };
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: false,
                compatible_surface: Some(&surface),
            })
            .block_on()
            .context("Failed to request adapter")?;

        let limits = adapter.limits();
        let features = adapter.features();
        let surface_format = surface
            .get_supported_formats(&adapter)
            .into_iter()
            .find(|&f| f == wgpu::TextureFormat::Bgra8Unorm)
            .expect("Surface does't support BGRA8Unorm :raised_eyebrow:");

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("Device"),
                    features,
                    limits: limits.clone(),
                },
                None,
            )
            .block_on()?;

        let PhysicalSize { width, height } = window.inner_size();
        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width,
            height,
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
        };
        surface.configure(&device, &surface_config);

        let depth_format = wgpu::TextureFormat::Depth24Plus;
        let depth_texture = create_depth_framebuffer(&device, &surface_config, depth_format);

        let camera = camera::Camera::new(
            5.17,
            -0.34,
            2.6,
            (0., 5., 0.).into(),
            width as f32 / height as f32,
        );
        let camera_binding = camera::CameraBinding::new(&device);

        let scene =
            GltfScene::import("glTF-Sample-Models/2.0/AntiqueCamera/glTF/AntiqueCamera.gltf")?;

        let node_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Node Bind Group Layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: Some(glam::Mat4::SIZE),
                    },
                    count: None,
                }],
            });

        let mut node_data = HashMap::new();
        for node in scene.document.nodes().filter(|n| n.mesh().is_some()) {
            let node_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(&format!("Node Buffer: {:?}", node.name())),
                contents: bytemuck::bytes_of(&node.transform().matrix()),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });
            let node_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(&format!("Node Bind Group: {:?}", node.name())),
                layout: &node_bind_group_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: node_buffer.as_entire_binding(),
                }],
            });

            node_data.insert(node.index(), node_bind_group);
        }

        let global_bind_group_layout = device.create_bind_group_layout(&Uniform::DESC);
        let camera_bind_group_layout = device.create_bind_group_layout(&CameraBinding::DESC);

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Pipeline Layout"),
            bind_group_layouts: &[
                &global_bind_group_layout,
                &camera_bind_group_layout,
                &node_bind_group_layout,
            ],
            push_constant_ranges: &[],
        });

        let mut primitive_data = HashMap::new();
        for mesh in scene.document.meshes() {
            for primitive in mesh.primitives() {
                struct VertexLayout {
                    array_stride: u64,
                    step_mode: wgpu::VertexStepMode,
                }
                let mut vertex_buffer_layouts = vec![];
                let mut vertex_attributes = vec![];
                let mut primitive_buffers = vec![];
                let mut draw_count = 0;
                for (semantic, accessor) in primitive.attributes() {
                    let Some(buffer_view) = accessor.view() else { continue };

                    let Some(shader_location) = ShaderLocation::new(semantic) else { continue; };

                    let array_stride = buffer_view
                        .stride()
                        .unwrap_or(stride_of_component_type(&accessor));
                    vertex_buffer_layouts.push(VertexLayout {
                        array_stride: array_stride as _,
                        step_mode: wgpu::VertexStepMode::Vertex,
                    });
                    vertex_attributes.push([wgpu::VertexAttribute {
                        format: accessor_type_to_format(&accessor),
                        offset: accessor.offset() as _,
                        shader_location: shader_location.0,
                    }]);

                    let buffer = scene.data_of_accessor(&accessor)?;
                    primitive_buffers.push(device.create_buffer_init(
                        &wgpu::util::BufferInitDescriptor {
                            label: Some(&format!("Vertex Buffer {:?}", mesh.name())),
                            contents: buffer,
                            usage: wgpu::BufferUsages::VERTEX,
                        },
                    ));

                    draw_count = accessor.count();
                }

                let vertex_buffers: Vec<_> = vertex_buffer_layouts
                    .into_iter()
                    .enumerate()
                    .map(|(i, buff)| wgpu::VertexBufferLayout {
                        array_stride: buff.array_stride,
                        step_mode: buff.step_mode,
                        attributes: &vertex_attributes[i],
                    })
                    .collect();

                let shader_module =
                    device.create_shader_module(wgpu::include_wgsl!("../shaders/draw_mesh.wgsl"));
                let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("Render Pipeline {i}"),
                    layout: Some(&pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: &shader_module,
                        entry_point: "vs_main",
                        buffers: &vertex_buffers,
                    },
                    primitive: wgpu::PrimitiveState {
                        topology: mesh_mode_to_topology(primitive.mode()),
                        cull_mode: Some(wgpu::Face::Back),
                        ..Default::default()
                    },
                    fragment: Some(wgpu::FragmentState {
                        module: &shader_module,
                        entry_point: "fs_main",
                        targets: &[Some(surface_format.into())],
                    }),
                    depth_stencil: Some(wgpu::DepthStencilState {
                        format: depth_format,
                        depth_write_enabled: true,
                        depth_compare: wgpu::CompareFunction::Less,
                        stencil: wgpu::StencilState::default(),
                        bias: wgpu::DepthBiasState::default(),
                    }),
                    multisample: wgpu::MultisampleState::default(),
                    multiview: None,
                });

                let draw_mode = match primitive.indices() {
                    None => DrawMode::Normal(draw_count as _),
                    Some(idx) => {
                        let buffer = scene.data_of_accessor(&idx)?;
                        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some(&format!("Index Buffer")),
                            contents: buffer,
                            usage: wgpu::BufferUsages::INDEX,
                        });
                        DrawMode::Indexed {
                            buffer,
                            offset: idx.offset() as _,
                            ty: component_type_to_index_format(idx.data_type()),
                            draw_count: idx.count() as _,
                        }
                    }
                };

                // Create primitive
                let gpu_primitive = GpuPrimitive {
                    pipeline,
                    buffers: primitive_buffers,
                    draw_mode,
                };

                // Push primitive
                primitive_data.insert((mesh.index(), primitive.index()), gpu_primitive);
            }
        }

        let shader_module =
            device.create_shader_module(wgpu::include_wgsl!("../shaders/shader.wgsl"));

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Render Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader_module,
                entry_point: "vs_main",
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader_module,
                entry_point: "fs_main",
                targets: &[Some(surface_format.into())],
            }),
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: Default::default(),
            multiview: None,
        });

        Ok(Self {
            adapter,
            surface,
            surface_config,
            surface_format,
            queue,
            limits,
            features,
            pipeline,
            camera,
            camera_binding,

            global_uniform: Uniform::default(),
            global_uniform_binding: GlobalUniformBinding::new(&device),

            depth_texture,
            depth_format,

            device,

            scene,
            node_data,
            primitive_data,
        })
    }

    pub fn update(&mut self, accumulated_time: f64, frame_number: u32) {
        self.global_uniform.frame = frame_number;
        self.global_uniform.time = accumulated_time as _;
        self.global_uniform.resolution = [
            self.surface_config.width as _,
            self.surface_config.height as _,
        ];

        self.global_uniform_binding
            .update(&self.queue, &self.global_uniform);

        self.camera_binding.update(&self.queue, &mut self.camera);
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.surface_config.width = width;
        self.surface_config.height = height;
        self.surface.configure(&self.device, &self.surface_config);
        self.depth_texture =
            create_depth_framebuffer(&self.device, &self.surface_config, self.depth_format);

        self.camera.set_aspect(width, height);
    }

    pub fn render(&self) -> Result<(), wgpu::SurfaceError> {
        let output = self.surface.get_current_texture()?;
        let output_view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &output_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.13,
                        g: 0.13,
                        b: 0.13,
                        a: 1.,
                    }),
                    store: true,
                },
            })],
            depth_stencil_attachment: None,
        });
        rpass.set_pipeline(&self.pipeline);
        rpass.set_bind_group(0, &self.global_uniform_binding.binding, &[]);
        rpass.set_bind_group(1, &self.camera_binding.binding, &[]);
        rpass.draw(0..3, 0..1);
        drop(rpass);

        self.queue.submit(Some(encoder.finish()));
        output.present();

        Ok(())
    }

    pub fn render_mesh(&self) -> Result<(), wgpu::SurfaceError> {
        let output = self.surface.get_current_texture()?;
        let output_view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Main Encoder"),
            });

        let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Render Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &output_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.13,
                        g: 0.13,
                        b: 0.13,
                        a: 1.,
                    }),
                    store: true,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &self.depth_texture,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: true,
                }),
                stencil_ops: None,
            }),
        });
        rpass.set_bind_group(0, &self.global_uniform_binding.binding, &[]);
        rpass.set_bind_group(1, &self.camera_binding.binding, &[]);
        for (&node, gpu_node) in &self.node_data {
            rpass.set_bind_group(2, &gpu_node, &[]);

            let node = self.scene.document.nodes().nth(node).unwrap();
            let mesh = node.mesh().unwrap();
            for primitive in mesh.primitives() {
                let gpu_primitive = &self.primitive_data[&(mesh.index(), primitive.index())];

                rpass.set_pipeline(&gpu_primitive.pipeline);
                for (i, buffer) in gpu_primitive.buffers.iter().enumerate() {
                    rpass.set_vertex_buffer(i as _, buffer.slice(..));
                }

                match &gpu_primitive.draw_mode {
                    DrawMode::Normal(draw_count) => rpass.draw(0..*draw_count, 0..1),
                    DrawMode::Indexed {
                        buffer,
                        offset,
                        ty,
                        draw_count,
                    } => {
                        rpass.set_index_buffer(buffer.slice(*offset..), *ty);
                        rpass.draw_indexed(0..*draw_count, 0, 0..1)
                    }
                }
            }
        }

        drop(rpass);

        self.queue.submit(Some(encoder.finish()));
        output.present();

        Ok(())
    }
}

#[derive(Debug)]
pub struct RendererInfo {
    pub device_name: String,
    pub device_type: String,
    pub vendor_name: String,
    pub backend: String,
    pub screen_format: wgpu::TextureFormat,
}

impl std::fmt::Display for RendererInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Vendor name: {}", self.vendor_name)?;
        writeln!(f, "Device name: {}", self.device_name)?;
        writeln!(f, "Device type: {}", self.device_type)?;
        writeln!(f, "Backend: {}", self.backend)?;
        write!(f, "Screen format: {:?}", self.screen_format)?;
        Ok(())
    }
}

fn create_depth_framebuffer(
    device: &wgpu::Device,
    config: &wgpu::SurfaceConfiguration,
    format: wgpu::TextureFormat,
) -> wgpu::TextureView {
    let size = wgpu::Extent3d {
        width: config.width,
        height: config.height,
        depth_or_array_layers: 1,
    };
    let desc = &wgpu::TextureDescriptor {
        label: Some("Depth Texture"),
        format,
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
    };

    device.create_texture(desc).create_view(&Default::default())
}
