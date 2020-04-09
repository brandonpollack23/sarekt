//! This is the vulkan renderer module for Sarekt.
//!  It is organized into submodules of bundles of structs that make sense
//! together, mainly just for organization.
pub mod vulkan_core;

mod base_pipeline_bundle;
mod debug_utils_ext;
mod depth_buffer;
mod draw_synchronization;
mod pipelines;
mod render_targets;
mod surface;
mod swap_chain;

use crate::{
  error::{SarektError, SarektResult},
  image_data::{ImageData, Monocolor},
  renderer::{
    buffers_and_images::{
      BufferAndImageLoader, BufferImageHandle, BufferImageStore, BufferOrImage, BufferType,
      IndexBufferElemSize, MagnificationMinificationFilter, ResourceType, TextureAddressMode,
      UniformBufferHandle,
    },
    drawable_object::DrawableObject,
    shaders::ShaderStore,
    vertex_bindings::DescriptorLayoutInfo,
    vulkan::{
      images::ImageAndView,
      queues::QueueFamilyIndices,
      vulkan_buffer_image_functions::{BufferAndMemoryMapped, ImageAndMemory, ResourceWithMemory},
      vulkan_renderer::{
        debug_utils_ext::DebugUserData,
        depth_buffer::DepthResources,
        draw_synchronization::DrawSynchronization,
        pipelines::Pipelines,
        render_targets::RenderTargetBundle,
        vulkan_core::{VulkanCoreStructures, VulkanDeviceStructures},
      },
      vulkan_shader_functions::VulkanShaderFunctions,
    },
    ApplicationDetails, Drawer, EngineDetails, Renderer, ShaderCode, ShaderHandle, ShaderType,
    VulkanBufferFunctions, MAX_FRAMES_IN_FLIGHT,
  },
};
use ash::{
  version::{DeviceV1_0, InstanceV1_0},
  vk, Device, Instance,
};
use log::{error, info, warn};
use raw_window_handle::HasRawWindowHandle;
use std::{
  cell::Cell,
  mem::ManuallyDrop,
  pin::Pin,
  sync::{Arc, RwLock},
};
use vk_shader_macros::include_glsl;

// TODO(issue#8) PERFORMANCE can i make things like descriptor set count and
// uniform buffers allocate number of frames in flight, not render target image
// count?

/// Default vertex shader that contain their own vertices, will be removed in
/// the future.
pub const DEFAULT_VERTEX_SHADER: &[u32] = include_glsl!("shaders/sarekt_forward.vert");
/// Default fragment shader that contain their own vertices, will be removed in
/// the future.
pub const DEFAULT_FRAGMENT_SHADER: &[u32] = include_glsl!("shaders/sarekt_forward.frag");

/// The Sarekt Vulkan Renderer, see module and crate level documentations for
/// details.
pub struct VulkanRenderer {
  vulkan_core: ManuallyDrop<VulkanCoreStructures>,
  vulkan_device_structures: ManuallyDrop<VulkanDeviceStructures>,
  render_target_bundle: RenderTargetBundle,
  pipelines: Pipelines,

  // Command pools, buffers, drawing, and synchronization related primitives and information.
  main_gfx_command_pool: vk::CommandPool,
  primary_gfx_command_buffers: Vec<vk::CommandBuffer>,
  transfer_command_pool: vk::CommandPool,
  draw_synchronization: DrawSynchronization,
  // Frame count since swapchain creation, not beginning of rendering.
  // TODO CRITICAL renderer function that returns this.
  frame_count: Cell<usize>,
  // Frame in flight number 0..MAX_FRAMES_IN_FLIGHT
  current_frame_num: Cell<usize>,
  next_image_index: Cell<usize>,

  // Descriptor pools.
  main_descriptor_pools: Vec<vk::DescriptorPool>,

  // Utilities
  allocator: Arc<vk_mem::Allocator>,
  shader_store: Arc<RwLock<ShaderStore<VulkanShaderFunctions>>>,
  // Manually drop so that the underlying allocator can be dropped in this class.
  buffer_image_store: ManuallyDrop<Arc<RwLock<BufferImageStore<VulkanBufferFunctions>>>>,

  // Null objects for default pipeline.
  default_texture: Option<(
    BufferImageHandle<VulkanBufferFunctions>,
    BufferOrImage<ResourceWithMemory>,
  )>,

  // Application controllable fields
  rendering_enabled: bool,
}
impl VulkanRenderer {
  /// Creates a VulkanRenderer for the window with no application name, no
  /// engine, and base versions of 0.1.0.
  pub fn new<W: HasRawWindowHandle, OW: Into<Option<Arc<W>>>>(
    window: OW, requested_width: u32, requested_height: u32,
  ) -> Result<Self, SarektError> {
    Self::new_detailed(
      window,
      requested_width,
      requested_height,
      ApplicationDetails::default(),
      EngineDetails::default(),
    )
  }

  /// Creates a VulkanRenderer with a given name/version/engine name/engine
  /// version.
  pub fn new_detailed<W: HasRawWindowHandle, OW: Into<Option<Arc<W>>>>(
    window: OW, requested_width: u32, requested_height: u32,
    application_details: ApplicationDetails, engine_details: EngineDetails,
  ) -> Result<Self, SarektError> {
    Self::new_detailed_with_debug_user_data(
      window,
      requested_width,
      requested_height,
      application_details,
      engine_details,
      None,
    )
  }

