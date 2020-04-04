use crate::renderer::{vulkan::vulkan_shader_functions::VulkanShaderFunctions, ShaderHandle};

pub mod images;
pub mod queues;
pub mod vulkan_buffer_image_functions;
pub mod vulkan_renderer;
pub mod vulkan_shader_functions;
pub mod vulkan_vertex_bindings;

pub type VulkanShaderHandle = ShaderHandle<VulkanShaderFunctions>;
