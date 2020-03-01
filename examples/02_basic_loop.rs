use log::{info, Level};
use sarekt::{self, error::SarektError, renderer::VulkanRenderer};
use std::{error::Error, sync::Arc};
use winit::{
  event::{ElementState, Event, VirtualKeyCode, WindowEvent},
  event_loop::{ControlFlow, EventLoop},
  window::{Window, WindowBuilder, WindowId},
};

const WIDTH: u32 = 800;
const HEIGHT: u32 = 800;

struct SarektApp {
  renderer: VulkanRenderer,
  event_loop: EventLoop<()>,
  window: Arc<Window>,
}
impl SarektApp {
  fn new() -> Result<Self, SarektError> {
    info!("Creating App");

    let event_loop = EventLoop::new();
    let window = Arc::new(WindowBuilder::new().build(&event_loop).unwrap());
    let renderer = VulkanRenderer::new(window.clone(), WIDTH, HEIGHT).unwrap();

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
          // TODO
          // renderer.draw_frame();
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
    _renderer: &mut VulkanRenderer,
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
      WindowEvent::Resized(_size) => {
        // TODO
        // If the size is 0, minimization or something like that happened so I
        // toggle drawing.
        // let enabled = !(size.height == 0 && size.width == 0);
        // renderer.set_rendering_enabled(enabled);

        // renderer.notify_window_resized()
      }
      _ => (),
    }
  }
}

fn main() -> Result<(), Box<dyn Error>> {
  simple_logger::init_with_level(Level::Warn)?;
  let app = SarektApp::new().expect("Could not create instance!");
  app.main_loop();
  Ok(())
}
