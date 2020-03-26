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
//! Also see the file in project root.
pub mod buffers_and_images;
pub mod drawable_object;
pub mod shaders;
pub mod vertex_bindings;

mod vulkan;

pub use crate::{
  error::SarektResult,
  renderer::shaders::{ShaderBackendHandleTrait, ShaderCode, ShaderLoader},
};
pub use shaders::{ShaderHandle, ShaderType};
pub use vulkan::{
  vulkan_buffer_image_functions::VulkanBufferFunctions, vulkan_renderer::VulkanRenderer,
};

use crate::{
  image_data::ImageData,
  renderer::{
    buffers_and_images::{
      BackendHandleTrait, BufferAndImageLoader, BufferImageHandle, BufferType,
      MagnificationMinificationFilter, TextureAddressMode, UniformBufferHandle,
    },
    drawable_object::DrawableObject,
    vertex_bindings::DescriptorLayoutInfo,
  },
};
use std::fmt::Debug;

// ================================================================================
//  Compile Time Constants and Configurations
// ================================================================================
#[cfg(debug_assertions)]
const IS_DEBUG_MODE: bool = true;
#[cfg(not(debug_assertions))]
const IS_DEBUG_MODE: bool = false;
const ENABLE_VALIDATION_LAYERS: bool = IS_DEBUG_MODE;

// Wanna know more about what number is good here? [readme](https://software.intel.com/en-us/articles/practical-approach-to-vulkan-part-1)
const MAX_FRAMES_IN_FLIGHT: usize = 2;

// ================================================================================
//  Renderer Trait
// ================================================================================
/// This is the trait interface that every backend supports.  Used to create
/// [drawers](trait.Drawer.html) for use in other threads (to build up command
/// buffers in parallel), finalize the frame, etc.
///
/// SL is the [Shader Loader](trait.ShaderLoader.html) for the backing renderer.
/// BL is the [Buffer Loader](trait.BufferLoader.html) for the backing renderer.
pub trait Renderer {
  type BL;
  type SL;

  // TODO MULTITHREADING should load/get/update functions be part of drawer so
  // anyone can do it (within their own pools/queues)

  /// Enables or disables rendering.
  fn set_rendering_enabled(&mut self, enabled: bool);

  /// Mark this frame as complete and render it to the target of the renderer
  /// when ready.
  fn frame(&self) -> SarektResult<()>;

  // TODO PIPELINE create a new pipeline type out of shaders, render pass, etc.

  // TODO SHADER get_shader with handle
  // TODO SHADER when loading a shader, use spirv-reflect to make sure you don't
  // exceed max bound descriptors to use it.
  /// Loads a shader and returns a RAII handle to be used for retrieval or
  /// pipeline creation.
  fn load_shader(
    &mut self, spirv: &ShaderCode, shader_type: ShaderType,
  ) -> SarektResult<ShaderHandle<Self::SL>>
  where
    Self::SL: ShaderLoader,
    <Self::SL as ShaderLoader>::SBH: ShaderBackendHandleTrait + Copy + Debug;

  /// Loads a buffer and returns a RAII handle to be used for retrieval.
  fn load_buffer<BufElem: Sized + Copy>(
    &mut self, buffer_type: BufferType, buffer: &[BufElem],
  ) -> SarektResult<BufferImageHandle<Self::BL>>
  where
    Self::BL: BufferAndImageLoader,
    <Self::BL as BufferAndImageLoader>::BackendHandle: BackendHandleTrait + Copy + Debug;

  /// Gets a buffer given th handle generated when it was loaded (see
  /// load_buffer).
  fn get_buffer(
    &self, handle: &BufferImageHandle<Self::BL>,
  ) -> SarektResult<<Self::BL as BufferAndImageLoader>::BackendHandle>
  where
    Self::BL: BufferAndImageLoader,
    <Self::BL as BufferAndImageLoader>::BackendHandle: BackendHandleTrait + Copy + Debug;

  /// Loads a uniform buffer.
  fn load_uniform_buffer<UniformBufElem: Sized + Copy>(
    &mut self, buffer: UniformBufElem,
  ) -> SarektResult<UniformBufferHandle<Self::BL, UniformBufElem>>
  where
    Self::BL: BufferAndImageLoader,
    <Self::BL as BufferAndImageLoader>::BackendHandle: BackendHandleTrait + Copy + Debug;

  /// Returns a uniform buffer given the handle returned in load_uniform_buffer.
  fn get_uniform_buffer<UniformBufElem: Sized + Copy>(
    &self, handle: &UniformBufferHandle<Self::BL, UniformBufElem>,
  ) -> SarektResult<<Self::BL as BufferAndImageLoader>::UniformBufferDataHandle>
  where
    Self::BL: BufferAndImageLoader;

  /// Updates the uniform buffer's contained value.
  fn set_uniform<BufElem: Sized + Copy>(
    &self, handle_data: &<Self::BL as BufferAndImageLoader>::UniformBufferDataHandle,
    data: &BufElem,
  ) -> SarektResult<()>
  where
    Self::BL: BufferAndImageLoader;

  /// Loads a 32 bit r8b8g8a8 image (texture) into the renderer using a staging
  /// buffer. [ImageData](trait.ImageData.html) must be implemented for the
  /// type, see its documentation for details.
  fn load_image_with_staging_rgba_32(
    &mut self, pixels: impl ImageData, magnification_filter: MagnificationMinificationFilter,
    minification_filter: MagnificationMinificationFilter, address_x: TextureAddressMode,
    address_y: TextureAddressMode, address_z: TextureAddressMode,
  ) -> SarektResult<BufferImageHandle<Self::BL>>
  where
    Self::BL: BufferAndImageLoader,
    <Self::BL as BufferAndImageLoader>::BackendHandle: BackendHandleTrait + Copy + Debug;

  /// Retrieves an image using the handle returned by the `load_image_*` family
  /// of functions.
  fn get_image(
    &self, handle: &BufferImageHandle<Self::BL>,
  ) -> SarektResult<<Self::BL as BufferAndImageLoader>::BackendHandle>
  where
    Self::BL: BufferAndImageLoader,
    <Self::BL as BufferAndImageLoader>::BackendHandle: BackendHandleTrait + Copy + Debug;

  /// Handle swapchain out of date, such as window changes.
  fn recreate_swapchain(&mut self, width: u32, height: u32) -> SarektResult<()>;
}

/// Trait that each renderer as well as its secondary drawers (if supported)
/// implement for multi-threading purposes.
pub trait Drawer {
  type R;

  fn draw<UniformBufElem>(
    &self, object: &DrawableObject<Self::R, UniformBufElem>,
  ) -> SarektResult<()>
  where
    UniformBufElem: Sized + Copy + DescriptorLayoutInfo,
    Self::R: Renderer,
    <Self::R as Renderer>::BL: BufferAndImageLoader,
    <<Self::R as Renderer>::BL as BufferAndImageLoader>::BackendHandle:
      BackendHandleTrait + Copy + Debug;

  // TODO PIPELINE select render pass (predefined set?) log when pipeline not
  // compatible and dont draw? End previous render pass and keep track of last
  // render pass to end it as well.
}

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
