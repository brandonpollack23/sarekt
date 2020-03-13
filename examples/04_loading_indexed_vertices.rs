use lazy_static::lazy_static;
use log::{info, warn, Level};
use sarekt::{
  self,
  error::{SarektError, SarektResult},
  renderer::{
    buffers::{BufferType, IndexBufferElemSize},
    drawable_object::DrawableObject,
    vertex_bindings::DefaultForwardShaderVertex,
    Drawer, Renderer, VulkanRenderer,
  },
};
use std::{error::Error, sync::Arc};
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
  DefaultForwardShaderVertex::new(&[-0.5f32, -0.5f32], &[1.0f32, 0.0f32, 0.0f32]), // Top Left, Red
  DefaultForwardShaderVertex::new(&[0.5f32, -0.5f32], &[0.0f32, 1.0f32, 0.0f32]), // Top Right, Green
  DefaultForwardShaderVertex::new(&[0.5f32, 0.5f32], &[0.0f32, 0.0f32, 1.0f32]),  // Bottom Right, Blue
  DefaultForwardShaderVertex::new(&[-0.5f32, 0.5f32], &[1.0f32, 1.0f32, 1.0f32]), // Bottom Left, White
];
}
const RECT_INDICES: [u16; 6] = [0u16, 1u16, 2u16, 2u16, 3u16, 0u16]; // two triangles, upper right and lower left

fn main() -> Result<(), Box<dyn Error>> {
  simple_logger::init_with_level(Level::Info)?;
  main_loop()?;
  Ok(())
}

/// Takes full control of the executing thread and runs the event loop for it.
fn main_loop() -> SarektResult<()> {
  info!("Running main loop...");

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

  // Create Resources.
  let rect_vertex_buffer = renderer.load_buffer(BufferType::Vertex, &RECT_VERTICES)?;
  let rect_index_buffer = renderer.load_buffer(
    BufferType::Index(IndexBufferElemSize::UInt16),
    &RECT_INDICES,
  )?;
  let rect = DrawableObject::new_indexed(&renderer, &rect_vertex_buffer, &rect_index_buffer, None)?;

  // Run the loop.
  event_loop.run_return(move |event, _, control_flow| {
    // By default continuously run this event loop, even if the OS hasn't
    // distributed an event, that way we will draw as fast as possible.
    *control_flow = ControlFlow::Poll;

    match event {
      Event::MainEventsCleared => {
        // All the main events to process are done we can do "work" now (game
        // engine state update etc.)

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
        main_loop_window_event(&event, &window_id, control_flow, &mut renderer)
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

/// Handles all winit window specific events.
fn main_loop_window_event(
  event: &WindowEvent, _id: &WindowId, control_flow: &mut winit::event_loop::ControlFlow,
  renderer: &mut VulkanRenderer,
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
      renderer.set_rendering_enabled(enabled);
      return renderer.recreate_swapchain(size.width, size.height);
    }

    _ => (),
  }

  Ok(())
}
