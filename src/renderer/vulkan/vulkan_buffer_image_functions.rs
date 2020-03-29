use crate::{
  error::{SarektError, SarektResult},
  image_data::ImageData,
  renderer::{
    buffers_and_images::{
      BackendHandleTrait, BufferAndImageLoader, BufferImageHandle, BufferType, IndexBufferElemSize,
      MagnificationMinificationFilter, TextureAddressMode,
    },
    vulkan::images::ImageAndView,
  },
};
use ash::{version::DeviceV1_0, vk, Device};
use log::info;
use std::sync::Arc;

/// TODO PERFORMANCE MEMORY allow swapping memory with "lost" in VMA.

/// TODO PERFORMANCE stage buffer allocations to be transfered in one staging
/// buffer commit load operation instead of doing each one seperate and waiting.
/// Be sure to only delete staging stuff after the commit operation.

/// TODO PERFORMANCE defragmentation of VMA

/// Vulkan implementation of [BufferLoader](trait.BufferLoader.html).
#[derive(Clone)]
pub struct VulkanBufferFunctions {
  logical_device: Arc<Device>,
  allocator: Arc<vk_mem::Allocator>,
  transfer_command_buffer: vk::CommandBuffer,
  transfer_command_queue: vk::Queue,
  graphics_command_buffer: vk::CommandBuffer,
  graphics_command_queue: vk::Queue,
  graphics_queue_family: u32,
  transfer_queue_family: u32,

  ownership_semaphore: [vk::Semaphore; 1],
}
impl VulkanBufferFunctions {
  pub fn new(
    logical_device: Arc<Device>, allocator: Arc<vk_mem::Allocator>, graphics_queue_family: u32,
    transfer_queue_family: u32, transfer_command_pool: vk::CommandPool,
    transfer_command_queue: vk::Queue, graphics_command_pool: vk::CommandPool,
    graphics_command_queue: vk::Queue,
  ) -> SarektResult<Self> {
    let command_buffer_alloc_info = vk::CommandBufferAllocateInfo::builder()
      .level(vk::CommandBufferLevel::PRIMARY)
      .command_pool(transfer_command_pool)
      .command_buffer_count(1)
      .build();
    let transfer_command_buffer =
      unsafe { logical_device.allocate_command_buffers(&command_buffer_alloc_info)?[0] };

    let graphics_command_buffer = if graphics_command_pool != transfer_command_pool {
      let command_buffer_alloc_info = vk::CommandBufferAllocateInfo::builder()
        .level(vk::CommandBufferLevel::PRIMARY)
        .command_pool(graphics_command_pool)
        .command_buffer_count(1)
        .build();
      unsafe { logical_device.allocate_command_buffers(&command_buffer_alloc_info)?[0] }
    } else {
      transfer_command_buffer
    };

    let ownership_semaphore = if graphics_command_pool != transfer_command_pool {
      let semaphore_ci = vk::SemaphoreCreateInfo::default();
      unsafe { [logical_device.create_semaphore(&semaphore_ci, None)?] }
    } else {
      [vk::Semaphore::null()]
    };

    Ok(Self {
      logical_device,
      allocator,
      transfer_command_buffer,
      transfer_command_queue,
      graphics_command_buffer,
      graphics_command_queue,
      graphics_queue_family,
      transfer_queue_family,

      ownership_semaphore,
    })
  }

  /// Creates a CPU visible staging buffer that has the TRANSFER_SRC usage bit
  /// flipped.
  fn create_staging_buffer(
    &self, buffer_size: u64,
  ) -> SarektResult<(vk::Buffer, vk_mem::Allocation, vk_mem::AllocationInfo)> {
    info!("Creating staging buffer");
    self.create_cpu_accessible_buffer(buffer_size, vk::BufferUsageFlags::TRANSFER_SRC)
  }

