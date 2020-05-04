use crate::{
  error::SarektResult,
  renderer::{
    buffers_and_images::BufferImageStore,
    config::{AntiAliasingConfig, Config, NumSamples},
    shaders::ShaderStore,
    vertex_bindings::{
      DefaultForwardShaderLayout, DefaultForwardShaderVertex, DescriptorLayoutInfo, VertexBindings,
    },
    vulkan::{
      images::ImageAndView,
      vulkan_renderer::{
        base_pipeline_bundle::{BasePipelineBundle, MsaaColorImage},
        depth_buffer::DepthResources,
        render_targets::RenderTargetBundle,
        vulkan_core::{VulkanCoreStructures, VulkanDeviceStructures},
        DEFAULT_FRAGMENT_SHADER, DEFAULT_VERTEX_SHADER,
      },
      vulkan_shader_functions::VulkanShaderFunctions,
      VulkanShaderHandle,
    },
    ShaderCode, ShaderHandle, ShaderType, VulkanBufferImageFunctions,
  },
};
use ash::{version::DeviceV1_0, vk, vk::DescriptorSetLayout, Device};
use log::info;
use std::{
  convert::TryInto,
  ffi::CStr,
  sync::{Arc, RwLock},
};

/// Pipeline related fields and methods, including forward render pass, base
/// pipeline, and fraembuffers.
pub struct Pipelines {
  pub framebuffers: Vec<vk::Framebuffer>,
  pub forward_render_pass: vk::RenderPass,
  base_graphics_pipeline_bundle: BasePipelineBundle,
}
impl Pipelines {
  pub fn new(
    config: &Config, vulkan_core: &VulkanCoreStructures, device_bundle: &VulkanDeviceStructures,
    render_target_bundle: &RenderTargetBundle,
    shader_store: &Arc<RwLock<ShaderStore<VulkanShaderFunctions>>>,
    buffer_image_store: &Arc<RwLock<BufferImageStore<VulkanBufferImageFunctions>>>,
  ) -> SarektResult<Pipelines> {
    let dimensions = (
      render_target_bundle.extent.width,
      render_target_bundle.extent.height,
    );
    let num_msaa_samples = if let AntiAliasingConfig::MSAA(ns) = config.aa_config {
      ns
    } else {
      NumSamples::One
    };

    // TODO(issue#2) RENDERING_CAPABILITIES support other render pass types.
    let depth_buffer = DepthResources::new(
      &vulkan_core.instance,
      device_bundle.physical_device,
      buffer_image_store,
      dimensions,
      num_msaa_samples,
    )?;

    let msaa_color_image = MsaaColorImage::new(
      buffer_image_store,
      dimensions,
      render_target_bundle
        .swapchain_and_extension
        .format
        .try_into()
        .expect("Format not supported by sarekt for msaa color buffer"),
      num_msaa_samples,
    )?;

    let forward_render_pass = Self::create_forward_render_pass(
      &device_bundle.logical_device,
      render_target_bundle.get_render_target_format(),
      &depth_buffer,
    )?;

    // TODO(issue#2) RENDERING_CAPABILITIES when I can have multiple render pass
    // types I need new framebuffers for each.
    let framebuffers = Self::create_framebuffers(
      &device_bundle.logical_device,
      forward_render_pass,
      &depth_buffer,
      &render_target_bundle.render_targets,
      render_target_bundle.extent,
    )?;

    let base_graphics_pipeline_bundle = Self::create_base_graphics_pipeline_and_shaders(
      &device_bundle.logical_device,
      &shader_store, // Unlock and get a local mut ref to shaderstore.
      render_target_bundle.extent,
      forward_render_pass,
      msaa_color_image,
      depth_buffer,
    )?;

    Ok(Pipelines {
      framebuffers,
      forward_render_pass,
      base_graphics_pipeline_bundle,
    })
  }

  /// Gets the currently active pipeline (just the base pipeline for now).
  pub fn get_current_pipeline(&self) -> vk::Pipeline {
    self.base_graphics_pipeline_bundle.pipeline
  }

  /// Returns the current pipeline layout (just base for now).
  pub fn get_pipeline_layout(&self) -> vk::PipelineLayout {
    self.base_graphics_pipeline_bundle.pipeline_layout
  }

  /// Returns the descriptor layouts of the current pipeline (just base for
  /// now).
  pub fn get_pipeline_descriptor_layouts(&self) -> Vec<vk::DescriptorSetLayout> {
    vec![
      self
        .base_graphics_pipeline_bundle
        .descriptor_set_layouts
        .as_ref()
        .unwrap()[0],
    ]
  }

