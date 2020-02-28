use crate::{
  error::{SarektError, SarektResult},
  renderer::RendererBackend,
};
use ash::{version::DeviceV1_0, vk, Device};
use log::info;
use slotmap::{DefaultKey, SlotMap};
use static_assertions::_core::ops::Deref;

// TODO if adding more backends move to outer module and make generic.

pub struct ShaderStore {
  loaded_shaders: SlotMap<DefaultKey, Shader>,
  device_handle: vk::Device,
  destroy_shader_fn: vk::PFN_vkDestroyShaderModule,
}
impl ShaderStore {
  pub unsafe fn new(logical_device: &Device) -> Self {
    Self {
      loaded_shaders: SlotMap::new(),
      device_handle: logical_device.handle(),
      destroy_shader_fn: logical_device.fp_v1_0().destroy_shader_module,
    }
  }

  pub fn load_shader(
    &mut self, logical_device: &Device, spirv: &[u32], shader_type: ShaderType,
  ) -> SarektResult<ShaderHandle> {
    let ci = vk::ShaderModuleCreateInfo::builder().code(spirv).build();
    let shader_module = unsafe { logical_device.create_shader_module(&ci, None)? };

    let inner_handle = self
      .loaded_shaders
      .insert(Shader::new(shader_module, shader_type));

    Ok(ShaderHandle(inner_handle))
  }

  pub fn destroy_shader(
    &mut self, logical_device: &Device, handle: DefaultKey,
  ) -> SarektResult<()> {
    let shader = self.loaded_shaders.remove(handle);
    if shader.is_none() {
      return Err(SarektError::UnknownShader);
    }

    unsafe { logical_device.destroy_shader_module(shader.unwrap().shader_module, None) };
    Ok(())
  }

  pub fn get(&self, handle: &ShaderHandle) -> SarektResult<&Shader> {
    let shader = self.loaded_shaders.get(handle.0);
    if shader.is_none() {
      return Err(SarektError::UnknownShader);
    }
    Ok(shader.unwrap())
  }
}
impl Drop for ShaderStore {
  fn drop(&mut self) {
    unsafe {
      info!("Destrying all shaders...");
      for shader in self.loaded_shaders.iter() {
        let destroy_shader_fn = &self.destroy_shader_fn;
        destroy_shader_fn(self.device_handle, shader.1.shader_module, std::ptr::null());
      }
    }
  }
}

pub struct ShaderHandle(DefaultKey);

#[derive(Copy, Clone)]
struct Shader {
  pub shader_module: vk::ShaderModule,
  pub shader_type: ShaderType,
}
impl Shader {
  fn new(shader_module: vk::ShaderModule, shader_type: ShaderType) -> Self {
    Self {
      shader_module,
      shader_type,
    }
  }
}
#[derive(Copy, Clone)]
enum ShaderType {
  Vertex,
  Fragment,
  Geometry,
  Tesselation,
  Compute,
}
