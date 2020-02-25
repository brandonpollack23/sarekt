use log::Level;
use sarekt::{self, error::SarektError, renderer::VulkanRenderer};
use std::{error::Error, sync::Arc};
use winit::{event_loop::EventLoop, window::WindowBuilder};

struct SarektApp {
  _renderer: VulkanRenderer,
  _event_loop: EventLoop<()>,
}
impl SarektApp {
  fn new() -> Result<Self, SarektError> {
    println!("Creating App");

    let event_loop = EventLoop::new();
    let window = Arc::new(WindowBuilder::new().build(&event_loop).unwrap());
    let renderer = VulkanRenderer::new(window.clone()).unwrap();

    Ok(Self {
      _event_loop: event_loop,
      _renderer: renderer,
    })
  }

  fn run(&mut self) {
    println!("Running App");
  }
}

fn main() -> Result<(), Box<dyn Error>> {
  simple_logger::init_with_level(Level::Info)?;
  let mut app = SarektApp::new()?;
  app.run();
  Ok(())
}