  /// For creating a cpu accessible buffer for any usage (more generic than
  /// staging buffer).
  ///
  /// Set usage flags as you see fit (or don't...).
  fn create_cpu_accessible_buffer(
    &self, buffer_size: u64, usage_flags: vk::BufferUsageFlags,
  ) -> SarektResult<(vk::Buffer, vk_mem::Allocation, vk_mem::AllocationInfo)> {
    info!(
      "Creating cpu accessible buffer and memory of size {} to transfer from CPU memory...",
      buffer_size
    );
    let staging_buffer_ci = vk::BufferCreateInfo::builder()
      .size(buffer_size)
      .usage(usage_flags)
      .sharing_mode(vk::SharingMode::EXCLUSIVE) // This is still only used by one Queue (Command)
      .build();
    let staging_alloc_ci = vk_mem::AllocationCreateInfo {
      usage: vk_mem::MemoryUsage::CpuToGpu,
      /* All the required and preferred flags such as
       * HOST_VISIBLE, HOST_COHERENT, memory type bits, etc
       * are automagically configured by this usage flag.
       * Which works for my use case */
      ..vk_mem::AllocationCreateInfo::default()
    };

    Ok(
      self
        .allocator
        .create_buffer(&staging_buffer_ci, &staging_alloc_ci)?,
    )
  }

  /// Create a buffer with TRANSFER_DST and appropriate buffer type flags
  /// flipped.
  fn create_gpu_buffer(
    &self, buffer_type: BufferType, buffer_size: u64,
  ) -> SarektResult<(vk::Buffer, vk_mem::Allocation, vk_mem::AllocationInfo)> {
    info!("Creating GPU buffer and memory to use during drawing...");
    let buffer_usage =
      vk::BufferUsageFlags::TRANSFER_DST | usage_flags_from_buffer_type(buffer_type);
    // TODO PERFORMANCE instead of concurrent do a transfer like for images.
    let sharing_mode = if self.graphics_queue_family == self.transfer_queue_family {
      vk::SharingMode::EXCLUSIVE
    } else {
      vk::SharingMode::CONCURRENT
    };
    let queue_family_indices = [self.graphics_queue_family, self.transfer_queue_family];
    let buffer_ci = vk::BufferCreateInfo::builder()
      .size(buffer_size)
      .usage(buffer_usage)
      .sharing_mode(sharing_mode)
      .queue_family_indices(&queue_family_indices) // Ignored if exclusive.
      .build();
    let alloc_ci = vk_mem::AllocationCreateInfo {
      usage: vk_mem::MemoryUsage::GpuOnly,
      /* All the required and preferred flags such as
       * HOST_VISIBLE, HOST_COHERENT, memory type bits, etc
       * are automagically configured by this usage flag.
       * Which works for my use case */
      ..vk_mem::AllocationCreateInfo::default()
    };

    Ok(self.allocator.create_buffer(&buffer_ci, &alloc_ci)?)
  }

  /// Creates a buffer with TRANSFER_DST and appropriate image type flags
  /// flipped.
  fn create_gpu_image(
    &self, dimens: (u32, u32),
  ) -> SarektResult<(vk::Image, vk_mem::Allocation, vk_mem::AllocationInfo)> {
    let image_ci = vk::ImageCreateInfo::builder()
      .image_type(vk::ImageType::TYPE_2D)
      .usage(vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::SAMPLED)
      .extent(vk::Extent3D {
        width: dimens.0,
        height: dimens.1,
        depth: 1,
      })
      .mip_levels(1) // Not mipmapping.
      .array_layers(1) // Not an array.
      .format(vk::Format::R8G8B8A8_SRGB)
      .tiling(vk::ImageTiling::OPTIMAL) // Texels are laid out in hardware optimal format, not necessarily linearly.
      .initial_layout(vk::ImageLayout::UNDEFINED)
      .sharing_mode(vk::SharingMode::EXCLUSIVE) // Only used by the one queue family.
      .samples(vk::SampleCountFlags::TYPE_1) // Not multisampling, this isn't for an attachment.
      .build();
    let alloc_ci = vk_mem::AllocationCreateInfo {
      usage: vk_mem::MemoryUsage::GpuOnly,
      ..vk_mem::AllocationCreateInfo::default()
    };

    Ok(self.allocator.create_image(&image_ci, &alloc_ci)?)
  }

