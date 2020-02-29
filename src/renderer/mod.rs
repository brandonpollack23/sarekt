//! This is the main renderer module of Sarekt.
//!
//! As I learn vulkan and rendering better, so too will this documentation
//! improve.  For now in order to use Sarekt follow these steps:
//! 1) Create your system window (I reccomened using
//! [winit](https://www.crates.io/crates/winit))
//!
//! 2) Create the renderer with either `new`  or `new_detailed`, passing in your
//! system window.
//! ```no_run
//! use sarekt::renderer::VulkanRenderer;
//! use sarekt::renderer::Renderer;
//! # use std::sync::Arc;
//! # use winit::window::WindowBuilder;
//! # use winit::event_loop::EventLoop;
//! const WIDTH: u32 = 800;
//! const HEIGHT: u32 = 600;
//! let event_loop = EventLoop::new();
//! let window = Arc::new(WindowBuilder::new().build(&event_loop).unwrap());
//! let renderer = VulkanRenderer::new(window.clone(), WIDTH, HEIGHT).unwrap();
//! ```
//!
//! 3) That's all you can do.
//!
//! I hope to support some stuff like:
//! - [ ] Actually Rendering something
//! - [ ] Actually rendering something of your choosing
//! - [ ] Loading spirv shaders and generating internal information needed for
//!   them (for Vulkan that's descriptor sets/layouts)
//! - [ ] Support Other backends
//! - [ ] Moar.
//!
//! Things on the TODO list are in a repo TODO file.
pub mod shaders;
mod vulkan;

pub use vulkan::{
  debug_utils_ext::{DebugUserData, DebugUserDataCopy},
  vulkan_renderer::VulkanRenderer,
};

use crate::{error::SarektResult, renderer::shaders::ShaderCode};
use shaders::{ShaderHandle, ShaderType};

// ================================================================================
//  Compile Time Constants and Configurations
// ================================================================================
#[cfg(debug_assertions)]
const IS_DEBUG_MODE: bool = true;
#[cfg(not(debug_assertions))]
const IS_DEBUG_MODE: bool = false;
const ENABLE_VALIDATION_LAYERS: bool = IS_DEBUG_MODE;

// ================================================================================
//  Version struct
// ================================================================================
/// A simple version with major, minor and patch fields for specifying
/// information about your application.
pub struct Version {
  major: u32,
  minor: u32,
  patch: u32,
}
impl Version {
  pub fn new(major: u32, minor: u32, patch: u32) -> Self {
    Self {
      major,
      minor,
      patch,
    }
  }
}
impl Default for Version {
  fn default() -> Self {
    Self {
      major: 0,
      minor: 1,
      patch: 0,
    }
  }
}

// ================================================================================
//  ApplicationDetails Struct
// ================================================================================
/// Application Details and version for your application.
pub struct ApplicationDetails<'a> {
  name: &'a str,
  version: Version,
}
impl<'a> ApplicationDetails<'a> {
  pub fn new(name: &'a str, version: Version) -> Self {
    Self { name, version }
  }

  /// Get Major Minor Patch in a single u32.
  fn get_u32_version(self) -> u32 {
    ash::vk::make_version(self.version.major, self.version.minor, self.version.patch)
  }
}
impl<'a> Default for ApplicationDetails<'a> {
  fn default() -> Self {
    Self {
      name: "Nameless Application",
      version: Version::new(0, 1, 0),
    }
  }
}

// ================================================================================
//  EngineDetails Struct
// ================================================================================
/// Application Details and version for your engine.
pub struct EngineDetails<'a> {
  name: &'a str,
  version: Version,
}
impl<'a> EngineDetails<'a> {
  pub fn new(name: &'a str, version: Version) -> Self {
    Self { name, version }
  }

  /// Get Major Minor Patch in a single u32.
  fn get_u32_version(self) -> u32 {
    ash::vk::make_version(self.version.major, self.version.minor, self.version.patch)
  }
}
impl<'a> Default for EngineDetails<'a> {
  fn default() -> Self {
    Self {
      name: "Nameless Engine",
      version: Version::new(0, 1, 0),
    }
  }
}

// ================================================================================
//  Renderer Trait
// ================================================================================
/// This is the trait interface that every backend supports.
pub trait Renderer {
  fn load_shader(
    &mut self, spirv: &ShaderCode, shader_type: ShaderType,
  ) -> SarektResult<ShaderHandle>;
  fn destroy_shader(&mut self, handle: ShaderHandle) -> SarektResult<()>;
}

enum RendererBackend {
  Vulkan,
}

// TODO for resources. user application in charge of loading, once they are
// given to the renderer they are loaded into GPU memory and a handle is
// returned.
//
// These handles are implemented as enums with a type for each backend
// containing its actual handle.
//
// This includes handles to uniforms (backed by whatever the shader said to be)
// (if other backends implemented shader_cross has to get them ready to be
// for that shader type and use them how we like).
//
// These handles could be used to change changeable values (eg uniforms).
//
// These objects are used to insert an object into the scene, which generates
// the appropriate pipline (potentially the pipeline can be generated even
// without full insertion), commands to draw it, etc.
//
// Uniforms (and maybe SSBOs along with them) specifically in vulkan would be an
// enum of uniform/ssbo type WITHIN the SarektUniformHandle.  Then i can match
// and do the write command buffer strategy to draw them.

// TODO some kind of factory for backends?
