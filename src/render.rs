use std::ops::{Deref, DerefMut};

use futures::executor::block_on;
use wgpu::util::DeviceExt;
use winit::{
    dpi::PhysicalSize,
    event::*,
    event_loop::{ControlFlow, EventLoop},
    window::{Window, WindowBuilder},
};

// -----------------------------------------------------------------------------
//     - Vertex-
// -----------------------------------------------------------------------------
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Vertex {
    position: [f32; 3],
    tex_coords: [f32; 2],
}

impl Vertex {
    fn desc<'a>() -> wgpu::VertexBufferLayout<'a> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::InputStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float3,
                    offset: 0,
                    shader_location: 0,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float2,
                    offset: std::mem::size_of::<[f32; 3]>() as wgpu::BufferAddress,
                    shader_location: 1,
                },
            ],
        }
    }
}

unsafe impl bytemuck::Pod for Vertex {}
unsafe impl bytemuck::Zeroable for Vertex {}

// -----------------------------------------------------------------------------
//     - Square -
//     Drawing area
// -----------------------------------------------------------------------------
const VERTICES: &[Vertex] = &[
    // Top left 0
    Vertex {
        position: [-1.0, 1.0, 0.0],
        tex_coords: [0.0, 0.0],
    },
    // Top right 1
    Vertex {
        position: [1.0, 1.0, 0.0],
        tex_coords: [1.0, 0.0],
    },
    // Bottom left 2
    Vertex {
        position: [-1.0, -1.0, 0.0],
        tex_coords: [0.0, 1.0],
    },
    // Bottom right 3
    Vertex {
        position: [1.0, -1.0, 0.0],
        tex_coords: [1.0, 1.0],
    },
];

const INDICES: &[u16] = &[0, 2, 3, 0, 3, 1];

// -----------------------------------------------------------------------------
//     - Layer -
// -----------------------------------------------------------------------------
struct Layer {
    texture: wgpu::Texture,
    width: u32,
    height: u32,
    texture_size: wgpu::Extent3d,
}

// -----------------------------------------------------------------------------
//     - Pixel -
// -----------------------------------------------------------------------------
#[derive(Debug, Copy, Clone)]
#[repr(C)]
pub struct Pixel {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Pixel {
    fn black() -> Self {
        Self {
            r: 0,
            g: 0,
            b: 0,
            a: 255,
        }
    }
}

unsafe impl bytemuck::Pod for Pixel {}
unsafe impl bytemuck::Zeroable for Pixel {}

// -----------------------------------------------------------------------------
//     - Pixel buffer -
// -----------------------------------------------------------------------------
pub struct PixelBuffer {
    inner: Vec<Pixel>,
}

impl PixelBuffer {
    pub fn with_capacity(cap: usize) -> Self {
        Self {
            inner: (0..cap).map(|_| Pixel::black()).collect(),
        }
    }

    pub fn flap(&mut self, index: usize) -> &mut Pixel {
        &mut self.inner[index]
    }
}

impl Deref for PixelBuffer {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        bytemuck::cast_slice(&self.inner)
    }
}

impl DerefMut for PixelBuffer {
    fn deref_mut(&mut self) -> &mut Self::Target {
        bytemuck::cast_slice_mut(&mut self.inner)
    }
}

// -----------------------------------------------------------------------------
//     - Renderer -
// -----------------------------------------------------------------------------
pub struct Renderer {
    pixels: PixelBuffer,
    state: State,
}

impl Renderer {
    pub fn pixels(&mut self) -> &mut PixelBuffer {
        &mut self.pixels
    }

    pub fn draw(&mut self) {
        let layer = &self.state.layers[0];

        self.state.queue.write_texture(
            wgpu::TextureCopyView {
                texture: &layer.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
            },
            &self.pixels,
            wgpu::TextureDataLayout {
                offset: 0,
                bytes_per_row: 4 * layer.width,
                rows_per_image: layer.height,
            },
            layer.texture_size,
        );
    }

    pub fn render(&mut self) {
        self.state.render();
    }

    pub fn resize(&mut self, new_size: PhysicalSize<u32>) {
        self.state.resize(new_size);
    }

    pub fn new(w: usize, h: usize, window: &Window) -> Self {
        let pixel_count = w * h;

        Self {
            pixels: PixelBuffer::with_capacity(pixel_count),
            state: block_on(State::new(window)),
        }
    }
}

// -----------------------------------------------------------------------------
//     - State-
//     Maybe absolute nonsense:
//     Device -> [ Queue -> SwapChain -> RenderPipeline -> Surface ]
// -----------------------------------------------------------------------------

