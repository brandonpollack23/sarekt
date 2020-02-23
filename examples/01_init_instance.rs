use sarekt::error::SarektError;
use std::error::Error;

struct SarektApp;
impl SarektApp {
  fn new() -> Result<Self, SarektError> {
    println!("Creating App");
    Ok(Self)
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