  // TODO IMAGE MIPMAPPING
  fn transfer_staging_to_gpu_buffer_or_image(
    &self, buffer_size: u64, staging_buffer: vk::Buffer, gpu_buffer_or_image: ImageOrBuffer,
  ) -> SarektResult<()> {
    info!("Initiating transfer command to transfer from staging buffer to device only memory...");
    let transfer_command_buffer = self.transfer_command_buffer;

    let command_begin_info = vk::CommandBufferBeginInfo::builder()
      .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT)
      .build();
    unsafe {
      self
        .logical_device
        .begin_command_buffer(transfer_command_buffer, &command_begin_info)?;

      let (src_queue_family, dst_queue_family) = match gpu_buffer_or_image {
        ImageOrBuffer::Buffer(gpu_buffer) => {
          let copy_region = vk::BufferCopy::builder()
            .src_offset(0)
            .dst_offset(0)
            .size(buffer_size)
            .build();
          self.logical_device.cmd_copy_buffer(
            transfer_command_buffer,
            staging_buffer,
            gpu_buffer,
            &[copy_region],
          );

          (vk::QUEUE_FAMILY_IGNORED, vk::QUEUE_FAMILY_IGNORED)
        }
        ImageOrBuffer::Image(gpu_image, format, extent) => {
          // Transition layout to transfer destination.
          // This wont transfer ownership of queues, no need to check.
          self.insert_layout_transition_barrier(
            transfer_command_buffer,
            gpu_image,
            format,
            vk::ImageLayout::UNDEFINED,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
          )?;

          // Do the copy
          let image_subresource = vk::ImageSubresourceLayers::builder()
            .aspect_mask(vk::ImageAspectFlags::COLOR)
            .mip_level(0)
            .base_array_layer(0)
            .layer_count(1)
            .build();
          let regions = [vk::BufferImageCopy::builder()
            .buffer_offset(0)
            .buffer_row_length(0)
            .buffer_image_height(0)
            .image_subresource(image_subresource)
            .image_offset(vk::Offset3D { x: 0, y: 0, z: 0 })
            .image_extent(extent)
            .build()];
          self.logical_device.cmd_copy_buffer_to_image(
            transfer_command_buffer,
            staging_buffer,
            gpu_image,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            &regions,
          );

          // Transition layout to shader read only.
          self.insert_layout_transition_barrier(
            transfer_command_buffer,
            gpu_image,
            format,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
          )?
        }
      };

      self
        .logical_device
        .end_command_buffer(transfer_command_buffer)?;

      let command_buffers = [transfer_command_buffer];

      let mut submit_info_builder = vk::SubmitInfo::builder().command_buffers(&command_buffers);
      if src_queue_family != dst_queue_family {
        submit_info_builder = submit_info_builder.signal_semaphores(&self.ownership_semaphore)
      }
      let submit_info = submit_info_builder.build();
      self.logical_device.queue_submit(
        self.transfer_command_queue,
        &[submit_info],
        vk::Fence::null(),
      )?;

      self.transfer_queue_ownership_if_necessary(
        &gpu_buffer_or_image,
        src_queue_family,
        dst_queue_family,
      )?;

      self.logical_device.device_wait_idle()?;

      self.logical_device.reset_command_buffer(
        self.transfer_command_buffer,
        vk::CommandBufferResetFlags::empty(),
      )?;
      self.logical_device.reset_command_buffer(
        self.graphics_command_buffer,
        vk::CommandBufferResetFlags::empty(),
      )?;
    }