fn color_target_state() -> wgpu::ColorTargetState {
    wgpu::ColorTargetState {
        format: wgpu::TextureFormat::Bgra8UnormSrgb,
        color_blend: wgpu::BlendState::REPLACE,
        alpha_blend: wgpu::BlendState::REPLACE,
        write_mask: wgpu::ColorWrite::ALL,
    }
}

struct State {
    surface: wgpu::Surface,
    device: wgpu::Device,
    queue: wgpu::Queue,
    sc_desc: wgpu::SwapChainDescriptor,
    swap_chain: wgpu::SwapChain,
    size: PhysicalSize<u32>,
    render_pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    num_indices: u32,
    diffuse_bind_group: wgpu::BindGroup,
    layers: Vec<Layer>,
}

impl State {
    async fn new(window: &Window) -> Self {
        let size = window.inner_size();
        let instance = wgpu::Instance::new(wgpu::BackendBit::PRIMARY);
        let surface = unsafe { instance.create_surface(window) };
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance, //TODO: make this a setting
                compatible_surface: Some(&surface),
            })
            .await
            .unwrap();

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: None,
                    features: wgpu::Features::empty(),
                    limits: wgpu::Limits::default(),
                    // shader_validation: true, // TODO: where does this go now?
                },
                None,
            )
            .await
            .unwrap();

        let sc_desc = wgpu::SwapChainDescriptor {
            usage: wgpu::TextureUsage::RENDER_ATTACHMENT,
            format: wgpu::TextureFormat::Bgra8UnormSrgb,
            width: size.width,
            height: size.height,
            present_mode: wgpu::PresentMode::Fifo,
        };

        let swap_chain = device.create_swap_chain(&surface, &sc_desc);

        // -----------------------------------------------------------------------------
        //     - Texture -
        // -----------------------------------------------------------------------------
        // let diffuse_bytes = include_bytes!("../textures/supertexture.png");
        // let diffuse_image = image::load_from_memory(diffuse_bytes).unwrap();
        // let diffuse_rgba = diffuse_image.as_rgba8().unwrap();
        // let diffuse_rgba = red().into_iter().map(|p| p.bytes().into_iter()).flatten().collect::<Vec<_>>();

        let diffuse_rgba = PixelBuffer::with_capacity(128 * 128);

        use image::GenericImageView;
        let (width, height) = (128, 128); // diffuse_image.dimensions();
        let texture_size = wgpu::Extent3d {
            width,
            height,
            depth: 1,
        };

        let diffuse_texture = device.create_texture(&wgpu::TextureDescriptor {
            size: texture_size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsage::SAMPLED | wgpu::TextureUsage::COPY_DST,
            label: Some("omg textures!!!!"),
        });

        queue.write_texture(
            wgpu::TextureCopyView {
                texture: &diffuse_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
            },
            &diffuse_rgba,
            wgpu::TextureDataLayout {
                offset: 0,
                bytes_per_row: 4 * width,
                rows_per_image: height,
            },
            texture_size,
        );

        let diffuse_texture_view =
            diffuse_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let layer_one = Layer {
            width,
            height,
            texture: diffuse_texture,
            texture_size: wgpu::Extent3d {
                width: width,
                height: height,
                depth: 1,
            },
        };

        let diffuse_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStage::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Float { filterable: false },
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStage::FRAGMENT,
                        ty: wgpu::BindingType::Sampler {
                            comparison: false,
                            filtering: false,
                        },
                        count: None,
                    },
                ],
                label: Some("texture binding group layout"),
            });

        let diffuse_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &texture_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&diffuse_texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&diffuse_sampler),
                },
            ],
            label: Some("meh"),
        });

        // -----------------------------------------------------------------------------
        //     - Shader bits -
        // -----------------------------------------------------------------------------
        let vs_module = device.create_shader_module(&wgpu::include_spirv!("shader.vert.spv"));
        let fs_module = device.create_shader_module(&wgpu::include_spirv!("shader.frag.spv"));

        // buffer business
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Vertex buffer yaaaay"),
            contents: bytemuck::cast_slice(VERTICES),
            usage: wgpu::BufferUsage::VERTEX,
        });

        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Index buffer because things aren't hard enough as they are"),
            contents: bytemuck::cast_slice(INDICES),
            usage: wgpu::BufferUsage::INDEX,
        });

        // -----------------------------------------------------------------------------
        //     - Pipeline -
        // -----------------------------------------------------------------------------
        let render_pipeline = create_pipeline(
            &device,
            &sc_desc,
            vs_module,
            fs_module,
            texture_bind_group_layout,
        );

        Self {
            surface,
            device,
            queue,
            sc_desc,
            swap_chain,
            size,
            render_pipeline,
            vertex_buffer,
            index_buffer,
            num_indices: INDICES.len() as u32,
            diffuse_bind_group,
            layers: vec![layer_one],
        }
    }

    fn resize(&mut self, new_size: PhysicalSize<u32>) {
        self.size = new_size;
        self.sc_desc.width = new_size.width;
        self.sc_desc.height = new_size.height;
        self.swap_chain = self.device.create_swap_chain(&self.surface, &self.sc_desc);
    }

    fn input(&mut self, event: &WindowEvent) -> bool {
        false
    }

    fn draw(&mut self) {
        // let diffuse_rgba = PixelBuffer::with_capacity();
        // let mut diffuse_rgba = blue().into_iter().map(|p| p.bytes()).flatten().collect::<Vec<_>>();
        // let index = diffuse_rgba.len() / 2;
        // diffuse_rgba.iter_mut().skip(index).map(|p| *p = 255).collect::<Vec<_>>();

        // let layer = &self.layers[0];

        // self.queue.write_texture(
        //     wgpu::TextureCopyView {
        //         texture: &layer.texture,
        //         mip_level: 0,
        //         origin: wgpu::Origin3d::ZERO,
        //     },
        //     diffuse_rgba.as_slice(),
        //     wgpu::TextureDataLayout {
        //         offset: 0,
        //         bytes_per_row: 4 * layer.width,
        //         rows_per_image: layer.height,
        //     },
        //     layer.texture_size,
        // );
    }

    fn render(&mut self) {
        let frame = self
            .swap_chain
            .get_current_frame()
            .expect("Timeout?")
            .output;

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("I haz label"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[wgpu::RenderPassColorAttachmentDescriptor {
                    attachment: &frame.view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.0,
                            g: 0.0,
                            b: 0.0,
                            a: 1.0,
                        }),
                        store: true,
                    },
                }],
                depth_stencil_attachment: None,
            });

            render_pass.set_pipeline(&self.render_pipeline);
            render_pass.set_bind_group(0, &self.diffuse_bind_group, &[]);
            render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            render_pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            render_pass.draw_indexed(0..self.num_indices, 0, 0..1);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
    }
}

