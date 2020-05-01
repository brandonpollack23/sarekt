use log::{info, warn, Level};
use sarekt::{
  self,
  error::SarektError,
  renderer::{config::Config, Renderer, VulkanRenderer},
};
use std::{error::Error, sync::Arc};
use winit::{
  dpi::{LogicalSize, PhysicalSize},
  event::{ElementState, Event, VirtualKeyCode, WindowEvent},
  event_loop::{ControlFlow, EventLoop},
  window::{Window, WindowBuilder, WindowId},
};

const WIDTH: u32 = 800;
const HEIGHT: u32 = 600;

struct SarektApp {
  renderer: VulkanRenderer,
  event_loop: EventLoop<()>,
  window: Arc<Window>,
}
impl SarektApp {
  fn new() -> Result<Self, SarektError> {
    info!("Creating App");

    let event_loop = EventLoop::new();
    let window = Arc::new(
      WindowBuilder::new()
        .with_inner_size(LogicalSize::new(WIDTH, HEIGHT))
        .build(&event_loop)
        .unwrap(),
    );
    let config = Config::builder()
      .requested_width(WIDTH)
      .requested_height(HEIGHT)
      .build()
      .unwrap();
    let renderer = VulkanRenderer::new(window.clone(), config).unwrap();

    Ok(Self {
      renderer,
      event_loop,
      window,
    })
  }

  /// Takes full control of the executing thread and runs the event loop for it.
  fn main_loop(self) {
    info!("Running main loop...");
    let window = self.window;
    let mut renderer = self.renderer;
    self.event_loop.run(move |event, _, control_flow| {
      // By default continuously run this event loop, even if the OS hasn't
      // distributed an event, that way we will draw as fast as possible.
      *control_flow = ControlFlow::Poll;

      match event {
        Event::MainEventsCleared => {
          // All the main events to process are done we can do "work" now (game
          // engine state update etc.)

          // Nothing to do though...yet.

          // At the end of "work" request redraw.
          window.request_redraw();
        }
        Event::RedrawRequested(_) => {
          // Redraw requested, this is called after MainEventsCleared.
          renderer.frame().unwrap_or_else(|err| {
            match err {
              SarektError::SwapchainOutOfDate => {
                // Handle window resize etc.
                warn!("Tried to render without processing window resize event!");
                let PhysicalSize { width, height } = window.inner_size();
                renderer.recreate_swapchain(width, height).unwrap();
              }
              e => panic!(e),
            }
          });
        }
        Event::WindowEvent { window_id, event } => {
          Self::main_loop_window_event(&event, &window_id, control_flow, &mut renderer);
        }
        _ => (),
      }
    });
  }

  fn main_loop_window_event(
    event: &WindowEvent, _id: &WindowId, control_flow: &mut winit::event_loop::ControlFlow,
    renderer: &mut VulkanRenderer,
  ) {
    match event {
      WindowEvent::CloseRequested => {
        // When the window system requests a close, signal to winit that we'd like to
        // close the window.
        info!("Exiting due to close request event from window system...");
        *control_flow = ControlFlow::Exit
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
        renderer
          .recreate_swapchain(size.width, size.height)
          .unwrap();
      }
      _ => (),
    }
  }
}

fn main() -> Result<(), Box<dyn Error>> {
  simple_logger::init_with_level(Level::Info)?;
  let app = SarektApp::new().expect("Could not create instance!");
  app.main_loop();
  Ok(())
}