    Ok(())
  }

  /// When a memory barrier is inserted that transfers queue ownership, the
  /// accept end of the memory barrier must also be run in a command buffer of
  /// the queue taking ownership of the resource.
  unsafe fn transfer_queue_ownership_if_necessary(
    &self, gpu_buffer_or_image: &ImageOrBuffer, src_queue_family: u32, dst_queue_family: u32,
  ) -> SarektResult<()> {
    if src_queue_family == dst_queue_family {
      return Ok(());
    }
    // Do the wait in the dst queue.
    let command_begin_info = vk::CommandBufferBeginInfo::builder()
      .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT)
      .build();
    self
      .logical_device
      .begin_command_buffer(self.graphics_command_buffer, &command_begin_info)?;

    let img = gpu_buffer_or_image.image().unwrap();
    let subresource_range = vk::ImageSubresourceRange::builder()
      .aspect_mask(vk::ImageAspectFlags::COLOR)
      .base_mip_level(0)
      .level_count(1)
      .base_array_layer(0)
      .layer_count(1)
      .build();
    let barriers = [vk::ImageMemoryBarrier::builder()
        .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
        .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
        .src_queue_family_index(src_queue_family) // Transfer ownership to graphics queue if necessary.
        .dst_queue_family_index(dst_queue_family)
        .image(img.0)
        .subresource_range(subresource_range)
        .src_access_mask(vk::AccessFlags::empty())
        .dst_access_mask(vk::AccessFlags::SHADER_READ)
        .build()];
    self.logical_device.cmd_pipeline_barrier(
      self.graphics_command_buffer,
      vk::PipelineStageFlags::TOP_OF_PIPE,
      vk::PipelineStageFlags::FRAGMENT_SHADER,
      vk::DependencyFlags::empty(),
      &[],
      &[],
      &barriers,
    );
    let command_buffers = [self.graphics_command_buffer];
    let submit_info = [vk::SubmitInfo::builder()
      .command_buffers(&command_buffers)
      .wait_semaphores(&self.ownership_semaphore)
      .wait_dst_stage_mask(&[vk::PipelineStageFlags::TOP_OF_PIPE])
      .build()];
    self
      .logical_device
      .end_command_buffer(self.graphics_command_buffer)?;

    self.logical_device.queue_submit(
      self.graphics_command_queue,
      &submit_info,
      vk::Fence::null(),
    )?;

    Ok(())
  }

  fn create_image_view(&self, image: vk::Image) -> SarektResult<vk::ImageView> {
    let subresource_range = vk::ImageSubresourceRange::builder()
      .base_mip_level(0)
      .aspect_mask(vk::ImageAspectFlags::COLOR)
      .level_count(1)
      .base_array_layer(0)
      .layer_count(1)
      .build();
    let image_view_ci = vk::ImageViewCreateInfo::builder()
      .image(image)
      .view_type(vk::ImageViewType::TYPE_2D)
      .format(vk::Format::R8G8B8A8_SRGB)
      .subresource_range(subresource_range)
      .build();
    unsafe {
      Ok(
        self
          .logical_device
          .create_image_view(&image_view_ci, None)?,
      )
    }
  }

  fn create_sampler(
    &self, magnification_filter: MagnificationMinificationFilter,
    minification_filter: MagnificationMinificationFilter, address_u: TextureAddressMode,
    address_v: TextureAddressMode, address_w: TextureAddressMode,
  ) -> SarektResult<vk::Sampler> {
    // TODO CONFIG anisotropy
    // TODO CONFIG border color (as part of TextureAddressMode enum)
    // TODO CONFIG MIPMAPPING
    let mag_filter = match magnification_filter {
      MagnificationMinificationFilter::Linear => vk::Filter::LINEAR,
      MagnificationMinificationFilter::Nearest => vk::Filter::NEAREST,
    };
    let min_filter = match minification_filter {
      MagnificationMinificationFilter::Linear => vk::Filter::LINEAR,
      MagnificationMinificationFilter::Nearest => vk::Filter::NEAREST,
    };
    let address_u = match address_u {
      TextureAddressMode::Repeat => vk::SamplerAddressMode::REPEAT,
      TextureAddressMode::MirroredRepeat => vk::SamplerAddressMode::MIRRORED_REPEAT,
      TextureAddressMode::ClampToEdge => vk::SamplerAddressMode::CLAMP_TO_EDGE,
      TextureAddressMode::MirroredClampToEdge => vk::SamplerAddressMode::MIRROR_CLAMP_TO_EDGE,
    };
    let address_v = match address_v {
      TextureAddressMode::Repeat => vk::SamplerAddressMode::REPEAT,
      TextureAddressMode::MirroredRepeat => vk::SamplerAddressMode::MIRRORED_REPEAT,
      TextureAddressMode::ClampToEdge => vk::SamplerAddressMode::CLAMP_TO_EDGE,
      TextureAddressMode::MirroredClampToEdge => vk::SamplerAddressMode::MIRROR_CLAMP_TO_EDGE,
    };
    let address_w = match address_w {
      TextureAddressMode::Repeat => vk::SamplerAddressMode::REPEAT,
      TextureAddressMode::MirroredRepeat => vk::SamplerAddressMode::MIRRORED_REPEAT,
      TextureAddressMode::ClampToEdge => vk::SamplerAddressMode::CLAMP_TO_EDGE,
      TextureAddressMode::MirroredClampToEdge => vk::SamplerAddressMode::MIRROR_CLAMP_TO_EDGE,
    };
    let sampler_ci = vk::SamplerCreateInfo::builder()
      .mag_filter(mag_filter)
      .min_filter(min_filter)
      .address_mode_u(address_u)
      .address_mode_v(address_v)
      .address_mode_w(address_w)
      .anisotropy_enable(true)
      .max_anisotropy(16f32)
      .border_color(vk::BorderColor::INT_OPAQUE_BLACK)
      .unnormalized_coordinates(false)
      .compare_enable(false)
      .compare_op(vk::CompareOp::ALWAYS)
      .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
      .mip_lod_bias(0.0f32)
      .min_lod(0.0f32)
      .max_lod(0.0f32)
      .build();
    unsafe { Ok(self.logical_device.create_sampler(&sampler_ci, None)?) }
  }

  // TODO IMAGE MIPMAPPING levels as params
  /// Returns the source and destination queue family indices.
  fn insert_layout_transition_barrier(
    &self, transfer_command_buffer: vk::CommandBuffer, image: vk::Image, _format: vk::Format,
    old_layout: vk::ImageLayout, new_layout: vk::ImageLayout,
  ) -> SarektResult<(u32, u32)> {
    let subresource_range = vk::ImageSubresourceRange::builder()
      .aspect_mask(vk::ImageAspectFlags::COLOR)
      .base_mip_level(0)
      .level_count(1)
      .base_array_layer(0)
      .layer_count(1)
      .build();

    let source_stage: vk::PipelineStageFlags;
    let source_access_mask: vk::AccessFlags;
    let destination_stage: vk::PipelineStageFlags;
    let destination_access_mask: vk::AccessFlags;
    let mut src_queue_family = vk::QUEUE_FAMILY_IGNORED;
    let mut dst_queue_family = vk::QUEUE_FAMILY_IGNORED;
    if old_layout == vk::ImageLayout::UNDEFINED
      && new_layout == vk::ImageLayout::TRANSFER_DST_OPTIMAL
    {
      source_access_mask = vk::AccessFlags::empty();
      destination_access_mask = vk::AccessFlags::TRANSFER_WRITE;

      source_stage = vk::PipelineStageFlags::TOP_OF_PIPE;
      destination_stage = vk::PipelineStageFlags::TRANSFER;
    } else if old_layout == vk::ImageLayout::TRANSFER_DST_OPTIMAL
      && new_layout == vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL
    {
      source_access_mask = vk::AccessFlags::TRANSFER_WRITE;
      destination_access_mask = vk::AccessFlags::SHADER_READ;

      source_stage = vk::PipelineStageFlags::TRANSFER;
      destination_stage = vk::PipelineStageFlags::FRAGMENT_SHADER;

      // This will initiate queue ownership transfer if necessary.
      src_queue_family = self.transfer_queue_family;
      dst_queue_family = self.graphics_queue_family;
    } else {
      return Err(SarektError::UnsupportedLayoutTransition);
    }

    let barriers = [vk::ImageMemoryBarrier::builder()
      .old_layout(old_layout)
      .new_layout(new_layout)
      .src_queue_family_index(src_queue_family) // Transfer ownership to graphics queue if necessary.
      .dst_queue_family_index(dst_queue_family)
      .image(image)
      .subresource_range(subresource_range)
      .src_access_mask(source_access_mask)
      .dst_access_mask(destination_access_mask)
      .build()];

    unsafe {
      self.logical_device.cmd_pipeline_barrier(
        transfer_command_buffer,
        source_stage,
        destination_stage,
        vk::DependencyFlags::empty(),
        &[],
        &[],
        &barriers,
      );
    }

    Ok((src_queue_family, dst_queue_family))
  }
}
unsafe impl BufferAndImageLoader for VulkanBufferFunctions {
  type BackendHandle = ResourceWithMemory;
  type UniformBufferDataHandle = Vec<BufferAndMemoryMapped>;
  type UniformBufferHandle = Vec<BufferImageHandle<VulkanBufferFunctions>>;

