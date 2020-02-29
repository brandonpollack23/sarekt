use crate::{
  error::{SarektError, SarektError::VulkanError, SarektResult},
  renderer::shaders::{ShaderBackendHandle, ShaderCode, ShaderHandle, ShaderLoader, ShaderType},
};
use ash::{version::DeviceV1_0, vk, Device};
use vk_shader_macros::include_glsl;

pub const DEFAULT_VERTEX_SHADER: &[u32] = include_glsl!("shaders/no_buffer_triangle.vert");
pub const DEFAULT_FRAGMENT_SHADER: &[u32] = include_glsl!("shaders/no_buffer_triangle.frag");

#[derive(Copy, Clone)]
pub struct VulkanShaderFunctions {
  device_handle: vk::Device,
  load_shader_fn: vk::PFN_vkCreateShaderModule,
  destroy_shader_fn: vk::PFN_vkDestroyShaderModule,
}
impl VulkanShaderFunctions {
  pub fn new(logical_device: &Device) -> Self {
    Self {
      device_handle: logical_device.handle(),
      load_shader_fn: logical_device.fp_v1_0().create_shader_module,
      destroy_shader_fn: logical_device.fp_v1_0().destroy_shader_module,
    }
  }
}
unsafe impl ShaderLoader for VulkanShaderFunctions {
  type SBH = vk::ShaderModule;

  fn load_shader(&mut self, code: &ShaderCode) -> SarektResult<vk::ShaderModule> {
    let load_shader_fn = self.load_shader_fn;
    if let ShaderCode::Spirv(spirv) = code {
      let ci = vk::ShaderModuleCreateInfo::builder().code(spirv).build();
      unsafe {
        let mut shader_module = std::mem::zeroed();
        let err_code = load_shader_fn(
          self.device_handle,
          &ci,
          std::ptr::null(),
          &mut shader_module,
        );
        return match err_code {
          vk::Result::SUCCESS => Ok(shader_module),
          _ => Err(VulkanError(err_code)),
        };
      }
    }

    Err(SarektError::IncompatibleShaderCode)
  }

  fn delete_shader(&mut self, shader: vk::ShaderModule) -> SarektResult<()> {
    unsafe {
      let destroy_shader_fn = &self.destroy_shader_fn;
      destroy_shader_fn(self.device_handle, shader, std::ptr::null());
    }
    Ok(())
  }
}

unsafe impl ShaderBackendHandle for vk::ShaderModule {}

// TODO use these
// pub fn load_shader(code: &ShaderCode, shader_type: ShaderType) ->
// SarektResult<ShaderHandle> {   let ci =
// vk::ShaderModuleCreateInfo::builder().code(spirv).build();
//
//   let inner_handle = self
//     .loaded_shaders
//     .insert(Shader::new(shader_module, shader_type));
//
//   Ok(ShaderHandle(inner_handle))
// }
// pub fn destroy_shader(
//   &mut self, logical_device: &Device, handle: ShaderHandle,
// ) -> SarektResult<()> {
//   let shader = self.loaded_shaders.remove(handle.0);
//   if shader.is_none() {
//     return Err(SarektError::UnknownShader);
//   }
//
//   unsafe {
// logical_device.destroy_shader_module(shader.unwrap().shader_handle, None) };
//   Ok(())
// }
