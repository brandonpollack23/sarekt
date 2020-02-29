use crate::error::{SarektError, SarektResult};
use log::{error, info};
use slotmap::{DefaultKey, SlotMap};

/// A storage for all shaders to be loaded or destroyed from.  Returns a handle
/// that can be used to retrieve the associated shader, which includes it's type
/// and it's handle to whichever backend you're using.
pub(crate) struct ShaderStore<SL>
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
  /// Create with a group of methods to load/destroy shaders.
  pub fn new(shader_loader: SL) -> Self {
    Self {
      loaded_shaders: SlotMap::new(),
      shader_loader,
    }
  }

  /// Load a shader into the driver and return a handle.
  pub fn load_shader(
    &mut self, code: &ShaderCode, shader_type: ShaderType,
  ) -> SarektResult<ShaderHandle> {
    let shader_backend_handle = self.shader_loader.load_shader(code)?;

    let inner_handle = self
      .loaded_shaders
      .insert(Shader::new(shader_backend_handle, shader_type));

    Ok(ShaderHandle(inner_handle))
  }

  /// Using the handle, destroy the shader from the backend.
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

  /// Retrieve a loaded shader to be used in pipeline construction, etc.
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
    info!("Destroying all shaders...");
    for shader in self.loaded_shaders.iter() {
      let result = self.shader_loader.delete_shader(shader.1.shader_handle);
      if result.is_err() {
        error!("Error destroying shader");
      }
    }
  }
}

/// A newtype to keep teh keys for the shader store type safe.
pub struct ShaderHandle(DefaultKey);

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

/// The backing type of the shader, for vulkan this is spirv, gl just uses glsl,
/// D3D hlsl, etc.
pub enum ShaderCode<'a> {
  Spirv(&'a [u32]),
  Glsl(&'a str),
}

/// The shader in it's backend type along with the type of shader itself (vertex
/// etc).
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

/// The type of shader (vertex, fragment, etc).
#[derive(Copy, Clone)]
pub enum ShaderType {
  Vertex,
  Fragment,
  Geometry,
  Tesselation,
  Compute,
}