  unsafe fn cleanup(&self) -> SarektResult<()> {
    if self.ownership_semaphore[0] != vk::Semaphore::null() {
      return Ok(
        self
          .logical_device
          .destroy_semaphore(self.ownership_semaphore[0], None),
      );
    }

    Ok(())
  }

  /// I could create a buffer myself and allocate memory with VMA, but their
  /// recomended approach is to allow the library to create a buffer and bind
  /// the memory, effectively replacing all of this code.  See their [docs](https://gpuopen-librariesandsdks.github.io/VulkanMemoryAllocator/html/choosing_memory_type.html).
  /// To see manual creation of this (with super naive memory allocation) see
  /// this file at tag 17_vertex_buffer_creation.
  ///
  /// So in summary, VMA handles creating the buffer, finding the appropriate
  /// memory type index, allocating the memory, and binding the buffer to the
  /// memory.
  ///
  /// The way this function operates to keep things as efficient as possible at
  /// GPU runtime is to copy into a staging buffer and initiate a transfer
  /// operation on the GPU to a more efficient device only GPU memory buffer.
  fn load_buffer_with_staging<BufElem: Sized + Copy>(
    &self, buffer_type: BufferType, buffer: &[BufElem],
  ) -> SarektResult<ResourceWithMemory> {
    let buffer_size =
      (std::mem::size_of::<BufElem>() as vk::DeviceSize) * buffer.len() as vk::DeviceSize;

    // Create the staging buffer and memory.
    let (staging_buffer, staging_allocation, _) = self.create_staging_buffer(buffer_size)?;

    // Copy over all the bytes from host memory to mapped device memory
    let data = self.allocator.map_memory(&staging_allocation)? as *mut BufElem;
    unsafe {
      data.copy_from(buffer.as_ptr(), buffer_size as usize);
    }
    self.allocator.unmap_memory(&staging_allocation)?;

    let (gpu_buffer, gpu_allocation, _gpu_allocation_info) =
      self.create_gpu_buffer(buffer_type, buffer_size)?;

    self.transfer_staging_to_gpu_buffer_or_image(
      buffer_size,
      staging_buffer,
      ImageOrBuffer::Buffer(gpu_buffer),
    )?;

    // Staging buffer no longer needed, delete it.
    info!("Destroying staging buffer and memory...");
    self
      .allocator
      .destroy_buffer(staging_buffer, &staging_allocation)?;

    // If this is an index buffer, keep track of the size of the elements (16 or
    // 32).
    let index_buffer_elem_size = match buffer_type {
      BufferType::Index(size) => Some(size),
      _ => None,
    };

    Ok(ResourceWithMemory::Buffer(BufferAndMemory {
      buffer: gpu_buffer,
      length: buffer.len() as u32,
      index_buffer_elem_size,
      allocation: gpu_allocation,
    }))
  }

