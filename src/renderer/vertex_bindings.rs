use nalgebra as na;

// TODO critical move out of renderer along with different backend files into
// their own folder.

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
  pub position: na::Vector2<f32>,
  pub color: na::Vector3<f32>,
}
impl DefaultForwardShaderVertex {
  pub fn new(pos: &[f32; 2], color: &[f32; 3]) -> Self {
    Self {
      position: na::Vector2::from_row_slice(pos),
      color: na::Vector3::from_row_slice(color),
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

  fn get_descriptor_set_layout_bindings() -> Self::BackendDescriptorSetLayoutBindings;
}

/// Input uniforms to the sarekt_forward shader set.
#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct DefaultForwardShaderUniforms {
  /// The model view projection matrix to apply to the containing
  /// DrawableObject.
  pub mvp: na::Matrix4<f32>,
}
