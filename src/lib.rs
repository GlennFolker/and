use pollster::FutureExt as _;
use winit::{
    dpi::PhysicalSize,
    event::{
        Event, WindowEvent,
        StartCause,
    },
    event_loop::{
        EventLoop, EventLoopWindowTarget,
        ControlFlow,
    },
    window::{
        Window, WindowBuilder,
    },
};
use std::iter;

#[cfg(target_os = "android")]
#[ndk_glue::main]
fn main() {
    run();
}

struct State {
    window: Window,
    surface: wgpu::Surface,
    config: wgpu::SurfaceConfiguration,
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::RenderPipeline,
}

impl State {
    fn new(event_loop: &EventLoopWindowTarget<()>) -> Self {
        let window = WindowBuilder::new()
            .with_title("And".to_string())
            .with_resizable(false)
            .with_window_icon((|| {
                #[cfg(target_os = "android")]
                return None;
                #[cfg(not(target_os = "android"))]
                {
                    use image::{
                        load_from_memory_with_format,
                        ImageFormat,
                    };
                    use winit::window::Icon;

                    let bytes = include_bytes!("../res/mipmap-xxxhdpi/icon.png");
                    let img = load_from_memory_with_format(bytes, ImageFormat::Png)
                        .expect("Couldn't load icon")
                        .into_rgba8();
                    let (width, height) = img.dimensions();
                    Some(Icon::from_rgba(img.into_vec(), width, height).expect("Couldn't set icon"))
                }
            })())
            .build(&event_loop)
            .expect("Unable to create window");
        let PhysicalSize { width, height, } = window.inner_size();

        let instance = wgpu::Instance::new(wgpu::Backends::all());
        let surface = unsafe { instance.create_surface(&window) };
        let adapter = instance.request_adapter(
            &wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            },
        ).block_on().or_else(|| instance.enumerate_adapters(wgpu::Backends::all())
            .filter(|adapter| !surface.get_supported_formats(&adapter).is_empty())
            .next()
        ).expect("Unable to request video adapter.");
        let (device, queue) = adapter.request_device(
            &wgpu::DeviceDescriptor {
                features: wgpu::Features::empty(),
                limits: wgpu::Limits::default(),
                label: None,
            },
            None,
        ).block_on().expect("Unable to request WGPU device and render queue");
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: {
                let format = surface.get_supported_formats(&adapter)[0];
                log::info!("Using surface format {format:?}");
                format
            },
            width, height,
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
        };
        surface.configure(&device, &config);
        let shader = device.create_shader_module(wgpu::include_wgsl!("shader.wgsl"));
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Pipeline layout"),
            bind_group_layouts: &[],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
        });

        Self { window, surface, config, device, queue, pipeline, }
    }
}

pub fn run() {
    #[cfg(target_os = "android")]
    android_logger::init_once(android_logger::Config::default()
        .with_min_level(log::Level::Info)
    );

    #[cfg(not(target_os = "android"))]
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .init();

    let event_loop = EventLoop::new();
    let mut state = None;

    event_loop.run(move |event, event_loop, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
            Event::NewEvents(StartCause::Init) => log::info!("Hello, world!"),
            Event::Resumed => {
                log::info!("Hello again, world!");
                if state.is_none() {
                    let st = State::new(&event_loop);
                    st.window.request_redraw();
                    state = Some(st);
                }
            },
            Event::Suspended => {
                log::info!("Where are you going, world?");
                state = None;
            },
            Event::WindowEvent { window_id, event } => {
                let Some(st) = state.as_mut() else { return };
                if window_id != st.window.id() { return };

                match event {
                    WindowEvent::Resized(PhysicalSize { width, height, }) => {
                        if width == 0 || height == 0 { return };

                        st.config.width = width;
                        st.config.height = height;
                        st.surface.configure(&st.device, &st.config);
                    },
                    WindowEvent::CloseRequested => *control_flow = ControlFlow::ExitWithCode(0),
                    _ => {},
                }
            },
            Event::RedrawRequested(window_id) => {
                let Some(st) = state.as_ref() else { return };
                if window_id != st.window.id() { return };

                let output = match st.surface.get_current_texture() {
                    Ok(output) => output,
                    Err(err) => {
                        match err {
                            wgpu::SurfaceError::Lost => st.surface.configure(&st.device, &st.config),
                            wgpu::SurfaceError::OutOfMemory => *control_flow = ControlFlow::ExitWithCode(1),
                            e => log::error!("Skipping frame due to {e:?}"),
                        }

                        return;
                    },
                };

                let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());
                let mut encoder = st.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Screen renderer"),
                });

                {
                    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("Screen pass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &view,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.1, g: 0.2, b: 0.3, a: 1.0, }),
                                store: true,
                            },
                        })],
                        depth_stencil_attachment: None,
                    });

                    pass.set_pipeline(&st.pipeline);
                    pass.draw(0..3, 0..1);
                }

                st.queue.submit(iter::once(encoder.finish()));
                output.present();
            },
            Event::LoopDestroyed => {
                state = None;
                log::info!("Goodbye, world!");
            },
            _ => {},
        }
    });
}