  fn load_buffer_without_staging<BufElem: Sized + Copy>(
    &self, buffer_type: BufferType, buffer: &[BufElem],
  ) -> SarektResult<ResourceWithMemory> {
    let buffer_size =
      (std::mem::size_of::<BufElem>() as vk::DeviceSize) * buffer.len() as vk::DeviceSize;

    // There is only one buffer, no staging needed, but we will initialze the
    // values.
    let (vk_buffer, allocation, _) =
      self.create_cpu_accessible_buffer(buffer_size, usage_flags_from_buffer_type(buffer_type))?;

    // Copy over all the bytes from host memory to mapped device memory
    let data = self.allocator.map_memory(&allocation)? as *mut BufElem;
    unsafe {
      data.copy_from(buffer.as_ptr(), buffer_size as usize);
    }
    self.allocator.unmap_memory(&allocation)?;

    // If this is an index buffer, keep track of the size of the elements (16 or
    // 32).
    let index_buffer_elem_size = match buffer_type {
      BufferType::Index(size) => Some(size),
      _ => None,
    };

    Ok(ResourceWithMemory::Buffer(BufferAndMemory {
      buffer: vk_buffer,
      length: buffer.len() as u32,
      index_buffer_elem_size,
      allocation,
    }))
  }

  /// The procedure for loading an image in vulkan could use a staging image,
  /// but its just as well we use a staging buffer, which is easier and [could even be faster](https://developer.nvidia.com/vulkan-memory-management)
  /// TODO IMAGES more general load_image functoin that selects best format that
  /// is available. TODO IMAGES MIPMAPPING
  fn load_image_with_staging_rgba32(
    &self, pixels: impl ImageData, magnification_filter: MagnificationMinificationFilter,
    minification_filter: MagnificationMinificationFilter, address_u: TextureAddressMode,
    address_v: TextureAddressMode, address_w: TextureAddressMode,
  ) -> SarektResult<ResourceWithMemory> {
    let dimens = pixels.dimensions();
    let formatted_pixels = pixels.into_rbga_32();

    info!(
      "Loading texture with dimensions {:?}, and {} bytes",
      dimens,
      formatted_pixels.len()
    );

    let (staging_buffer, staging_allocation, _) =
      self.create_staging_buffer(formatted_pixels.len() as u64)?;

    let data = self.allocator.map_memory(&staging_allocation)?;
    unsafe {
      data.copy_from(formatted_pixels.as_ptr(), formatted_pixels.len());
    }
    self.allocator.unmap_memory(&staging_allocation)?;

    let (image, image_allocation, _) = self.create_gpu_image(dimens)?;

    let extent = vk::Extent3D {
      width: dimens.0,
      height: dimens.1,
      depth: 1,
    };
    self.transfer_staging_to_gpu_buffer_or_image(
      formatted_pixels.len() as u64,
      staging_buffer,
      ImageOrBuffer::Image(image, vk::Format::R8G8B8A8_SRGB, extent),
    )?;

    info!("Destroying staging buffer and memory...");
    self
      .allocator
      .destroy_buffer(staging_buffer, &staging_allocation)?;

    let image_view = self.create_image_view(image)?;
    let sampler = self.create_sampler(
      magnification_filter,
      minification_filter,
      address_u,
      address_v,
      address_w,
    )?;

    Ok(ResourceWithMemory::Image(ImageAndMemory {
      allocation: image_allocation,
      length: formatted_pixels.len() as u32,
      image_and_view: unsafe { ImageAndView::new(image, image_view) },
      sampler,
    }))
  }

