use crate::renderer::{vulkan::vulkan_shader_functions::VulkanShaderFunctions, ShaderHandle};

pub mod base_pipeline_bundle;
pub mod debug_utils_ext;
pub mod draw_synchronization;
pub mod images;
pub mod queues;
pub mod surface;
pub mod swap_chain;
pub mod vulkan_renderer;
pub mod vulkan_shader_functions;
pub mod vulkan_vertex_bindings;

pub type VulkanShaderHandle = ShaderHandle<VulkanShaderFunctions>;