  /// Like new_detailed but allows injection of user data, for unit testing or
  /// metric gathering.
  fn new_detailed_with_debug_user_data<W: HasRawWindowHandle, OW: Into<Option<Arc<W>>>>(
    window: OW, requested_width: u32, requested_height: u32,
    application_details: ApplicationDetails, engine_details: EngineDetails,
    debug_user_data: Option<Pin<Arc<DebugUserData>>>,
  ) -> SarektResult<Self> {
    let window = window
      .into()
      .expect("Sarekt only supports rendering to a window right now :(");

    // TODO(issue#9) OFFSCREEN Support rendering to a non window surface if window
    // is None (change it to an Enum of WindowHandle or OtherSurface).
    info!("Creating Sarekt Renderer with Vulkan Backend...");

    let vulkan_core = ManuallyDrop::new(VulkanCoreStructures::new(
      window.as_ref(),
      application_details,
      engine_details,
      debug_user_data,
    )?);

    let vulkan_device_structures = ManuallyDrop::new(VulkanDeviceStructures::new(&vulkan_core)?);
    let physical_device = vulkan_device_structures.physical_device;
    let logical_device = &vulkan_device_structures.logical_device;
    let queue_families = &vulkan_device_structures.queue_families;
    let queues = &vulkan_device_structures.queues;

    // TODO(issue#9) OFFSCREEN only create if drawing to window, get format and
    // extent elsewhere.
    let render_target_bundle = RenderTargetBundle::new(
      &vulkan_core,
      &vulkan_device_structures,
      requested_width,
      requested_height,
    )?;
    let render_targets = &render_target_bundle.render_targets;

    let (main_gfx_command_pool, transfer_command_pool) =
      Self::create_primary_command_pools(queue_families, &logical_device)?;

    let allocator = Self::create_memory_allocator(
      vulkan_core.instance.as_ref().clone(),
      physical_device,
      logical_device.as_ref().clone(),
    )?;

    let shader_store = Self::create_shader_store(&logical_device);

    // TODO(issue#1) MULTITHREADING all graphics command pools needed here to
    // specify concurrent access.
    let buffer_image_store = ManuallyDrop::new(Self::create_buffer_image_store(
      &vulkan_core,
      &vulkan_device_structures,
      allocator.clone(),
      queue_families.graphics_queue_family.unwrap(),
      queue_families.transfer_queue_family.unwrap(),
      transfer_command_pool,
      queues.transfer_queue,
      main_gfx_command_pool,
      queues.graphics_queue,
    )?);

    let pipeline = Pipelines::new(
      &vulkan_core,
      &vulkan_device_structures,
      &render_target_bundle,
      &shader_store,
      &buffer_image_store,
    )?;
    let framebuffers = &pipeline.framebuffers;

    let primary_gfx_command_buffers =
      Self::create_main_gfx_command_buffers(&logical_device, main_gfx_command_pool, framebuffers)?;

    let draw_synchronization =
      DrawSynchronization::new(logical_device.clone(), render_targets.len())?;

    let main_descriptor_pools = Self::create_main_descriptor_pools(
      &vulkan_core.instance,
      physical_device,
      &logical_device,
      &render_targets,
    )?;

    let mut renderer = Self {
      vulkan_core,
      vulkan_device_structures,
      render_target_bundle,
      pipelines: pipeline,

      main_gfx_command_pool,
      primary_gfx_command_buffers,
      transfer_command_pool,
      draw_synchronization,
      frame_count: Cell::new(0),
      current_frame_num: Cell::new(0),
      next_image_index: Cell::new(0),

      main_descriptor_pools,

      allocator,
      shader_store,
      buffer_image_store,

      // To be initialized.
      default_texture: None,

      rendering_enabled: true,
    };

    renderer.create_default_texture();

    // Begin recording first command buffer so the first call to Drawer::draw is
    // ready to record.  The rest are started by Renderer::frame.
    renderer.setup_next_main_command_buffer()?;

    Ok(renderer)
  }
}
impl VulkanRenderer {
  /// When the target dimensions or requirements change, we must recreate a
  /// bunch of stuff to remain compatible and continue rendering to the new
  /// surface.
  unsafe fn do_recreate_swapchain(&mut self, width: u32, height: u32) -> SarektResult<()> {
    let instance = &self.vulkan_core.instance;
    let logical_device = &self.vulkan_device_structures.logical_device;
    let physical_device = self.vulkan_device_structures.physical_device;
    let shader_store = &self.shader_store;

    // Procedure: Wait for the device to be idle, make new Swapchain (recycling old
    // one), cleanup old resources and recreate them:
    // * ImageViews
    // * Render Passes
    // * Graphics Pipelines
    // * Framebuffers
    // * Command Buffers.
    logical_device.device_wait_idle()?;

    let (old_swapchain, old_images) = self.render_target_bundle.recreate_swapchain(
      &self.vulkan_core,
      &self.vulkan_device_structures,
      width,
      height,
    )?;
    self.cleanup_swapchain(Some((&old_images, old_swapchain)))?;
    let new_format = self.render_target_bundle.swapchain_and_extension.format;
    let new_extent = self.render_target_bundle.extent;

    let depth_buffer = DepthResources::new(
      &instance,
      physical_device,
      &self.buffer_image_store,
      (width, height),
    )?;

    self
      .pipelines
      .recreate_renderpasses(logical_device, new_format)?;

    let (vertex_shader_handle, fragment_shader_handle, descriptor_set_layouts) =
      self.pipelines.take_shaders_and_layouts();

    self.pipelines.recreate_framebuffers(
      logical_device,
      &depth_buffer,
      &self.render_target_bundle.render_targets,
      new_extent,
    )?;

    self.pipelines.recreate_base_pipeline_bundle(
      logical_device,
      shader_store,
      new_extent,
      depth_buffer,
      descriptor_set_layouts.unwrap(),
      vertex_shader_handle.unwrap(),
      fragment_shader_handle.unwrap(),
    )?;

    self.main_descriptor_pools = Self::create_main_descriptor_pools(
      instance,
      physical_device,
      logical_device,
      &self.render_target_bundle.render_targets,
    )?;

    // Reset render_frame_count
    self.current_frame_num.set(0);

    // Reset command buffers and rerun setup.
    logical_device.reset_command_pool(
      self.main_gfx_command_pool,
      vk::CommandPoolResetFlags::empty(),
    )?;
    self.draw_synchronization.recreate_semaphores()?;
    self.setup_next_main_command_buffer()?;

    Ok(())
  }