// -----------------------------------------------------------------------------
//     - Create pipeline -
// -----------------------------------------------------------------------------
fn create_pipeline(
    device: &wgpu::Device,
    sc_desc: &wgpu::SwapChainDescriptor,
    vs_module: wgpu::ShaderModule,
    fs_module: wgpu::ShaderModule,
    texture_bind_group: wgpu::BindGroupLayout,
) -> wgpu::RenderPipeline {
    let render_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Render pipeline layout what does this even mean"),
        bind_group_layouts: &[&texture_bind_group],
        push_constant_ranges: &[],
    });

    let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Pipeline omg pipeline (render okay)"),
        layout: Some(&render_pipeline_layout),
        vertex: wgpu::VertexState {
            module: &vs_module,
            entry_point: "main",
            buffers: &[Vertex::desc()],
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            polygon_mode: wgpu::PolygonMode::Fill,
            cull_mode: Some(wgpu::Face::Back),
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState {
            count: 1,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        fragment: Some(wgpu::FragmentState {
            module: &fs_module,
            entry_point: "main",
            targets: &[color_target_state()], //TODO finish
        })



        // vertex_stage: wgpu::ProgrammableStageDescriptor {
        //     module: &vs_module,
        //     entry_point: "main",
        // },
        // fragment_stage: Some(wgpu::ProgrammableStageDescriptor {
        //     module: &fs_module,
        //     entry_point: "main",
        // }),
        // rasterization_state: Some(wgpu::RasterizationStateDescriptor {
        //     front_face: wgpu::FrontFace::Ccw,
        //     cull_mode: wgpu::CullMode::Back,
        //     depth_bias: 0,
        //     depth_bias_slope_scale: 0.0,
        //     depth_bias_clamp: 0.0,
        //     clamp_depth: false,
        // }),
        // color_states: &[wgpu::ColorStateDescriptor {
        //     format: sc_desc.format,
        //     color_blend: wgpu::BlendDescriptor::REPLACE,
        //     alpha_blend: wgpu::BlendDescriptor::REPLACE,
        //     write_mask: wgpu::ColorWrite::ALL,
        // }],
        // primitive_topology: wgpu::PrimitiveTopology::TriangleList,
        // depth_stencil: None,
        // vertex_state: wgpu::VertexStateDescriptor {
        //     index_format: wgpu::IndexFormat::Uint16,
            // vertex_buffers: &[Vertex::desc()],
        // },
        // sample_count: 1,
        // sample_mask: !0,
        // alpha_to_coverage_enabled: false,
    });

    render_pipeline
}
