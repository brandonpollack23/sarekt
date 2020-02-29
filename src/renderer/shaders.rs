use crate::{
  error::{SarektError, SarektResult},
  renderer::RendererBackend,
};
use log::info;
use slotmap::{DefaultKey, SlotMap};
use static_assertions::_core::ops::Deref;

// TODO if adding more backends move to outer module and make generic.

pub struct ShaderStore<SL>
where
  SL: ShaderLoader,
  SL::SBH: ShaderBackendHandle + Copy,
{
  loaded_shaders: SlotMap<DefaultKey, Shader<SL::SBH>>,
  shader_loader: SL,
}
impl<SL> ShaderStore<SL>
where
  SL: ShaderLoader,
  SL::SBH: ShaderBackendHandle + Copy,
{
  pub fn new(shader_loader: SL) -> Self {
    Self {
      loaded_shaders: SlotMap::new(),
      shader_loader,
    }
  }

  pub fn load_shader(
    &mut self, code: &ShaderCode, shader_type: ShaderType,
  ) -> SarektResult<ShaderHandle> {
    let shader_backend_handle = self.shader_loader.load_shader(code)?;

    let inner_handle = self
      .loaded_shaders
      .insert(Shader::new(shader_backend_handle, shader_type));

    Ok(ShaderHandle(inner_handle))
  }

  pub fn destroy_shader(&mut self, handle: ShaderHandle) -> SarektResult<()> {
    let shader = self.loaded_shaders.remove(handle.0);
    if shader.is_none() {
      return Err(SarektError::UnknownShader);
    }
    self
      .shader_loader
      .delete_shader(shader.unwrap().shader_handle)?;
    Ok(())
  }

  pub fn get(&self, handle: &ShaderHandle) -> SarektResult<&Shader<SL::SBH>> {
    let shader = self.loaded_shaders.get(handle.0);
    if shader.is_none() {
      return Err(SarektError::UnknownShader);
    }
    Ok(shader.unwrap())
  }
}
impl<SL> Drop for ShaderStore<SL>
where
  SL: ShaderLoader,
  SL::SBH: ShaderBackendHandle + Copy,
{
  fn drop(&mut self) {
    unsafe {
      info!("Destroying all shaders...");
      for shader in self.loaded_shaders.iter() {
        self.shader_loader.delete_shader(shader.1.shader_handle);
      }
    }
  }
}

pub struct ShaderHandle(DefaultKey);

#[derive(Copy, Clone)]
pub struct Shader<SBH: ShaderBackendHandle + Copy> {
  pub shader_handle: SBH,
  pub shader_type: ShaderType,
}
impl<SBH> Shader<SBH>
where
  SBH: ShaderBackendHandle + Copy,
{
  fn new(shader_module: SBH, shader_type: ShaderType) -> Self {
    Self {
      shader_handle: shader_module,
      shader_type,
    }
  }
}
#[derive(Copy, Clone)]
pub enum ShaderType {
  Vertex,
  Fragment,
  Geometry,
  Tesselation,
  Compute,
}

/// The backing type of the shader, for vulkan this is spirv, gl just uses glsl,
/// D3D hlsl, etc.
pub enum ShaderCode<'a> {
  Spirv(&'a [u32]),
}

/// A marker to note that the type used is a Shader backend handle (eg
/// vkShaderModule for Vulkan).
///
/// Unsafe because:
/// This must specifically be the handle used to delete your
/// shader in the driver in [ShaderLoader](trait.ShaderLoader.html).
pub unsafe trait ShaderBackendHandle {}

/// A trait used by each implementation in order to load shaders in their own
/// way.
///
/// Unsafe because:
/// * The lifetimes of the functions to create them (which are
/// usually dynamically loaded) must outlive the Loader itself.
///
/// * SBH must be an implementer of
///   [ShaderBackendHandle](trait.ShaderBackendHandle.html)
pub unsafe trait ShaderLoader {
  type SBH;
  /// Loads the shader using underlying mechanism.
  fn load_shader(&mut self, code: &ShaderCode) -> SarektResult<Self::SBH>;
  /// Deletes the shader using underlying mechanism.
  fn delete_shader(&mut self, shader: Self::SBH) -> SarektResult<()>;
}