  /// Cleans up all resources dependent on the swapchain and the swapchain
  /// itself.
  /// * ImageViews
  /// * Render Passes
  /// * Graphics Pipelines
  /// * Framebuffers
  /// * Command Buffers.
  ///
  /// Optionally takes in the old swapchain images and handle to clean up for
  /// recreation, otherwise cleans up the currently active swapchain.
  unsafe fn cleanup_swapchain(
    &self, old_swapchain_bundle: Option<(&[ImageAndView], vk::SwapchainKHR)>,
  ) -> SarektResult<()> {
    let logical_device = &self.vulkan_device_structures.logical_device;

    self.draw_synchronization.wait_for_all_frames()?;

    info!("Destroying descriptor pools...");
    for &desc_pool in self.main_descriptor_pools.iter() {
      logical_device.destroy_descriptor_pool(desc_pool, None);
    }

    self.pipelines.cleanup(logical_device);

    let (images, swapchain) = old_swapchain_bundle.unwrap_or((
      self.render_target_bundle.render_targets.as_slice(),
      self.render_target_bundle.swapchain_and_extension.swapchain,
    ));
    self.render_target_bundle.cleanup_render_targets(
      &self.vulkan_device_structures,
      images,
      swapchain,
    );

    Ok(())
  }

  // ================================================================================
  //  Command & Descriptor Pool/Buffer Methods
  // ================================================================================
  /// Creates all command pools needed for drawing and presentation on one
  /// thread.
  ///
  /// return is (gfx command pool, transfer command pool).
  ///
  /// May be expanded in the future (compute etc).
  fn create_primary_command_pools(
    queue_family_indices: &QueueFamilyIndices, logical_device: &Device,
  ) -> SarektResult<(vk::CommandPool, vk::CommandPool)> {
    info!("Command Queues Selected: {:?}", queue_family_indices);

    let gfx_pool_ci = vk::CommandPoolCreateInfo::builder()
      .queue_family_index(queue_family_indices.graphics_queue_family.unwrap())
      .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER) // TODO PERFORMANCE create one command pool for each framebuffer to allow resetting individually at the pool level?
      .build();

    let gfx_pool = unsafe { logical_device.create_command_pool(&gfx_pool_ci, None)? };

