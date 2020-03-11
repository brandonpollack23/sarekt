use crate::{
  error::{SarektError, SarektResult},
  renderer::shaders::{ShaderBackendHandleTrait, ShaderCode, ShaderLoader},
};
use ash::{version::DeviceV1_0, vk, Device};
use log::info;
use std::sync::Arc;

/// Vulkan implementation of [ShaderLoader](trait.ShaderLoader.html).
#[derive(Clone)]
pub struct VulkanShaderFunctions {
  logical_device: Arc<Device>,
}
impl VulkanShaderFunctions {
  pub fn new(logical_device: Arc<Device>) -> Self {
    Self { logical_device }
  }
}
unsafe impl ShaderLoader for VulkanShaderFunctions {
  type SBH = vk::ShaderModule;

  fn load_shader(&self, code: &ShaderCode) -> SarektResult<vk::ShaderModule> {
    if let ShaderCode::Spirv(spirv) = code {
      let ci = vk::ShaderModuleCreateInfo::builder().code(spirv).build();
      unsafe {
        return Ok(self.logical_device.create_shader_module(&ci, None)?);
      }
    }

    Err(SarektError::IncompatibleShaderCode)
  }

  fn delete_shader(&self, shader: vk::ShaderModule) -> SarektResult<()> {
    info!("Deleting shader {:?}...", shader);
    unsafe { self.logical_device.destroy_shader_module(shader, None) };
    Ok(())
  }
}

/// Allow vk::ShaderModule to be a backend handle for the
/// [ShaderStore](struct.ShaderStore.html).
unsafe impl ShaderBackendHandleTrait for vk::ShaderModule {}
