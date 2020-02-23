use sarekt::{error::SarektError, renderer::Renderer};
use std::{error::Error, sync::Arc};
use winit::{event_loop::EventLoop, window::WindowBuilder};

struct SarektApp {
  renderer: Renderer,
  event_loop: EventLoop<()>,
}
impl SarektApp {
  fn new() -> Result<Self, SarektError> {
    println!("Creating App");

    let event_loop = EventLoop::new();
    let window = Arc::new(WindowBuilder::new().build(&event_loop).unwrap());
    let renderer = Renderer::new(window.clone()).unwrap();

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
  let mut app = SarektApp::new()?;
  app.run();
  Ok(())
}