    let transfer_pool =
      if queue_family_indices.graphics_queue_family == queue_family_indices.transfer_queue_family {
        gfx_pool
      } else {
        let transfer_pool_ci = vk::CommandPoolCreateInfo::builder()
          .queue_family_index(queue_family_indices.transfer_queue_family.unwrap())
          .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)
          .build();
        unsafe { logical_device.create_command_pool(&transfer_pool_ci, None)? }
      };

    Ok((gfx_pool, transfer_pool))
  }

  /// Creates command buffer for main thread to make draw calls on.
  ///
  /// TODO(issue#1) MULTITHREADING one secondary per thread.
  fn create_main_gfx_command_buffers(
    logical_device: &Device, primary_gfx_command_pool: vk::CommandPool,
    framebuffers: &[vk::Framebuffer],
  ) -> SarektResult<Vec<vk::CommandBuffer>> {
    let image_count = framebuffers.len() as u32;
    let gfx_command_buffer_ci = vk::CommandBufferAllocateInfo::builder()
      .command_pool(primary_gfx_command_pool)
      .level(vk::CommandBufferLevel::PRIMARY)
      .command_buffer_count(image_count)
      .build();

    let primary_gfx_command_buffers =
      unsafe { logical_device.allocate_command_buffers(&gfx_command_buffer_ci)? };

    Ok(primary_gfx_command_buffers)
  }

  /// Sets up the command buffers for recording.
  /// The command buffers are written to by the [Drawer](trait.Drawer.html) draw
  /// commands.
  fn setup_next_main_command_buffer(&self) -> SarektResult<()> {
    let current_frame_num = self.current_frame_num.get();
    let image_available_sem = self
      .draw_synchronization
      .get_image_available_sem(current_frame_num);

    self.draw_synchronization.wait_for_acquire_fence()?;
    self.draw_synchronization.reset_acquire_fence()?;
    // Get next image to render to.
    let (image_index, is_suboptimal) =
      // Will return if swapchain is out of date.
      self.render_target_bundle.acquire_next_image(
        u64::max_value(),
        image_available_sem,
        self.draw_synchronization.get_acquire_fence(),
      )?;
    if is_suboptimal {
      warn!("Swapchain is suboptimal!");
    }

    // TODO(issue#1) MULTITHREADING all things that were only main thread, do for
    // all renderers, too.
    let logical_device = &self.vulkan_device_structures.logical_device;
    let descriptor_pool = self.main_descriptor_pools[image_index as usize];
    let command_buffer = self.primary_gfx_command_buffers[image_index as usize];
    let framebuffer = self.pipelines.get_framebuffer(image_index as usize);
    let extent = self.render_target_bundle.extent;
    // TODO(issue#2) PIPELINES when multiple render pass types are supported use the
    // *selected* one.
    let render_pass = self.pipelines.forward_render_pass;
    let pipeline = self.pipelines.get_current_pipeline();

    // Make sure we wait on any fences for that swap chain image in flight.  Can't
    // write to a command buffer if it is in flight.
    let fence = self
      .draw_synchronization
      .get_image_fence(image_index as usize);
    if fence != vk::Fence::null() {
      unsafe {
        logical_device.wait_for_fences(&[fence], true, u64::max_value())?;
      }
    }

    unsafe {
      // TODO(issue#10) PERFORMANCE cache descriptor sets: https://github.com/KhronosGroup/Vulkan-Samples/blob/master/samples/performance/descriptor_management/descriptor_management_tutorial.md
      logical_device
        .reset_descriptor_pool(descriptor_pool, vk::DescriptorPoolResetFlags::empty())?;
    }

    // Start recording.
    unsafe {
      let begin_ci = vk::CommandBufferBeginInfo::builder()
        .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT)
        .build();
      logical_device.begin_command_buffer(command_buffer, &begin_ci)?
    };

    unsafe {
      // Start the (forward) render pass.
      let render_area = vk::Rect2D::builder()
        .offset(vk::Offset2D::default())
        .extent(extent)
        .build();
      let clear_color_value = vk::ClearValue {
        color: vk::ClearColorValue {
          float32: [0f32, 0f32, 0f32, 1f32],
        },
      };
      let clear_depth_value = vk::ClearValue {
        depth_stencil: vk::ClearDepthStencilValue {
          depth: 1.0f32,
          stencil: 0u32,
        },
      };
      let clear_values = [clear_color_value, clear_depth_value];
      let render_pass_begin_info = vk::RenderPassBeginInfo::builder()
        .render_pass(render_pass)
        .framebuffer(framebuffer)
        .render_area(render_area)
        .clear_values(&clear_values) // Clear to black.
        .build();

      logical_device.cmd_begin_render_pass(
        command_buffer,
        &render_pass_begin_info,
        vk::SubpassContents::INLINE,
      );

      // Bind the pipeline. Can be overridden in secondary buffer by the user.
      // TODO(issue#1) MULTITHREADING we can keep track in each thread's
      // command buffer waht pipeline is bound so we don't insert extra rebind
      // commands.
      logical_device.cmd_bind_pipeline(command_buffer, vk::PipelineBindPoint::GRAPHICS, pipeline)
    };

    // Save image index for frame presentation.
    self.next_image_index.set(image_index as usize);

    // Draw occurs in in the Drawer::draw command.
    // Render pass completion occurs in Renderer::frame
    Ok(())
  }

  fn create_main_descriptor_pools(
    instance: &Instance, physical_device: vk::PhysicalDevice, logical_device: &Device,
    render_targets: &[ImageAndView],
  ) -> SarektResult<Vec<vk::DescriptorPool>> {
    // TODO(issue#1) MULTITHREADING one per per frame per thread.

    let physical_device_properties =
      unsafe { instance.get_physical_device_properties(physical_device) };

    let max_uniform_buffers = physical_device_properties
      .limits
      .max_descriptor_set_uniform_buffers;
    let max_combined_image_samplers = physical_device_properties
      .limits
      .max_descriptor_set_samplers;

    let pool_sizes = [
      vk::DescriptorPoolSize::builder()
        .ty(vk::DescriptorType::UNIFORM_BUFFER)
        .descriptor_count(max_uniform_buffers)
        .build(),
      vk::DescriptorPoolSize::builder()
        .ty(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
        .descriptor_count(max_combined_image_samplers)
        .build(),
    ];

    info!("Creating descriptor pool with sizes: {:?}", pool_sizes);

    let descriptor_pool_ci = vk::DescriptorPoolCreateInfo::builder()
      .pool_sizes(&pool_sizes)
      .max_sets(
        max_uniform_buffers.min(max_combined_image_samplers)
      ) // One set per render object.
      .build();

    info!(
      "Creating descriptor pool with configuration: {:?}",
      descriptor_pool_ci
    );

    let mut main_descriptor_pools = Vec::with_capacity(render_targets.len());
    for _ in 0..render_targets.len() {
      unsafe {
        main_descriptor_pools
          .push(logical_device.create_descriptor_pool(&descriptor_pool_ci, None)?);
      };
    }

    Ok(main_descriptor_pools)
  }

  // ================================================================================
  //  Storage Creation Methods
  // ================================================================================
  /// Creates a [Vulkan Memory Allocator](https://github.com/gwihlidal/vk-mem-rs)
  fn create_memory_allocator(
    instance: Instance, physical_device: vk::PhysicalDevice, logical_device: Device,
  ) -> SarektResult<Arc<vk_mem::Allocator>> {
    let allocator_create_info = vk_mem::AllocatorCreateInfo {
      physical_device,
      device: logical_device,
      instance,
      flags: vk_mem::AllocatorCreateFlags::default(),
      preferred_large_heap_block_size: 0,
      frame_in_use_count: (MAX_FRAMES_IN_FLIGHT - 1) as u32,
      heap_size_limits: None,
    };

    Ok(vk_mem::Allocator::new(&allocator_create_info).map(Arc::new)?)
  }

  /// Creates a shader store in the vulkan backend configuration to load and
  /// delete shaders from.
  fn create_shader_store(
    logical_device: &Arc<Device>,
  ) -> Arc<RwLock<ShaderStore<VulkanShaderFunctions>>> {
    let functions = VulkanShaderFunctions::new(logical_device.clone());
    Arc::new(RwLock::new(ShaderStore::new(functions)))
  }

  fn create_buffer_image_store(
    vulkan_core: &VulkanCoreStructures, vulkan_device_bundle: &VulkanDeviceStructures,
    allocator: Arc<vk_mem::Allocator>, graphics_queue_family: u32, transfer_queue_family: u32,
    transfer_command_pool: vk::CommandPool, transfer_command_queue: vk::Queue,
    graphics_command_pool: vk::CommandPool, graphics_command_queue: vk::Queue,
  ) -> SarektResult<Arc<RwLock<BufferImageStore<VulkanBufferFunctions>>>> {
    let functions = VulkanBufferFunctions::new(
      vulkan_core,
      vulkan_device_bundle,
      allocator,
      graphics_queue_family,
      transfer_queue_family,
      transfer_command_pool,
      transfer_command_queue,
      graphics_command_pool,
      graphics_command_queue,
    )?;
    Ok(Arc::new(RwLock::new(BufferImageStore::new(functions))))
  }

  // ================================================================================
  //  Draw Helper Methods
  // ================================================================================
  fn draw_vertices_cmd<UniformBufElem: Sized + Copy>(
    &self, object: &DrawableObject<Self, UniformBufElem>, command_buffer: vk::CommandBuffer,
  ) -> SarektResult<()> {
    let logical_device = &self.vulkan_device_structures.logical_device;

    unsafe {
      // Draw vertices.
      let vertex_buffers = [object.vertex_buffer.buffer()?.buffer];
      let vertex_buffer_length = object.vertex_buffer.buffer()?.length;
      let offsets = [0];
      logical_device.cmd_bind_vertex_buffers(
        command_buffer,
        0,
        &vertex_buffers,
        &offsets, // There may be offset into memory, but not into the buffer.
      );

      if object.index_buffer.is_none() {
        // Non indexed draw.
        logical_device.cmd_draw(command_buffer, vertex_buffer_length, 1, 0, 0);
      } else {
        // Indexed Draw.
        let index_buffer = &object.index_buffer.unwrap().buffer()?;
        let index_buffer_element_size = match index_buffer.index_buffer_elem_size.unwrap() {
          IndexBufferElemSize::UInt16 => vk::IndexType::UINT16,
          IndexBufferElemSize::UInt32 => vk::IndexType::UINT32,
        };
        logical_device.cmd_bind_index_buffer(
          command_buffer,
          index_buffer.buffer,
          0,
          index_buffer_element_size,
        );
        logical_device.cmd_draw_indexed(
          command_buffer,
          index_buffer.length,
          1, // One instance
          0, // No index offset
          0, // No vertex offset
          0, // Zeroth instance.
        )
      }
    }
    Ok(())
  }

  fn bind_descriptor_sets<DescriptorLayoutStruct>(
    &self, uniform_buffer: vk::Buffer, texture_image: &Option<ImageAndMemory>,
    descriptor_pool: vk::DescriptorPool, command_buffer: vk::CommandBuffer,
  ) -> SarektResult<()>
  where
    DescriptorLayoutStruct: Sized + Copy + DescriptorLayoutInfo,
  {
    let logical_device = &self.vulkan_device_structures.logical_device;

    // First allocate descriptor sets.
    // TODO(issue#2) PIPELINES pass pipeline layout of the pipeline that is running
    // now.
    let layouts = self.pipelines.get_pipeline_descriptor_layouts();
    let alloc_info = vk::DescriptorSetAllocateInfo::builder()
      .descriptor_pool(descriptor_pool)
      .set_layouts(&layouts) // Sets descriptor set count.
      .build();
    // A vec of a single set.
    let descriptor_sets = unsafe { logical_device.allocate_descriptor_sets(&alloc_info)? };

    // Then configure them to bind the buffer to the pipeline.
    let bind_uniform_info = DescriptorLayoutStruct::get_bind_uniform_info()?;
    let uniform_buffer_infos = vec![vk::DescriptorBufferInfo::builder()
      .buffer(uniform_buffer)
      .offset(bind_uniform_info.offset as vk::DeviceSize)
      .range(bind_uniform_info.range as vk::DeviceSize)
      .build()];

    // TODO(issue#11) LIGHTING TEXTURES SHADERS when there is more than one texture
    // allowed fill a vec with null textures for all unused textures in drawable
    // objects, which will now be a option vec.

    // Either load the texture in the drawable object or use a transparent null
    // texture.
    let bind_texture_info = DescriptorLayoutStruct::get_bind_texture_info()?;
    let image_infos = vec![match texture_image {
      Option::Some(image_and_memory) => vk::DescriptorImageInfo::builder()
        .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
        .image_view(image_and_memory.image_and_view.view)
        .sampler(image_and_memory.sampler.unwrap())
        .build(),
      None => {
        let default_texture = self
          .default_texture
          .as_ref()
          .unwrap()
          .1
          .handle
          .image()
          .unwrap();
        vk::DescriptorImageInfo::builder()
          .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
          .sampler(default_texture.sampler.unwrap())
          .image_view(default_texture.image_and_view.view)
          .build()
      }
    }];

    // Create descriptor writes for uniforms.
    let uniform_descriptor_writes = bind_uniform_info.bindings.iter().map(|&binding| {
      vk::WriteDescriptorSet::builder()
      .dst_set(descriptor_sets[0])
      .dst_binding(binding) // corresponds to binding in layout.
      .dst_array_element(0) // We're not using an array yet, just one MVP so index is 0.
      .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
      .buffer_info(&uniform_buffer_infos)
      // No image infos or texel buffer views because this is a buffer.
      .build()
    });

    // Create and append descriptor writes for textures.
    let texture_descriptor_writes = bind_texture_info.bindings.iter().map(|&binding| {
      vk::WriteDescriptorSet::builder()
        .dst_set(descriptor_sets[0])
        .dst_binding(binding)
        .dst_array_element(0)
        .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
        .image_info(&image_infos)
        .build()
    });

    let mut descriptor_writes =
      Vec::with_capacity(uniform_descriptor_writes.len() + texture_descriptor_writes.len());
    descriptor_writes.extend(uniform_descriptor_writes);
    descriptor_writes.extend(texture_descriptor_writes);

    unsafe {
      logical_device.update_descriptor_sets(&descriptor_writes, &[]); // No descriptor copies.

      // Bind them to the pipeline layout.
      // TODO(issue#2) PIPELINES select current pipeline layout. Same as above.
      logical_device.cmd_bind_descriptor_sets(
        command_buffer,
        vk::PipelineBindPoint::GRAPHICS,
        self.pipelines.get_pipeline_layout(),
        0,
        &descriptor_sets,
        &[], // No dynamic offsets.
      );
    }

    Ok(())
  }

  // ================================================================================
  //  Null object setup methods
  // ================================================================================
  /// A default texture to use when there isn't one selected for the drawable
  /// object slot.
  ///
  /// This will be used in place of all shaders in a Drawable Object that are
  /// set to None.
  fn create_default_texture(&mut self) {
    let image_and_handle = BufferImageStore::load_image_with_staging_initialization(
      &self.buffer_image_store,
      Monocolor::clear(),
      MagnificationMinificationFilter::Nearest,
      MagnificationMinificationFilter::Nearest,
      TextureAddressMode::ClampToEdge,
      TextureAddressMode::ClampToEdge,
      TextureAddressMode::ClampToEdge,
    )
    .unwrap();

    self.default_texture = Some(image_and_handle);
  }

  // ================================================================================
  //  Renderer Utility Methods
  // ================================================================================
  fn increment_frame_count(&self) {
    self.frame_count.set(self.frame_count.get() + 1);
    self
      .current_frame_num
      .set((self.current_frame_num.get() + 1) % MAX_FRAMES_IN_FLIGHT);
  }
}
impl Renderer for VulkanRenderer {
  type BL = VulkanBufferFunctions;
  type SL = VulkanShaderFunctions;

