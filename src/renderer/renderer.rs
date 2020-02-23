use ash::{extensions::khr::Surface, version::EntryV1_0, vk, Entry, Instance};
use std::ffi::CString;

use crate::error::SarektError;
use raw_window_handle::HasRawWindowHandle;
use std::sync::Arc;

// #[cfg(debug_assertions)]
// const ENABLE_VALIDATION_LAYERS: bool = true;
// #[cfg(not(debug_assertions))]
// const ENABLE_VALIDATION_LAYERS: bool = false;

pub struct Renderer {
  _entry: Entry,
  instance: Instance,
}
impl Renderer {
  /// Creates a renderer with no application name, no engine, and base versions
  /// of 0.1.0.
  fn new<W: HasRawWindowHandle, OW: Into<Option<Arc<W>>>>(window: OW) -> Result<Self, SarektError> {
    Self::new_detailed(
      window,
      "Nameless Application",
      (0, 1, 0),
      "No Engine",
      (0, 1, 0),
    )
  }

  /// Creates a renderer with a given name/version/engine name/engine version.
  fn new_detailed<W: HasRawWindowHandle, OW: Into<Option<Arc<W>>>>(
    window: OW, application_name: &str,
    (app_version_major, app_version_minor, app_version_patch): (u32, u32, u32), engine_name: &str,
    (engine_version_major, engine_version_minor, engine_version_patch): (u32, u32, u32),
  ) -> Result<Self, SarektError> {
    // TODO
    // * Support rendering to a non window surface if window is None (change it to
    //   an Enum of WindowHandle or OtherSurface).
    println!("Creating Sarekt Renderer with Vulkan Backend...");
    let window = window
      .into()
      .expect("Sarekt only supports rendering to a window right now :(");

    let entry = ash::Entry::new().expect("Failed to load dynamic library and create Vulkan Entry");
    let instance = Self::create_instance(
      &entry,
      window.as_ref(),
      application_name,
      ash::vk::make_version(app_version_major, app_version_minor, app_version_patch),
      engine_name,
      ash::vk::make_version(
        engine_version_major,
        engine_version_minor,
        engine_version_patch,
      ),
    )?;

    Ok(Self {
      _entry: entry,
      instance,
    })
  }

  /// Creates an instance of the Vulkan client side driver.
  fn create_instance<W: HasRawWindowHandle>(
    entry: &Entry, window: &W, application_name: &str, application_version: u32, engine_name: &str,
    engine_version: u32,
  ) -> Result<Instance, SarektError> {
    // TODO
    // * Detect vulkan versions available?
    let app_info = vk::ApplicationInfo::builder()
      .application_name(CString::new(application_name)?.as_c_str())
      .application_version(application_version)
      .engine_name(CString::new(engine_name)?.as_c_str())
      .engine_version(engine_version)
      .api_version(ash::vk::make_version(0, 1, 0))
      .build();

    let extension_names = Self::get_required_extensions(window);
    let instance_create_info = vk::InstanceCreateInfo::builder()
      .application_info(&app_info)
      .enabled_layer_names(&[])
      .enabled_extension_names(&extension_names)
      .build();

    unsafe {
      entry
        .create_instance(&instance_create_info, None)
        .map_err(|err| SarektError::CouldNotCreateInstance("Error creating intance", err))
    }
  }

  fn get_required_extensions<W: HasRawWindowHandle>(window: &W) -> Vec<*const i8> {
    let surface_extension = Surface::name().as_ptr();
    let window_system_surface_extension = ash_window::enumerate_required_extension(window).as_ptr();
    vec![surface_extension, window_system_surface_extension]
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  #[cfg(unix)]
  use winit::platform::windows::EventLoopExtUnix;
  #[cfg(windows)]
  use winit::platform::windows::EventLoopExtWindows;
  use winit::{event_loop::EventLoop, window::WindowBuilder};

  #[test]
  fn can_construct_renderer_with_new() {
    let event_loop = EventLoop::<()>::new_any_thread();
    let window = Arc::new(WindowBuilder::new().build(&event_loop).unwrap());
    Renderer::new(window.clone()).unwrap();
  }

  #[test]
  fn can_construct_renderer_with_new_detailed() {
    let event_loop = EventLoop::<()>::new_any_thread();
    let window = Arc::new(WindowBuilder::new().build(&event_loop).unwrap());
    Renderer::new_detailed(
      window.clone(),
      "Testing App",
      (0, 1, 0),
      "No Test Engine",
      (0, 1, 0),
    )
    .unwrap();
  }
}
