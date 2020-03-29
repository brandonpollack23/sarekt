use image::GenericImageView;
use lazy_static::lazy_static;
use log::{info, warn, Level};
use sarekt::{
  self,
  error::{SarektError, SarektResult},
  renderer::{
    buffers_and_images::{
      BufferType, IndexBufferElemSize, MagnificationMinificationFilter, TextureAddressMode,
    },
    drawable_object::DrawableObject,
    vertex_bindings::{DefaultForwardShaderLayout, DefaultForwardShaderVertex},
    Drawer, Renderer, VulkanRenderer,
  },
};
use std::{error::Error, f32, sync::Arc, time::Instant};
use ultraviolet as uv;
use winit::{
  dpi::{LogicalSize, PhysicalSize},
  event::{ElementState, Event, VirtualKeyCode, WindowEvent},
  event_loop::{ControlFlow, EventLoop},
  platform::desktop::EventLoopExtDesktop,
  window::{WindowBuilder, WindowId},
};

const WIDTH: u32 = 800;
const HEIGHT: u32 = 600;

lazy_static! {
static ref RECT_VERTICES: Vec<DefaultForwardShaderVertex> = vec![
  // Texture is bound to the four corners.
  DefaultForwardShaderVertex::new(&[-0.5f32, 0.0f32, -0.5f32], &[1.0f32, 0.0f32, 0.0f32], &[1.0f32, 0.0f32]), // Top Left, Red,
  DefaultForwardShaderVertex::new(&[0.5f32, 0.0f32, -0.5f32], &[0.0f32, 1.0f32, 0.0f32], &[0.0f32, 0.0f32]), // Top Right, Green
  DefaultForwardShaderVertex::new(&[0.5f32, 0.0f32, 0.5f32], &[0.0f32, 0.0f32, 1.0f32], &[0.0f32, 1.0f32]),  // Bottom Right, Blue
  DefaultForwardShaderVertex::new(&[-0.5f32, 0.0f32, 0.5f32], &[1.0f32, 1.0f32, 1.0f32], &[1.0f32, 1.0f32]), // Bottom Left, White
];
}
const RECT_INDICES: [u16; 6] = [0u16, 1u16, 2u16, 2u16, 3u16, 0u16]; // two triangles, upper right and lower left

fn main() -> Result<(), Box<dyn Error>> {
  simple_logger::init_with_level(Level::Info)?;
  main_loop()?;
  Ok(())
}

/// Takes full control of the executing thread and runs the event loop for it.
fn main_loop() -> Result<(), Box<dyn Error>> {
  info!("Running main loop...");

  let mut ar = WIDTH as f32 / HEIGHT as f32;

  // Build Window.
  let mut event_loop = EventLoop::new();
  let window = Arc::new(
    WindowBuilder::new()
      .with_inner_size(LogicalSize::new(WIDTH, HEIGHT))
      .build(&event_loop)
      .unwrap(),
  );

  // Build Renderer.
  let mut renderer = VulkanRenderer::new(window.clone(), WIDTH, HEIGHT).unwrap();

  // Create Vertex Resources.
  let rect_vertex_buffer = renderer.load_buffer(BufferType::Vertex, &RECT_VERTICES)?;
  let rect_index_buffer = renderer.load_buffer(
    BufferType::Index(IndexBufferElemSize::UInt16),
    &RECT_INDICES,
  )?;

  // Create MVP uniform.
  let rect_uniform = DefaultForwardShaderLayout::default();
  let rect_uniform_buffer = renderer.load_uniform_buffer(rect_uniform)?;

  // Load textures and create image.
  let spoderman = image::open("textures/spoderman.gif")?;
  info!("spoderman dimensions: {:?}", spoderman.dimensions());
  let image = renderer.load_image_with_staging_rgba_32(
    spoderman,
    MagnificationMinificationFilter::Linear,
    MagnificationMinificationFilter::Linear,
    TextureAddressMode::Repeat,
    TextureAddressMode::Repeat,
    TextureAddressMode::Repeat,
  )?;

  let rect = DrawableObject::builder(&renderer)
    .vertex_buffer(&rect_vertex_buffer)
    .index_buffer(&rect_index_buffer)
    .uniform_buffer(&rect_uniform_buffer)
    .texture_image(&image)
    .build()?;

  let rect2_uniform_buffer = renderer.load_uniform_buffer(rect_uniform)?;
  let rect2 = DrawableObject::builder(&renderer)
    .vertex_buffer(&rect_vertex_buffer)
    .index_buffer(&rect_index_buffer)
    .uniform_buffer(&rect2_uniform_buffer)
    .texture_image(&image)
    .build()?;

  let args: Vec<String> = std::env::args().collect();
  let enable_colors = args.contains(&"colors".to_owned());
  info!("Colors enabled: {}", enable_colors);

  let start_time = Instant::now();

  let mut camera_height = 0.5f32;

  // Run the loop.
  event_loop.run_return(move |event, _, control_flow| {
    // By default continuously run this event loop, even if the OS hasn't
    // distributed an event, that way we will draw as fast as possible.
    *control_flow = ControlFlow::Poll;

    match event {
      Event::MainEventsCleared => {
        // All the main events to process are done we can do "work" now (game
        // engine state update etc.)
        let now = Instant::now();
        let time_since_start_secs = ((now - start_time).as_millis() as f32) / 1000f32;

        // Rise to max height then gently go back down.
        let camera_rate = 0.5f32;
        let min_camera_height = 0.5f32;
        let camera_range = 1.5f32;
        camera_height =
          (camera_rate * time_since_start_secs) % (2.0f32 * camera_range) + min_camera_height;
        if camera_height >= (camera_range + min_camera_height) {
          camera_height = (2.0f32 * (camera_range + min_camera_height)) - camera_height;
        }

        let rotation =
          (std::f32::consts::PI * time_since_start_secs) % (2f32 * std::f32::consts::PI);
        update_uniforms(
          &renderer,
          &rect,
          uv::Vec3::new(0.0f32, 0.0f32, -1.0f32),
          rotation,
          camera_height,
          enable_colors,
          time_since_start_secs,
          ar,
        )
        .unwrap();
        update_uniforms(
          &renderer,
          &rect2,
          uv::Vec3::new(0.5f32, 0.5f32, -1.0f32),
          -rotation,
          camera_height,
          enable_colors,
          time_since_start_secs,
          ar,
        )
        .unwrap();

        renderer.draw(&rect2).unwrap();
        renderer.draw(&rect).unwrap();

        // At the end of work request redraw.
        window.request_redraw();
      }

      Event::RedrawRequested(_) => {
        // Redraw requested, this is called after MainEventsCleared.
        renderer.frame().unwrap_or_else(|err| {
          match err {
            SarektError::SwapchainOutOfDate | SarektError::SuboptimalSwapchain => {
              // Handle window resize etc.
              warn!("Tried to render without processing window resize event!");

              let PhysicalSize { width, height } = window.inner_size();
              renderer
                .recreate_swapchain(width, height)
                .expect("Error recreating swapchain");
            }
            e => panic!("Frame had an unrecoverable error! {}", e),
          }
        });
      }

      Event::WindowEvent { window_id, event } => {
        main_loop_window_event(&event, &window_id, control_flow, &mut renderer, &mut ar)
          .expect("Error processing window event.");
      }

      Event::LoopDestroyed => {
        // Explicitly call exit so resources are cleaned up.
        std::process::exit(0);
      }
      _ => (),
    }
  });

  Ok(())
}