  fn set_rendering_enabled(&mut self, enabled: bool) {
    self.rendering_enabled = enabled;
  }

  // TODO(issue#9) OFFSCREEN handle off screen rendering.
  fn frame(&self) -> SarektResult<()> {
    let logical_device = &self.vulkan_device_structures.logical_device;
    let queues = &self.vulkan_device_structures.queues;

    if !self.rendering_enabled {
      return Ok(());
    }

    let current_frame_num = self.current_frame_num.get();
    let image_available_sem = self
      .draw_synchronization
      .get_image_available_sem(current_frame_num);
    let render_finished_sem = self
      .draw_synchronization
      .get_render_finished_semaphore(current_frame_num);

    let image_index = self.next_image_index.get();
    let current_command_buffer = self.primary_gfx_command_buffers[image_index as usize];
    unsafe {
      // End Render Pass.
      logical_device.cmd_end_render_pass(current_command_buffer);

      // Finish recording on all command buffers.
      // TODO(issue#1) MULTITHREADING all of them not just main.
      logical_device.end_command_buffer(current_command_buffer)?;
    }

    // Wait for max images in flight.
    let frame_fence = self
      .draw_synchronization
      .ensure_image_resources_ready(image_index as usize, current_frame_num)?;
    self
      .draw_synchronization
      .set_image_to_in_flight_frame(image_index as usize, current_frame_num);

    // Submit draw commands.
    let wait_semaphores = [image_available_sem];
    let wait_dst_stage_mask = [vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];
    let command_buffers = [current_command_buffer];
    let signal_semaphores = [render_finished_sem];
    let submit_info = vk::SubmitInfo::builder()
      .wait_semaphores(&wait_semaphores) // Don't draw until it is ready.
      .wait_dst_stage_mask(&wait_dst_stage_mask) // Don't we only need to wait until Color Attachment is ready to start drawing.  Vertex and other shaders can begin sooner.
      .command_buffers(&command_buffers) // Only use the command buffer corresponding to this image index.
      .signal_semaphores(&signal_semaphores) // Signal we're done drawing when we are.
      .build();
    unsafe { logical_device.queue_submit(queues.graphics_queue, &[submit_info], frame_fence)? };

    // TODO(issue#1) OFFSCREEN only if presenting to swapchain.
    // Present to swapchain and display completed frame.
    let wait_semaphores = [render_finished_sem];
    self.render_target_bundle.queue_present(
      image_index,
      queues.presentation_queue,
      &wait_semaphores,
    )?;

    // Increment frames rendered count.
    self.increment_frame_count();

    // Set up the next frame for drawing. Will wait on fence.
    self.setup_next_main_command_buffer()?;

    Ok(())
  }

