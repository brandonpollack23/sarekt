use log::{info, Level};
use sarekt::{self, renderer::VulkanRenderer};
use std::{error::Error, sync::Arc};
use winit::{event_loop::EventLoop, window::WindowBuilder};

const WIDTH: u32 = 800;
const HEIGHT: u32 = 800;

fn main() -> Result<(), Box<dyn Error>> {
  simple_logger::init_with_level(Level::Info)?;
  info!("Creating App");

  let event_loop = EventLoop::new();
  let window = Arc::new(WindowBuilder::new().build(&event_loop).unwrap());
  let _renderer = VulkanRenderer::new(window, WIDTH, HEIGHT).unwrap();
  Ok(())
}