fn update_uniforms(
  renderer: &VulkanRenderer, rect: &DrawableObject<VulkanRenderer, DefaultForwardShaderLayout>,
  position: uv::Vec3, rotation: f32, camera_height: f32, enable_colors: bool,
  time_since_start_secs: f32, ar: f32,
) -> SarektResult<()> {
  // Pi radians per second around the z axis.
  let model_matrix = uv::Mat4::from_translation(position) * uv::Mat4::from_rotation_y(rotation);
  let view_matrix = uv::Mat4::look_at(
    /* eye= */ uv::Vec3::new(0.0f32, camera_height, 0.0f32),
    /* at= */ uv::Vec3::new(0f32, 0f32, -1f32),
    /* up= */ uv::Vec3::unit_y(),
  );
  // TODO BACKENDS this proj should be conditional on backend.
  let perspective_matrix =
    uv::projection::rh_yup::perspective_vk(std::f32::consts::PI / 2f32, ar, 0.1f32, 10f32);

  let uniform = DefaultForwardShaderLayout::new(
    perspective_matrix * view_matrix * model_matrix,
    enable_colors,
    /* enable_texture_mixing= */ true,
  );
  rect.set_uniform(renderer, &uniform)
}

/// Handles all winit window specific events.
fn main_loop_window_event(
  event: &WindowEvent, _id: &WindowId, control_flow: &mut winit::event_loop::ControlFlow,
  renderer: &mut VulkanRenderer, ar: &mut f32,
) -> SarektResult<()> {
  match event {
    WindowEvent::CloseRequested => {
      // When the window system requests a close, signal to winit that we'd like to
      // close the window.
      info!("Exiting due to close request event from window system...");
      *control_flow = ControlFlow::Exit;
    }

    WindowEvent::KeyboardInput { input, .. } => {
      // When the keyboard input is a press on the escape key, exit and print the
      // line.
      if let (Some(VirtualKeyCode::Escape), ElementState::Pressed) =
        (input.virtual_keycode, input.state)
      {
        info!("Exiting due to escape press...");
        *control_flow = ControlFlow::Exit
      }
    }

    WindowEvent::Resized(size) => {
      // If the size is 0, minimization or something like that happened so I
      // toggle drawing.
      info!("Window resized, recreating renderer swapchain...");
      let enabled = !(size.height == 0 && size.width == 0);
      if enabled {
        *ar = size.width as f32 / size.height as f32;
      }
      renderer.set_rendering_enabled(enabled);
      return renderer.recreate_swapchain(size.width, size.height);
    }

    _ => (),
  }

  Ok(())
}