  fn load_shader(
    &mut self, code: &ShaderCode, shader_type: ShaderType,
  ) -> SarektResult<ShaderHandle<VulkanShaderFunctions>> {
    ShaderStore::load_shader(&self.shader_store, &code, shader_type)
  }

  fn load_buffer<BufElem: Sized + Copy>(
    &mut self, buffer_type: BufferType, buffer: &[BufElem],
  ) -> SarektResult<BufferImageHandle<VulkanBufferFunctions>> {
    if matches!(buffer_type, BufferType::Uniform) {
      return Err(SarektError::IncorrectLoaderFunction);
    }

    Ok(BufferImageStore::load_buffer_with_staging(&self.buffer_image_store, buffer_type, buffer)?.0)
  }

  fn load_image_with_staging_initialization(
    &mut self, pixels: impl ImageData, magnification_filter: MagnificationMinificationFilter,
    minification_filter: MagnificationMinificationFilter, address_x: TextureAddressMode,
    address_y: TextureAddressMode, address_z: TextureAddressMode,
  ) -> SarektResult<BufferImageHandle<VulkanBufferFunctions>> {
    Ok(
      BufferImageStore::load_image_with_staging_initialization(
        &self.buffer_image_store,
        pixels,
        magnification_filter,
        minification_filter,
        address_x,
        address_y,
        address_z,
      )?
      .0,
    )
  }

