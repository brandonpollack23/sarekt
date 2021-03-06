use lazy_static::lazy_static;
use log::{info, warn, Level};
use sarekt::{
  self,
  error::{SarektError, SarektResult},
  renderer::{
    buffers_and_images::{BufferType, IndexBufferElemSize},
    config::Config,
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
  DefaultForwardShaderVertex::without_uv(&[-0.5f32, -0.5f32, 0.0f32], &[1.0f32, 0.0f32, 0.0f32]), // Top Left, Red
  DefaultForwardShaderVertex::without_uv(&[0.5f32, -0.5f32, 0.0f32], &[0.0f32, 1.0f32, 0.0f32]), // Top Right, Green
  DefaultForwardShaderVertex::without_uv(&[0.5f32, 0.5f32, 0.0f32], &[0.0f32, 0.0f32, 1.0f32]),  // Bottom Right, Blue
  DefaultForwardShaderVertex::without_uv(&[-0.5f32, 0.5f32, 0.0f32], &[1.0f32, 1.0f32, 1.0f32]), // Bottom Left, White
];
}
const RECT_INDICES: [u16; 6] = [0u16, 2u16, 1u16, 2u16, 0u16, 3u16]; // two triangles, upper right and lower left

fn main() -> Result<(), Box<dyn Error>> {
  simple_logger::init_with_level(Level::Info)?;
  main_loop()?;
  Ok(())
}

/// Takes full control of the executing thread and runs the event loop for it.
fn main_loop() -> SarektResult<()> {
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
  let config = Config::builder()
    .requested_width(WIDTH)
    .requested_height(HEIGHT)
    .build()
    .unwrap();
  let mut renderer = VulkanRenderer::new(window.clone(), config).unwrap();

  // Create Resources.
  let rect_vertex_buffer = renderer.load_buffer(BufferType::Vertex, &RECT_VERTICES)?;
  let rect_index_buffer = renderer.load_buffer(
    BufferType::Index(IndexBufferElemSize::UInt16),
    &RECT_INDICES,
  )?;
  let rect_uniform = DefaultForwardShaderLayout::default();
  let rect_uniform_buffer = renderer.load_uniform_buffer(rect_uniform)?;
  let rect: DrawableObject = DrawableObject::builder(&renderer)
    .vertex_buffer(&rect_vertex_buffer)
    .index_buffer(&rect_index_buffer)
    .uniform_buffer(&rect_uniform_buffer)
    .build()?;

  let start_time = Instant::now();

  // Run the loop.
  event_loop.run_return(move |event, _, control_flow| {
    // By default continuously run this event loop, even if the OS hasn't
    // distributed an event, that way we will draw as fast as possible.
    *control_flow = ControlFlow::Poll;

    match event {
      Event::MainEventsCleared => {
        // All the main events to process are done we can do "work" now (game
        // engine state update etc.)

        update_uniforms(&renderer, &rect, start_time, ar).unwrap();
        renderer.draw(&rect).expect("Unable to draw triangle!");

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
  start_time: Instant, ar: f32,
) -> SarektResult<()> {
  let now = Instant::now();

  let time_since_start_secs = ((now - start_time).as_millis() as f32) / 1000f32;

  // Pi radians per second around the z axis.
  let rotation = (std::f32::consts::PI * time_since_start_secs) % (2f32 * std::f32::consts::PI);
  let model_matrix = uv::Mat4::from_rotation_z(rotation); // No scaling or translation.
  let view_matrix = uv::Mat4::look_at(
    /* eye= */ uv::Vec3::new(1.0f32, 1.0f32, 1.0f32),
    /* origin= */ uv::Vec3::zero(),
    /* up= */ uv::Vec3::unit_z(),
  );
  // TODO BACKENDS this proj should be conditional on backend.
  let perspective_matrix =
    uv::projection::rh_ydown::perspective_vk(std::f32::consts::PI / 2f32, ar, 0.1f32, 10f32);

  let uniform =
    DefaultForwardShaderLayout::new(perspective_matrix * view_matrix * model_matrix, true, false);
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
