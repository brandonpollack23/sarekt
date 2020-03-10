use crate::{
  error::SarektResult,
  renderer::buffers::{BufferBackendHandle, BufferLoader, BufferType},
};
use ash::{version::DeviceV1_0, vk, Device};
use log::info;
use std::{ffi::c_void, sync::Arc};

/// TODO PERFORMANCE special allocation flags for staging buffer?

/// TODO PERFORMANCE stage buffer allocations to be transfered in one staging
/// buffer commit load operation instead of doing each one seperate and waiting.

/// TODO PERFORMANCE defragmentation of VMA

/// Vulkan implementation of [BufferLoader](trait.BufferLoader.html).
#[derive(Clone)]
pub struct VulkanBufferFunctions {
  logical_device: Arc<Device>,
  allocator: Arc<vk_mem::Allocator>,
  transfer_command_buffer: vk::CommandBuffer,
  transfer_command_queue: vk::Queue,
  graphics_queue_family: u32,
  transfer_queue_family: u32,
}
impl VulkanBufferFunctions {
  pub fn new(
    logical_device: Arc<Device>, allocator: Arc<vk_mem::Allocator>, graphics_queue_family: u32,
    transfer_queue_family: u32, transfer_command_pool: vk::CommandPool,
    transfer_command_queue: vk::Queue,
  ) -> SarektResult<Self> {
    let command_buffer_alloc_info = vk::CommandBufferAllocateInfo::builder()
      .level(vk::CommandBufferLevel::PRIMARY)
      .command_pool(transfer_command_pool)
      .command_buffer_count(1)
      .build();
    let transfer_command_buffer =
      unsafe { logical_device.allocate_command_buffers(&command_buffer_alloc_info)?[0] };

    Ok(Self {
      logical_device,
      allocator,
      transfer_command_buffer,
      transfer_command_queue,
      graphics_queue_family,
      transfer_queue_family,
    })
  }

  /// Creates a CPU visible staging buffer that has the TRANSFER_SRC usage bit
  /// flipped.
  fn create_staging_buffer(
    &self, buffer_size: u64,
  ) -> SarektResult<(vk::Buffer, vk_mem::Allocation, vk_mem::AllocationInfo)> {
    info!(
      "Creating staging buffer and memory of size {} to transfer from CPU memory...",
      buffer_size
    );
    let staging_buffer_usage = vk::BufferUsageFlags::TRANSFER_SRC;
    let staging_buffer_ci = vk::BufferCreateInfo::builder()
      .size(buffer_size)
      .usage(staging_buffer_usage)
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
    let buffer_usage = vk::BufferUsageFlags::TRANSFER_DST
      | match buffer_type {
        BufferType::Vertex => vk::BufferUsageFlags::VERTEX_BUFFER,
        BufferType::Index => vk::BufferUsageFlags::INDEX_BUFFER,
      };
    let sharing_mode = if self.graphics_queue_family == self.transfer_queue_family {
      vk::SharingMode::EXCLUSIVE
    } else {
      vk::SharingMode::CONCURRENT
    };
    let buffer_ci = vk::BufferCreateInfo::builder()
      .size(buffer_size)
      .usage(buffer_usage)
      .sharing_mode(sharing_mode)
      .queue_family_indices(&[self.graphics_queue_family, self.transfer_queue_family]) // Ignored if exclusive.
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

  fn transfer_staging_to_gpu_buffer(
    &self, buffer_size: u64, staging_buffer: vk::Buffer,
    transfer_allocation_info: &vk_mem::AllocationInfo, gpu_buffer: vk::Buffer,
    gpu_allocation_info: &vk_mem::AllocationInfo,
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
      let copy_region = vk::BufferCopy::builder()
        .src_offset(transfer_allocation_info.get_offset() as u64)
        .dst_offset(gpu_allocation_info.get_offset() as u64)
        .size(buffer_size)
        .build();
      self.logical_device.cmd_copy_buffer(
        transfer_command_buffer,
        staging_buffer,
        gpu_buffer,
        &[copy_region],
      );

      self
        .logical_device
        .end_command_buffer(transfer_command_buffer)?;

      let submit_info = vk::SubmitInfo::builder()
        .command_buffers(&[transfer_command_buffer])
        .build();
      self.logical_device.queue_submit(
        self.transfer_command_queue,
        &[submit_info],
        vk::Fence::null(),
      )?;

      // TODO PERFORMANCE use fence?
      self.logical_device.device_wait_idle()?;

      self.logical_device.reset_command_buffer(
        self.transfer_command_buffer,
        vk::CommandBufferResetFlags::empty(),
      )?;
    }

    Ok(())
  }
}
unsafe impl BufferLoader for VulkanBufferFunctions {
  type BBH = BufferAndMemory;

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
  fn load_buffer<BufElem: Sized>(
    &self, buffer_type: BufferType, buffer: &[BufElem],
  ) -> SarektResult<Self::BBH> {
    let buffer_size =
      (std::mem::size_of::<BufElem>() as vk::DeviceSize) * buffer.len() as vk::DeviceSize;

    // Create the staging buffer and memory.
    let (staging_buffer, staging_allocation, staging_allocation_info) =
      self.create_staging_buffer(buffer_size)?;

    // Copy over all the bytes from host memory to mapped device memory
    let data = self.allocator.map_memory(&staging_allocation)? as *mut BufElem;
    unsafe {
      data.copy_from(buffer.as_ptr(), staging_allocation_info.get_size());
    }
    self.allocator.unmap_memory(&staging_allocation)?;

    let (gpu_buffer, gpu_allocation, gpu_allocation_info) =
      self.create_gpu_buffer(buffer_type, buffer_size)?;

    self.transfer_staging_to_gpu_buffer(
      buffer_size,
      staging_buffer,
      &staging_allocation_info,
      gpu_buffer,
      &gpu_allocation_info,
    )?;

    // Staging buffer no longer needed, delete it.
    info!("Destroying staging buffer and memory...");
    self
      .allocator
      .destroy_buffer(staging_buffer, &staging_allocation)?;

    unsafe {
      Ok(BufferAndMemory {
        buffer: gpu_buffer,
        offset: gpu_allocation_info.get_offset() as u64,
        length: buffer.len() as u32,
        memory: std::mem::transmute(gpu_allocation),
      })
    }
  }

  fn delete_buffer(&self, handle: Self::BBH) -> SarektResult<()> {
    info!("Deleting buffer and memory {:?}...", handle);
    unsafe {
      self
        .allocator
        .destroy_buffer(handle.buffer, &std::mem::transmute(handle.memory))
        .expect("Could not destroy VMA buffer");
    }
    Ok(())
  }
}

#[derive(Copy, Clone, Debug)]
pub struct BufferAndMemory {
  pub(crate) buffer: vk::Buffer,
  pub(crate) offset: u64,
  pub(crate) length: u32,
  // TODO CRITICAL Super unsafe hack to get around vk_mem::Allocation not implementing Copy.
  memory: *mut c_void,
}

/// Allow vk::ShaderModule to be a backend handle for the
/// [ShaderStore](struct.ShaderStore.html).
unsafe impl BufferBackendHandle for BufferAndMemory {}
