use crate::error::{SarektError, SarektResult};

use log::warn;
use slotmap::{DefaultKey, SlotMap};
use std::{
  fmt::Debug,
  sync::{Arc, RwLock},
};

/// A type that can be used to retrieve a shader from the renderer and
/// ShaderStore that will destroy the shader when it goes out of scope.
///
/// As always, In order to pass this around with multiple ownership, wrap it in
/// an Arc.
#[derive(Clone)]
pub struct ShaderHandle<SL>
where
  SL: ShaderLoader,
  SL::SBH: ShaderBackendHandleTrait + Copy + Debug,
{
  inner_key: DefaultKey,
  shader_store: Arc<RwLock<ShaderStore<SL>>>,
}
impl<SL> Drop for ShaderHandle<SL>
where
  SL: ShaderLoader,
  SL::SBH: ShaderBackendHandleTrait + Copy + Debug,
{
  fn drop(&mut self) {
    let mut shader_store_guard = self
      .shader_store
      .write()
      .expect("Could not unlock ShaderStore due to previous panic");
    match shader_store_guard.destroy_shader(self.inner_key) {
      // Already deleted, likely shutting down. Nothing to do.
      Err(SarektError::UnknownShader) => {}
      Err(e) => warn!("shader not destroyed, maybe it was already? Error: {:?}", e),
      Ok(()) => {}
    }
  }
}

/// The backing type of the shader, for vulkan this is spirv, gl just uses glsl,
/// D3D hlsl, etc.
pub enum ShaderCode<'a> {
  Spirv(&'a [u32]),
  Glsl(&'a str), // TODO COMPAT support GLSL
}

/// The type of shader (vertex, fragment, etc).
#[derive(Copy, Clone, Debug)]
pub enum ShaderType {
  Vertex,
  Fragment,
  Geometry,
  Tesselation,
  Compute,
}

/// A marker to note that the type used is a Shader backend handle (eg
/// vkShaderModule for Vulkan).
///
/// Unsafe because:
/// This must specifically be the handle used to delete your
/// shader in the driver in [ShaderLoader](trait.ShaderLoader.html).
pub unsafe trait ShaderBackendHandleTrait: Copy {}

/// A trait used by each implementation in order to load shaders in their own
/// way.
///
/// Unsafe because:
/// * The lifetimes of the functions to create them (which are
/// usually dynamically loaded) must outlive the Loader itself.
///
/// * SBH must be an implementer of
///   [ShaderBackendHandle](trait.ShaderBackendHandle.html)
///
///  * It is the responsibility of the implementor to drop anything loaded using
///    delete_shader cleanly on all elements, if the ShaderHandle dropping
///    doesn't handle it.
pub unsafe trait ShaderLoader {
  type SBH;
  /// Loads the shader using underlying mechanism.
  fn load_shader(&self, code: &ShaderCode) -> SarektResult<Self::SBH>;
  /// Deletes the shader using underlying mechanism.
  fn delete_shader(&self, shader: Self::SBH) -> SarektResult<()>;
}

/// A storage for all shaders to be loaded or destroyed from.  Returns a handle
/// that can be used to retrieve the associated shader, which includes it's type
/// and it's handle to whichever backend you're using.
pub struct ShaderStore<SL>
where
  SL: ShaderLoader,
  SL::SBH: ShaderBackendHandleTrait + Copy + Debug,
{
  loaded_shaders: SlotMap<DefaultKey, Shader<SL::SBH>>,
  shader_loader: SL,
}

impl<SL> ShaderStore<SL>
where
  SL: ShaderLoader,
  SL::SBH: ShaderBackendHandleTrait + Copy + Debug,
{
  /// Create with a group of methods to load/destroy shaders.
  pub(crate) fn new(shader_loader: SL) -> Self {
    Self {
      loaded_shaders: SlotMap::new(),
      shader_loader,
    }
  }

  /// Load a shader into the driver and return a handle.
  pub(crate) fn load_shader(
    this: &Arc<RwLock<Self>>, code: &ShaderCode, shader_type: ShaderType,
  ) -> SarektResult<ShaderHandle<SL>> {
    let mut shader_store = this
      .write()
      .expect("Could not unlock ShaderStore due to previous panic");

    let shader_backend_handle = shader_store.shader_loader.load_shader(code)?;
    let inner_key = shader_store
      .loaded_shaders
      .insert(Shader::new(shader_backend_handle, shader_type));

    Ok(ShaderHandle {
      inner_key,
      shader_store: this.clone(),
    })
  }

  /// Using the handle, destroy the shader from the backend.
  fn destroy_shader(&mut self, inner_key: DefaultKey) -> SarektResult<()> {
    let shader = self.loaded_shaders.remove(inner_key);
    if shader.is_none() {
      return Err(SarektError::UnknownShader);
    }
    self
      .shader_loader
      .delete_shader(shader.unwrap().shader_handle)?;
    Ok(())
  }

  /// Destroys all the shaders.  Unsafe because any outstanding handles will not
  /// result in errors when they drop, so they must be forgotten.
  pub(crate) unsafe fn destroy_all_shaders(&mut self) {
    for shader in self.loaded_shaders.iter() {
      if let Err(err) = self.shader_loader.delete_shader(shader.1.shader_handle) {
        warn!(
          "Shader not destroyed, maybe it was already? Error: {:?}",
          err
        );
      }
    }

    self.loaded_shaders.clear();
  }

  /// Retrieve a loaded shader to be used in pipeline construction, etc.
  pub(crate) fn get_shader(&self, handle: &ShaderHandle<SL>) -> SarektResult<&Shader<SL::SBH>> {
    let shader = self.loaded_shaders.get(handle.inner_key);
    if let Some(shader) = shader {
      return Ok(shader);
    }
    Err(SarektError::UnknownShader)
  }
}

/// The shader in it's backend type along with the type of shader itself (vertex
/// etc).
#[derive(Copy, Clone, Debug)]
pub(crate) struct Shader<SBH: ShaderBackendHandleTrait + Copy> {
  pub shader_handle: SBH,
  pub shader_type: ShaderType,
}

impl<SBH> Shader<SBH>
where
  SBH: ShaderBackendHandleTrait + Copy,
{
  fn new(shader_module: SBH, shader_type: ShaderType) -> Self {
    Self {
      shader_handle: shader_module,
      shader_type,
    }
  }
}
