use crate::error::SarektResult;
use ultraviolet as uv;

/// A trait that provides a static function that generates backend specific
/// vertex bindings.  This is mainly provided out of convenience and would need
/// to be custom defined for each backend otherwise.  It is possible to seperate
/// them and provide a generic implementation for get_binding_descriptions if
/// you're only using one binding.
///
/// BVB is backend specific vertex bindings object.
/// BVA is backend specific vertex attributes object.
///
/// Unsafe because one must only bring one in scope and understand how to define
/// vertex bindings for the specific backend for Sarekt, which requires
/// understanding how the layouts and bindings are laid out in the shaders, or
/// creating ones own shaders and understanding layouts and bindings in them for
/// their backend.
pub unsafe trait VertexBindings {
  type BVA;
  type BVB;

  /// Return binding descriptions for the implemented type in the specific
  /// backend format. Bindings are bound during commands or command buffers
  /// and attach texture/image buffers to a binding location in the shader.
  fn get_binding_description() -> Self::BVB;

  /// Same as get_binding_description but for vertex attribute descriptions.
  fn get_attribute_descriptions() -> Vec<Self::BVA>;
}

/// Input vertices to the sarekt_forward shader set.
#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct DefaultForwardShaderVertex {
  pub position: uv::Vec3,
  pub color: uv::Vec3,
  pub texture_coordinates: uv::Vec2,
}
impl DefaultForwardShaderVertex {
  /// For use when there is no intended texture use.
  pub fn without_uv(pos: &[f32; 3], color: &[f32; 3]) -> Self {
    Self::new(pos, color, &[0.0f32, 0.0f32])
  }

  pub fn new(pos: &[f32; 3], color: &[f32; 3], texture_coordinates: &[f32; 2]) -> Self {
    Self {
      position: uv::Vec3::from(pos),
      color: uv::Vec3::from(color),
      texture_coordinates: uv::Vec2::from(texture_coordinates),
    }
  }

  // TODO ASSETS asset based creation (OBJ)
  // Do it in its own class that builds this and then loads it in without keeping
  // it in memory, or optionally returns a handle and the in memory handle.
}

/// Returns the descriptor layouts for the specific backend.  These contain
/// information such as which bindings to attach each part of uniform to in the
/// shader, which stages they are used, etc.
pub unsafe trait DescriptorLayoutInfo {
  type BackendDescriptorSetLayoutBindings;

  /// Gets the information for the shaders/pipeline in the backend for how to
  /// bind descriptors to them.
  fn get_descriptor_set_layout_bindings() -> Self::BackendDescriptorSetLayoutBindings;

  /// Gets the information needed in order to allocate/bind descriptors in the
  /// backend for uniforms.
  fn get_bind_uniform_info() -> SarektResult<BindUniformInfo>;

  /// Gets the information needed to allocate/bind descroptors in teh backend
  /// for textures.
  fn get_bind_texture_info() -> SarektResult<BindTextureInfo>;
}
#[derive(Clone, Debug)]
/// Contains information needed by various backends to configure their
/// descriptors for uniforms.
pub struct BindUniformInfo {
  pub offset: u64,
  pub range: u64,
  pub bindings: Vec<u32>,
}

/// Information needed by backend to bind textures.
pub struct BindTextureInfo {
  pub bindings: Vec<u32>,
}

/// Input uniforms to the sarekt_forward shader set.
///
/// Note that ***alignment matters*** Please see the specification for your
/// specific backend.
///
/// The initial backend is Vulkan, which is why booleans are u32, all scalars
/// need to be aligned to 4 bytes.
#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct DefaultForwardShaderLayout {
  /// The model view projection matrix to apply to the containing
  /// DrawableObject.
  pub mvp: uv::Mat4,
  pub enable_color_mixing: u32,
  pub enable_texture_mixing: u32,
}
impl DefaultForwardShaderLayout {
  pub fn new(mvp: uv::Mat4, enable_color_mixing: bool, enable_texture_mixing: bool) -> Self {
    Self {
      mvp,
      enable_color_mixing: u32::from(enable_color_mixing),
      enable_texture_mixing: u32::from(enable_texture_mixing),
    }
  }
}
impl Default for DefaultForwardShaderLayout {
  fn default() -> Self {
    DefaultForwardShaderLayout {
      mvp: uv::Mat4::identity(),
      enable_color_mixing: 0u32,
      enable_texture_mixing: 1u32,
    }
  }
}