  fn get_buffer(
    &self, handle: &BufferImageHandle<VulkanBufferFunctions>,
  ) -> SarektResult<ResourceWithMemory> {
    let store = self
      .buffer_image_store
      .read()
      .expect("Panic occured can't read from buffer store");

    if let result @ ResourceWithMemory::Buffer(_) = store.get_buffer(handle)?.handle {
      Ok(result)
    } else {
      Err(SarektError::IncorrectResourceType)
    }
  }

  fn load_uniform_buffer<UniformBufElem: Sized + Copy>(
    &mut self, buffer: UniformBufElem,
  ) -> SarektResult<UniformBufferHandle<VulkanBufferFunctions, UniformBufElem>> {
    info!("Loading a uniform buffer...");
    // Since each framebuffer may have different values for uniforms, they each need
    // their own UB.  These are stored in the same ordering as the render target
    // images.
    let mut uniform_buffers = Vec::with_capacity(self.pipelines.framebuffers.len());
    for _ in 0..self.pipelines.framebuffers.len() {
      // TODO(issue#13) PERFORMANCE EASY create a "locked" version of the loading
      // function so I don't have to keep reacquiring it.
      let (uniform_buffer_handle, _) = BufferImageStore::load_buffer_without_staging(
        &self.buffer_image_store,
        BufferType::Uniform,
        &[buffer],
      )?;
      uniform_buffers.push(uniform_buffer_handle);
    }

    Ok(UniformBufferHandle::new(uniform_buffers))
  }

  fn get_uniform_buffer<UniformBufElem: Sized + Copy>(
    &self, handle: &UniformBufferHandle<VulkanBufferFunctions, UniformBufElem>,
  ) -> SarektResult<Vec<BufferAndMemoryMapped>>
  where
    Self::BL: BufferAndImageLoader,
  {
    let store = self
      .buffer_image_store
      .read()
      .expect("Panic occured can't read from buffer store");
    let mut buffer_handles: Vec<BufferAndMemoryMapped> =
      Vec::with_capacity(self.render_target_bundle.render_targets.len());
    for ubh in handle.uniform_buffer_backend_handle.iter() {
      let handle = store.get_buffer(ubh)?;

      // Check this is a uniform buffer handle.
      match handle.resource_type {
        ResourceType::Buffer(BufferType::Uniform) => (),
        ResourceType::Buffer(_) => return Err(SarektError::IncorrectBufferType),
        _ => return Err(SarektError::IncorrectResourceType),
      };

      let allocation = &handle.handle.buffer()?.allocation;
      let ptr = self.allocator.map_memory(allocation)?;
      let buffer_and_mem_mapped = if let ResourceWithMemory::Buffer(buffer_handle) = handle.handle {
        Ok(BufferAndMemoryMapped::new(buffer_handle, ptr))
      } else {
        Err(SarektError::IncorrectResourceType)
      }?;

      buffer_handles.push(buffer_and_mem_mapped);
    }

    Ok(buffer_handles)
  }

  fn set_uniform<BufElem: Sized + Copy>(
    &self, handle_data: &Vec<BufferAndMemoryMapped>, data: &BufElem,
  ) -> SarektResult<()> {
    self.draw_synchronization.wait_for_acquire_fence()?;

    let next_image_index = self.next_image_index.get();
    unsafe {
      let ptr = handle_data[next_image_index].ptr as *mut BufElem;
      ptr.copy_from(data, std::mem::size_of_val(data));
    }

    Ok(())
  }

  fn recreate_swapchain(&mut self, width: u32, height: u32) -> SarektResult<()> {
    if width == 0 || height == 0 {
      // It violates the vulkan spec to make extents this small, rendering should be
      // disabled explicitly in this case, but its up the application/platform.
      return Ok(());
    }

    if self.render_target_bundle.extent_is_equal_to(width, height) {
      info!("No change, nothing to do");
      return Ok(());
    }

    unsafe { self.do_recreate_swapchain(width, height) }
  }

  fn get_image(
    &self, handle: &BufferImageHandle<VulkanBufferFunctions>,
  ) -> SarektResult<ResourceWithMemory> {
    let store = self
      .buffer_image_store
      .read()
      .expect("Panic occured can't read from buffer store");

    if let result @ ResourceWithMemory::Image(_) = store.get_image(handle)?.handle {
      Ok(result)
    } else {
      Err(SarektError::IncorrectResourceType)
    }
  }
}
impl Drawer for VulkanRenderer {
  type R = VulkanRenderer;

  /// TODO(issue#1) MULTITHREADING maybe need put everything that may need to be
  /// recreated in a cell?