  /// Returns the framebuffer of the given (swapchain) index.
  pub fn get_framebuffer(&self, image_index: usize) -> vk::Framebuffer {
    self.framebuffers[image_index]
  }

  /// Recreates all render passes associated with the pipeline for swapchain
  /// recreation.
  pub fn recreate_renderpasses(
    &mut self, logical_device: &Device, new_format: vk::Format,
  ) -> SarektResult<()> {
    self.forward_render_pass = Self::create_forward_render_pass(
      logical_device,
      new_format,
      self
        .base_graphics_pipeline_bundle
        .depth_resources
        .as_ref()
        .unwrap(),
    )?;
    Ok(())
  }

  /// Same as above, recreates vulkan framebuffers
  pub fn recreate_framebuffers(
    &mut self, logical_device: &Device, depth_buffer: &DepthResources,
    render_targets: &[ImageAndView], new_extent: vk::Extent2D,
  ) -> SarektResult<()> {
    self.framebuffers = Self::create_framebuffers(
      logical_device,
      self.forward_render_pass,
      &depth_buffer,
      render_targets,
      new_extent,
    )?;
    Ok(())
  }

  /// Same as above but recreate default pipeline and its bundled storages and
  /// handles.
  pub fn recreate_base_pipeline_bundle(
    &mut self, logical_device: &Device,
    shader_store: &Arc<RwLock<ShaderStore<VulkanShaderFunctions>>>, new_extent: vk::Extent2D,
    msaa_color_image: MsaaColorImage, depth_buffer: DepthResources,
    descriptor_set_layouts: Vec<DescriptorSetLayout>,
    vertex_shader_handle: ShaderHandle<VulkanShaderFunctions>,
    fragment_shader_handle: ShaderHandle<VulkanShaderFunctions>,
  ) -> SarektResult<()> {
    self.base_graphics_pipeline_bundle = Self::create_base_graphics_pipeline(
      logical_device,
      shader_store,
      new_extent,
      self.forward_render_pass,
      msaa_color_image,
      depth_buffer,
      descriptor_set_layouts,
      vertex_shader_handle,
      fragment_shader_handle,
    )?;
    Ok(())
  }

  // TODO(issue#2) PIPELINES handle when there is more than one pipeline.
  /// Save the handles to the base shaders so they don't have to be recreated
  /// for no reason during swapchain recreation.
  /// Returns vertex shader handle, fragment shader handle, and a the descriptor
  /// set layouts for the pipeline.
  pub fn take_shaders_and_layouts(
    &mut self,
  ) -> (
    Option<ShaderHandle<VulkanShaderFunctions>>,
    Option<ShaderHandle<VulkanShaderFunctions>>,
    Option<Vec<DescriptorSetLayout>>,
  ) {
    let vertex_shader_handle = self
      .base_graphics_pipeline_bundle
      .vertex_shader_handle
      .take();
    let fragment_shader_handle = self
      .base_graphics_pipeline_bundle
      .fragment_shader_handle
      .take();
    let descriptor_set_layouts = self
      .base_graphics_pipeline_bundle
      .descriptor_set_layouts
      .take();

    (
      vertex_shader_handle,
      fragment_shader_handle,
      descriptor_set_layouts,
    )
  }

  /// Must be called during renderer's drop.
  pub unsafe fn cleanup_descriptor_set_layouts(&mut self, logical_device: &Device) {
    info!("Destroying default descriptor set layouts for default pipeline...");
    if let Some(descriptor_set_layouts) = &self.base_graphics_pipeline_bundle.descriptor_set_layouts
    {
      for &layout in descriptor_set_layouts.iter() {
        logical_device.destroy_descriptor_set_layout(layout, None);
      }
    }
  }

  /// Cleans up all vulkan resources, unsafe because it should only be called
  /// when these resources are no longer needed.
  pub unsafe fn cleanup(&self, logical_device: &Device) {
    info!("Destroying all framebuffers...");
    for &fb in self.framebuffers.iter() {
      logical_device.destroy_framebuffer(fb, None);
    }

    info!("Destroying base graphics pipeline...");
    logical_device.destroy_pipeline(self.base_graphics_pipeline_bundle.pipeline, None);

    info!("Destroying base pipeline layouts...");
    logical_device
      .destroy_pipeline_layout(self.base_graphics_pipeline_bundle.pipeline_layout, None);

    info!("Destroying render pass...");
    logical_device.destroy_render_pass(self.forward_render_pass, None);
  }