  fn delete_buffer_or_image(&self, handle: ResourceWithMemory) -> SarektResult<()> {
    info!(
      "Deleting image or buffer and associated memory {:?}...",
      handle
    );

    match handle {
      ResourceWithMemory::Buffer(handle) => self
        .allocator
        .destroy_buffer(handle.buffer, &handle.allocation)?,
      ResourceWithMemory::Image(handle) => {
        unsafe {
          self.logical_device.destroy_sampler(handle.sampler, None);
          self
            .logical_device
            .destroy_image_view(handle.image_and_view.view, None);
        }
        self
          .allocator
          .destroy_image(handle.image_and_view.image, &handle.allocation)?;
      }
    }

    Ok(())
  }
}

/// A Vulkan Buffer or Image.
#[derive(Copy, Clone, Debug)]
pub enum ResourceWithMemory {
  Buffer(BufferAndMemory),
  Image(ImageAndMemory),
}
impl ResourceWithMemory {
  pub fn buffer(self) -> SarektResult<BufferAndMemory> {
    match self {
      ResourceWithMemory::Buffer(buffer) => Ok(buffer),
      _ => Err(SarektError::IncorrectResourceType),
    }
  }

  pub fn image(self) -> SarektResult<ImageAndMemory> {
    match self {
      ResourceWithMemory::Image(image) => Ok(image),
      _ => Err(SarektError::IncorrectResourceType),
    }
  }
}