  // TODO(issue#6) UNIFORMS do push_constant uniform buffers and example.
  fn draw<DescriptorLayoutStruct>(
    &self, object: &DrawableObject<Self, DescriptorLayoutStruct>,
  ) -> SarektResult<()>
  where
    DescriptorLayoutStruct: Sized + Copy + DescriptorLayoutInfo,
  {
    if !self.rendering_enabled {
      return Ok(());
    }

    let current_render_target_index = self.next_image_index.get();

    // Current render target command buffer.
    let current_command_buffer = self.primary_gfx_command_buffers[current_render_target_index];
    let current_uniform_buffer = object.uniform_buffer[current_render_target_index]
      .buffer_and_memory
      .buffer;
    let current_descriptor_pool = self.main_descriptor_pools[current_render_target_index];

    // Allocate and bind the correct uniform descriptors.
    self.bind_descriptor_sets::<DescriptorLayoutStruct>(
      current_uniform_buffer,
      &object.texture_image.map(|ti| ti.image().unwrap()),
      current_descriptor_pool,
      current_command_buffer,
    )?;

    // Draw the vertices (indexed or otherwise).
    self.draw_vertices_cmd(object, current_command_buffer)?;

    Ok(())
  }
}
impl Drop for VulkanRenderer {
  fn drop(&mut self) {
    unsafe {
      let logical_device = &self.vulkan_device_structures.logical_device;

      info!("Waiting for the device to be idle before cleaning up...");
      if let Err(e) = logical_device.device_wait_idle() {
        error!("Failed to wait for idle! {}", e);
      }

      info!("Destroying default null texture...");
      let default_texture = self.default_texture.take();
      std::mem::drop(default_texture);

      info!("Destroying all images, buffers, and associated synchronization semaphores...");
      self.buffer_image_store.write().unwrap().cleanup().unwrap();
      ManuallyDrop::drop(&mut self.buffer_image_store);

      info!("Destroying VMA...");
      Arc::get_mut(&mut self.allocator).unwrap().destroy();

      // TODO(issue#1) MULTITHREADING do I need to free others?
      info!("Freeing main command buffer...");
      logical_device.free_command_buffers(
        self.main_gfx_command_pool,
        &self.primary_gfx_command_buffers,
      );

      self
        .cleanup_swapchain(None)
        .expect("Could not clean up swapchain while cleaning up VulkanRenderer...");

      self
        .pipelines
        .cleanup_descriptor_set_layouts(logical_device);

      self.draw_synchronization.destroy_all();

      info!("Destroying all command pools...");
      logical_device.destroy_command_pool(self.main_gfx_command_pool, None);
      if self.main_gfx_command_pool != self.transfer_command_pool {
        logical_device.destroy_command_pool(self.transfer_command_pool, None);
      }

      info!("Destroying all shaders...");
      self.shader_store.write().unwrap().destroy_all_shaders();

      ManuallyDrop::drop(&mut self.vulkan_device_structures);
      ManuallyDrop::drop(&mut self.vulkan_core);
    }
  }
}

#[cfg(test)]
mod tests {
  use super::{debug_utils_ext::DebugUserData, VulkanRenderer};
  use crate::renderer::{ApplicationDetails, EngineDetails, Version, IS_DEBUG_MODE};
  use log::Level;
  use std::{pin::Pin, sync::Arc};
  #[cfg(unix)]
  use winit::platform::unix::EventLoopExtUnix;
  #[cfg(windows)]
  use winit::platform::windows::EventLoopExtWindows;
  use winit::{event_loop::EventLoop, window::WindowBuilder};

  const WIDTH: u32 = 800;
  const HEIGHT: u32 = 600;

  fn assert_no_warnings_or_errors_in_debug_user_data(debug_user_data: &Pin<Arc<DebugUserData>>) {
    if !IS_DEBUG_MODE {
      return;
    }

    let error_counts = debug_user_data.get_error_counts();

    assert_eq!(error_counts.error_count, 0);
    assert_eq!(error_counts.warning_count, 0);
  }

  #[test]
  fn can_construct_renderer_with_new() {
    let _log = simple_logger::init_with_level(Level::Info);
    let event_loop = EventLoop::<()>::new_any_thread();
    let window = Arc::new(WindowBuilder::new().build(&event_loop).unwrap());
    let renderer = VulkanRenderer::new(window, WIDTH, HEIGHT).unwrap();

    assert_no_warnings_or_errors_in_debug_user_data(
      &renderer.vulkan_core.get_debug_user_data().unwrap(),
    );
  }

  #[test]
  fn can_construct_renderer_with_new_detailed() {
    let _log = simple_logger::init_with_level(Level::Info);
    let event_loop = EventLoop::<()>::new_any_thread();
    let window = Arc::new(WindowBuilder::new().build(&event_loop).unwrap());
    let renderer = VulkanRenderer::new_detailed(
      window,
      WIDTH,
      HEIGHT,
      ApplicationDetails::new("Testing App", Version::new(0, 1, 0)),
      EngineDetails::new("Test Engine", Version::new(0, 1, 0)),
    )
    .unwrap();

    assert_no_warnings_or_errors_in_debug_user_data(
      &renderer.vulkan_core.get_debug_user_data().unwrap(),
    );
  }

  #[test]
  fn can_construct_renderer_with_new_detailed_and_user_data() {
    let _log = simple_logger::init_with_level(Level::Info);
    let event_loop = EventLoop::<()>::new_any_thread();
    let window = Arc::new(WindowBuilder::new().build(&event_loop).unwrap());
    let debug_user_data = Arc::pin(DebugUserData::new());
    let renderer = VulkanRenderer::new_detailed_with_debug_user_data(
      window,
      WIDTH,
      HEIGHT,
      ApplicationDetails::new("Testing App", Version::new(0, 1, 0)),
      EngineDetails::new("Test Engine", Version::new(0, 1, 0)),
      Some(debug_user_data.clone()),
    )
    .unwrap();

    std::mem::drop(renderer);
    assert_no_warnings_or_errors_in_debug_user_data(&debug_user_data);
  }

  // TODO(issue#14) TESTING write triangle sanity check that can dump buffer and
  // compare to golden image.

  // TODO(issue#14) TESTING write tests for public api using this.  rust doesn't
  // run in test harness so in some platforms calls to frame don't present correctly. also consider [this](https://stackoverflow.com/questions/43458194/is-there-any-way-to-tell-cargo-to-run-its-tests-on-the-main-thread)
}