  // ================================================================================
  //  Pipeline Helper Methods
  // ================================================================================
  /// Creates a simple forward render pass with one subpass.
  fn create_forward_render_pass(
    logical_device: &Device, format: vk::Format, depth_buffer: &DepthResources,
  ) -> SarektResult<vk::RenderPass> {
    // Used to reference attachments in render passes.
    let color_attachment = vk::AttachmentDescription::builder()
      .format(format)
      .samples(vk::SampleCountFlags::TYPE_1)
      .load_op(vk::AttachmentLoadOp::CLEAR) // Clear on loading the color attachment, since we're writing over it.
      .store_op(vk::AttachmentStoreOp::STORE) // Want to save to this attachment in the pass.
      .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE) // Not using stencil.
      .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE) // Not using stencil.
      .initial_layout(vk::ImageLayout::UNDEFINED) // Don't know the layout coming in.
      .final_layout(vk::ImageLayout::PRESENT_SRC_KHR) // TODO(issue#9) OFFSCREEN only do this if going to present. Otherwise TransferDST optimal would be good.
      .build();
    // Used to reference attachments in subpasses.
    let color_attachment_ref = vk::AttachmentReference::builder()
      .attachment(0u32) // Only using 1 (indexed from 0) attachment.
      .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL) // We're drawing color so optimize the pass to draw color to this attachment.
      .build();
    let color_attachment_refs = [color_attachment_ref];

    let depth_attachment = vk::AttachmentDescription::builder()
      .format(depth_buffer.format)
      .samples(vk::SampleCountFlags::TYPE_1)
      .load_op(vk::AttachmentLoadOp::CLEAR)
      .store_op(vk::AttachmentStoreOp::DONT_CARE)
      .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
      .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
      .initial_layout(vk::ImageLayout::UNDEFINED)
      .final_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
      .build();
    let depth_attachment_ref = vk::AttachmentReference::builder()
      .attachment(1)
      .layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
      .build();

    let attachments = [color_attachment, depth_attachment];

    // Subpasses could also reference previous subpasses as input, depth/stencil
    // data, or preserve attachments to send them to the next subpass.
    let subpass_description = vk::SubpassDescription::builder()
      .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS) // This is a graphics subpass
      // index of this attachment here is a reference to the output of the shader in the form of layout(location = 0).
      .color_attachments(&color_attachment_refs)
      .depth_stencil_attachment(&depth_attachment_ref)
      .build();
    let subpass_descriptions = [subpass_description];

    let dependency = vk::SubpassDependency::builder()
      .src_subpass(vk::SUBPASS_EXTERNAL)
      .dst_subpass(0u32)
      .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT) // We need to wait until the image is not in use (by the swapchain for example).
      .src_access_mask(vk::AccessFlags::empty()) // We're not going to access the swapchain as a source.
      .dst_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT) // Anyone waiting on this should wait in the color attachment stage.
      .dst_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_READ | vk::AccessFlags::COLOR_ATTACHMENT_WRITE) // Dependents should wait if they read or write the color attachment.
      .build();
    let dependencies = [dependency];

    let render_pass_ci = vk::RenderPassCreateInfo::builder()
      .attachments(&attachments)
      .subpasses(&subpass_descriptions) // Only one subpass in this case.
      .dependencies(&dependencies) // Only one dep.
      .build();

    Ok(unsafe { logical_device.create_render_pass(&render_pass_ci, None)? })
  }

  /// Creates the base pipeline for Sarekt.  A user can load custom shaders,
  /// etc, to create custom pipelines (passed back as opaque handles) based off
  /// this one that they can pass when requesting a draw.
  ///
  /// TODO(issue#2) RENDERING_CAPABILITIES allow for creating custom pipelines
  /// via LoadShaders etc.  When that is done, allow for disabling default
  /// pipeline creation via config if it wont be used to save resources.
  ///
  /// TODO(issue#17) RENDERING_CAPABILITIES enable pipeline cache.
  fn create_base_graphics_pipeline_and_shaders(
    logical_device: &Device, shader_store: &Arc<RwLock<ShaderStore<VulkanShaderFunctions>>>,
    extent: vk::Extent2D, render_pass: vk::RenderPass, msaa_color_image: MsaaColorImage,
    depth_buffer: DepthResources,
  ) -> SarektResult<BasePipelineBundle> {
    let (vertex_shader_handle, fragment_shader_handle) =
      Self::create_default_shaders(shader_store)?;

    let default_descriptor_set_layouts =
      Self::create_default_descriptor_set_layouts(logical_device)?;

    Self::create_base_graphics_pipeline(
      logical_device,
      shader_store,
      extent,
      render_pass,
      msaa_color_image,
      depth_buffer,
      default_descriptor_set_layouts,
      vertex_shader_handle,
      fragment_shader_handle,
    )
  }

  fn create_default_shaders(
    shader_store: &Arc<RwLock<ShaderStore<VulkanShaderFunctions>>>,
  ) -> SarektResult<(VulkanShaderHandle, VulkanShaderHandle)> {
    let vertex_shader_handle = ShaderStore::load_shader(
      shader_store,
      &ShaderCode::Spirv(DEFAULT_VERTEX_SHADER),
      ShaderType::Vertex,
    )?;
    let fragment_shader_handle = ShaderStore::load_shader(
      shader_store,
      &ShaderCode::Spirv(DEFAULT_FRAGMENT_SHADER),
      ShaderType::Vertex,
    )?;

    Ok((vertex_shader_handle, fragment_shader_handle))
  }

  fn create_default_descriptor_set_layouts(
    logical_device: &Device,
  ) -> SarektResult<Vec<vk::DescriptorSetLayout>> {
    // Create descriptor set layouts for the default forward shader uniforms.
    let descriptor_set_layout_bindings =
      DefaultForwardShaderLayout::get_descriptor_set_layout_bindings();
    let descriptor_set_layout_ci = vk::DescriptorSetLayoutCreateInfo::builder()
      .bindings(&descriptor_set_layout_bindings)
      .build();

    // One descriptor set layout contains all the bindings for the shader.
    // Pipline_layout_ci requires an array because of something I've yet to learn in
    // descriptor_pools and descriptor_sets.

    let descriptor_set_layout =
      unsafe { logical_device.create_descriptor_set_layout(&descriptor_set_layout_ci, None)? };
    Ok(vec![descriptor_set_layout])
  }

  fn create_base_graphics_pipeline(
    logical_device: &Device, shader_store: &Arc<RwLock<ShaderStore<VulkanShaderFunctions>>>,
    extent: vk::Extent2D, render_pass: vk::RenderPass, msaa_color_image: MsaaColorImage,
    depth_buffer: DepthResources, descriptor_set_layouts: Vec<vk::DescriptorSetLayout>,
    vertex_shader_handle: VulkanShaderHandle, fragment_shader_handle: VulkanShaderHandle,
  ) -> SarektResult<BasePipelineBundle> {
    let shader_store = shader_store.read().unwrap();

    let entry_point_name = CStr::from_bytes_with_nul(b"main\0").unwrap();
    let vert_shader_stage_ci = vk::PipelineShaderStageCreateInfo::builder()
      .stage(vk::ShaderStageFlags::VERTEX)
      .module(
        shader_store
          .get_shader(&vertex_shader_handle)
          .unwrap()
          .shader_handle,
      )
      .name(entry_point_name)
      .build();
    let frag_shader_stage_ci = vk::PipelineShaderStageCreateInfo::builder()
      .stage(vk::ShaderStageFlags::FRAGMENT)
      .module(
        shader_store
          .get_shader(&fragment_shader_handle)
          .unwrap()
          .shader_handle,
      )
      .name(entry_point_name)
      .build();

    let shader_stage_cis = [vert_shader_stage_ci, frag_shader_stage_ci];

    let binding_descs = [DefaultForwardShaderVertex::get_binding_description()];
    let attr_descs = DefaultForwardShaderVertex::get_attribute_descriptions();
    let vertex_input_ci = vk::PipelineVertexInputStateCreateInfo::builder()
      .vertex_binding_descriptions(&binding_descs)
      .vertex_attribute_descriptions(&attr_descs)
      .build();

    let input_assembly_ci = vk::PipelineInputAssemblyStateCreateInfo::builder()
      .topology(vk::PrimitiveTopology::TRIANGLE_LIST)
      .primitive_restart_enable(false)
      .build();

    let viewport = vk::Viewport::builder()
      .x(0f32)
      .y(0f32)
      .width(extent.width as f32)
      .height(extent.height as f32)
      .min_depth(0f32)
      .max_depth(1.0f32)
      .build();
    let viewports = [viewport];
    let scissor = vk::Rect2D::builder()
      .offset(vk::Offset2D::default())
      .extent(extent)
      .build();
    let scissors = [scissor];
    let viewport_state_ci = vk::PipelineViewportStateCreateInfo::builder()
      .viewports(&viewports)
      .scissors(&scissors)
      .build();

    let raster_state_ci = vk::PipelineRasterizationStateCreateInfo::builder()
      .depth_clamp_enable(false) // Don't clamp things to the edge, cull them.
      .rasterizer_discard_enable(false) // Don't discard geometry.
      .polygon_mode(vk::PolygonMode::FILL) // Fill stuff in. Could also be point or line.
      .line_width(1.0f32)
      .cull_mode(vk::CullModeFlags::BACK) // Back face culling.
      .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
      // Dont turn on depth bias, not adding constants to depth, same with depth_bias_clamp, bias_constant_factor, bias_slope_factor.
      .depth_bias_enable(false)
      .build();

    // Pretty much totall disable this.
    // TODO(issue#18) CONFIG make configurable
    let multisample_state_ci = vk::PipelineMultisampleStateCreateInfo::builder()
      .sample_shading_enable(false)
      .rasterization_samples(vk::SampleCountFlags::TYPE_1)
      .min_sample_shading(1.0f32)
      .alpha_to_coverage_enable(false)
      .alpha_to_one_enable(false)
      .build();

    // TODO(issue#18) CONFIG enable stencil.
    let depth_stencil_ci = vk::PipelineDepthStencilStateCreateInfo::builder()
      .depth_test_enable(true)
      .depth_write_enable(true)
      .depth_compare_op(vk::CompareOp::LESS) // Lower depth closer.
      .depth_bounds_test_enable(false) // Not using bounds test.
      .min_depth_bounds(0.0f32)
      .max_depth_bounds(1.0f32)
      .stencil_test_enable(false)
      // .front(vk::StencilOpState) // For use in stencil test
      // .back(vk::StencilOpState)
      .build();

    let color_blend_attachment_state = vk::PipelineColorBlendAttachmentState::builder()
      .color_write_mask(vk::ColorComponentFlags::all()) // RGBA
      .blend_enable(false)
      // everything else optional because its not enabled.
      .build();
    let attachments = [color_blend_attachment_state];
    let color_blend_ci = vk::PipelineColorBlendStateCreateInfo::builder()
      .logic_op_enable(false)
      .logic_op(vk::LogicOp::COPY)
      .attachments(&attachments)
      .build();

    let pipeline_layout_ci = vk::PipelineLayoutCreateInfo::builder()
      .set_layouts(&descriptor_set_layouts)
      .build();
    let pipeline_layout =
      unsafe { logical_device.create_pipeline_layout(&pipeline_layout_ci, None)? };

    let base_graphics_pipeline_ci = vk::GraphicsPipelineCreateInfo::builder()
      .flags(vk::PipelineCreateFlags::ALLOW_DERIVATIVES)
      .stages(&shader_stage_cis)
      .vertex_input_state(&vertex_input_ci)
      .input_assembly_state(&input_assembly_ci)
      .viewport_state(&viewport_state_ci)
      .rasterization_state(&raster_state_ci)
      .multisample_state(&multisample_state_ci)
      .depth_stencil_state(&depth_stencil_ci)
      .color_blend_state(&color_blend_ci)
      .layout(pipeline_layout)
      .render_pass(render_pass)
      .subpass(0) // The subpass where the pipeline will be used.
      // .base_pipeline_handle() // No basepipeline handle, this is the base pipeline!
      // .base_pipeline_index(-1)
      .build();

    // TODO(issue#17) RENDERING_CAPABILITIES use pipeline cache.
    let pipeline_create_infos = [base_graphics_pipeline_ci];
    let pipeline = unsafe {
      logical_device.create_graphics_pipelines(
        vk::PipelineCache::null(),
        &pipeline_create_infos,
        None,
      )
    };
    if let Err(err) = pipeline {
      return Err(err.1.into());
    }

    Ok(BasePipelineBundle::new(
      pipeline.unwrap()[0],
      pipeline_layout,
      base_graphics_pipeline_ci,
      descriptor_set_layouts,
      msaa_color_image,
      depth_buffer,
      vertex_shader_handle,
      fragment_shader_handle,
    ))
  }

  fn create_framebuffers(
    logical_device: &Device, render_pass: vk::RenderPass, depth_buffer: &DepthResources,
    render_target_images: &[ImageAndView], extent: vk::Extent2D,
  ) -> SarektResult<Vec<vk::Framebuffer>> {
    let mut framebuffers = Vec::with_capacity(render_target_images.len());

    for image_and_view in render_target_images.iter() {
      let attachments = [
        image_and_view.view,
        depth_buffer.image_and_memory.image_and_view.view,
      ];
      let framebuffer_ci = vk::FramebufferCreateInfo::builder()
        .render_pass(render_pass)
        .attachments(&attachments)
        .width(extent.width)
        .height(extent.height)
        .layers(1)
        .build();
      let framebuffer = unsafe { logical_device.create_framebuffer(&framebuffer_ci, None)? };
      framebuffers.push(framebuffer);
    }

    Ok(framebuffers)
  }
}