#[derive(Copy, Clone, Debug)]
pub struct BufferAndMemory {
  pub(crate) buffer: vk::Buffer,
  pub(crate) length: u32,
  /// Only present if this is an index buffer.
  pub(crate) index_buffer_elem_size: Option<IndexBufferElemSize>,
  pub(crate) allocation: vk_mem::Allocation,
}

/// Stores the mapped pointer along with the allocation.  There is no need
/// tformbo implement drop here because when the memory itself is dropped, it is
/// freed. According to the spec in `vkFreeMemory`'s docs "If a memeory object
/// is mapped at the tiem it is freed, it is implicitly unmapped"
#[derive(Copy, Clone, Debug)]
pub struct BufferAndMemoryMapped {
  pub(crate) buffer_and_memory: BufferAndMemory,
  pub(crate) ptr: *mut u8,
}
impl BufferAndMemoryMapped {
  pub(crate) fn new(buffer_and_memory: BufferAndMemory, ptr: *mut u8) -> Self {
    Self {
      buffer_and_memory,
      ptr,
    }
  }
}

fn usage_flags_from_buffer_type(buffer_type: BufferType) -> vk::BufferUsageFlags {
  match buffer_type {
    BufferType::Vertex => vk::BufferUsageFlags::VERTEX_BUFFER,
    BufferType::Index(_) => vk::BufferUsageFlags::INDEX_BUFFER,
    BufferType::Uniform => vk::BufferUsageFlags::UNIFORM_BUFFER,
  }
}

/// Just as BufferAndMemory works, this is an Image and it's bound allocated
/// memory.
#[derive(Copy, Clone, Debug)]
pub struct ImageAndMemory {
  pub(crate) image_and_view: ImageAndView,
  pub(crate) length: u32,
  pub(crate) allocation: vk_mem::Allocation,
  pub(crate) sampler: vk::Sampler,
}

/// Allow the ResourceType(Image or Buffer) to be the backend for images and
/// buffers.
unsafe impl BackendHandleTrait for ResourceWithMemory {}

/// Whether the operation will concern a buffer or an image.  Image includes its
/// extent.
enum ImageOrBuffer {
  Buffer(vk::Buffer),
  Image(vk::Image, vk::Format, vk::Extent3D),
}
impl ImageOrBuffer {
  fn image(&self) -> SarektResult<(vk::Image, vk::Format, vk::Extent3D)> {
    match *self {
      ImageOrBuffer::Image(image, format, extent) => Ok((image, format, extent)),
      _ => Err(SarektError::IncorrectResourceType),
    }
  }
}