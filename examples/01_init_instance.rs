use log::Level;
use sarekt::{
  error::SarektError,
  renderer::{Renderer, VulkanRenderer},
};
use std::{error::Error, sync::Arc};
use winit::{event_loop::EventLoop, window::WindowBuilder};

struct SarektApp {
  renderer: VulkanRenderer,
  event_loop: EventLoop<()>,
}
impl SarektApp {
  fn new() -> Result<Self, SarektError> {
    println!("Creating App");

    let event_loop = EventLoop::new();
    let window = Arc::new(WindowBuilder::new().build(&event_loop).unwrap());
    let renderer = VulkanRenderer::new(window.clone()).unwrap();

    Ok(Self {
      event_loop,
      renderer,
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
