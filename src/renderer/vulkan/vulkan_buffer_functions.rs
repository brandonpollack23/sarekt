use crate::{
  error::SarektResult,
  renderer::buffers::{BufferBackendHandle, BufferLoader, BufferType},
};
use ash::vk;
use log::info;
use std::{ffi::c_void, sync::Arc};

/// TODO PERFORMANCE stage buffer allocations to be transfered in one staging
/// buffer commit load operation instead of doing each one seperate and waiting.

/// Vulkan implementation of [BufferLoader](trait.BufferLoader.html).
#[derive(Clone)]
pub struct VulkanBufferFunctions {
  allocator: Arc<vk_mem::Allocator>,
}
impl VulkanBufferFunctions {
  pub fn new(allocator: Arc<vk_mem::Allocator>) -> Self {
    Self { allocator }
  }
}
unsafe impl BufferLoader for VulkanBufferFunctions {
  type BBH = BufferAndMemory;

  fn load_buffer<BufElem: Sized>(
    &self, buffer_type: BufferType, buffer: &[BufElem],
  ) -> SarektResult<Self::BBH> {
    // I could create a buffer myself and allocate memory with VMA, but their
    // recomended approach is to allow the library to create a buffer and bind the
    // memory, effectively replacing all of this code.  See their [docs](https://gpuopen-librariesandsdks.github.io/VulkanMemoryAllocator/html/choosing_memory_type.html).
    // To see manual creation of this (with super naive memory allocation) see this
    // file at tag 17_vertex_buffer_creation.

    // So in summary, VMA handles creating the buffer, finding the appropriate
    // memory type index, allocating the memory, and binding the buffer to the
    // memory.

    // Create the buffer and memory.
    let buffer_size =
      (std::mem::size_of::<BufElem>() as vk::DeviceSize) * buffer.len() as vk::DeviceSize;
    let buffer_usage = match buffer_type {
      BufferType::Vertex => vk::BufferUsageFlags::VERTEX_BUFFER,
      BufferType::Index => vk::BufferUsageFlags::INDEX_BUFFER,
    };
    let buffer_ci = vk::BufferCreateInfo::builder()
      .size(buffer_size)
      .usage(buffer_usage)
      .sharing_mode(vk::SharingMode::EXCLUSIVE)
      .build();

    let alloc_create_info = vk_mem::AllocationCreateInfo {
      usage: vk_mem::MemoryUsage::CpuToGpu, /* All the required and preferred flags such as
                                             * HOST_VISIBLE, HOST_COHERENT, memory type bits, etc
                                             * are automagically configured by this usage flag.
                                             * Which works for my use case */
      ..vk_mem::AllocationCreateInfo::default()
    };
    let (vulkan_buffer, allocation, allocation_info) = self
      .allocator
      .create_buffer(&buffer_ci, &alloc_create_info)?;

    // TODO CRITICAL staging buffer.

    // Copy over all the bytes from host memory to mapped device memory
    let data = self.allocator.map_memory(&allocation)? as *mut BufElem;
    unsafe {
      data.copy_from(buffer.as_ptr(), allocation_info.get_size());
    }
    self.allocator.unmap_memory(&allocation)?;

    unsafe {
      Ok(BufferAndMemory {
        buffer: vulkan_buffer,
        offset: allocation_info.get_offset() as u64,
        length: buffer.len() as u32,
        memory: std::mem::transmute(allocation),
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
