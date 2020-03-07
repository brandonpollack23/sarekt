use crate::{
  error::{SarektError, SarektResult},
  renderer::buffers::{BufferBackendHandle, BufferLoader, BufferType},
};
use ash::{
  version::{DeviceV1_0, InstanceV1_0},
  vk, Device, Instance,
};
use log::info;
use std::sync::Arc;

/// TODO PERFORMANCE stage buffer allocations to be transfered in one staging
/// buffer commit load operation instead of doing each one seperate and waiting.

/// Vulkan implementation of [BufferLoader](trait.BufferLoader.html).
#[derive(Clone)]
pub struct VulkanBufferFunctions {
  instance: Arc<Instance>,
  physical_device: vk::PhysicalDevice,
  logical_device: Arc<Device>,
}
impl VulkanBufferFunctions {
  pub fn new(
    instance: Arc<Instance>, physical_device: vk::PhysicalDevice, logical_device: Arc<Device>,
  ) -> Self {
    Self {
      instance,
      physical_device,
      logical_device,
    }
  }
}
impl VulkanBufferFunctions {
  /// Finds the appropriate memory type (by heap index) for the given type.
  /// This will find a memory heap index to allocate from that can allocate the
  /// correct memory type specified by suitable_type_filter and has the
  /// requested properties, such as host visibility.
  fn find_memory_type(
    &self, suitable_type_filter: u32, required_properties: vk::MemoryPropertyFlags,
  ) -> SarektResult<u32> {
    let mem_properties = unsafe {
      self
        .instance
        .get_physical_device_memory_properties(self.physical_device)
    };
    for mem_type_index in 0..mem_properties.memory_type_count as usize {
      // TODO PERFORMANCE select more appropriate heaps by checking
      // mem_properties.mem_types[mem_type_index].heap_index
      let can_alloc_suitable_memory_type = (suitable_type_filter & (1 << mem_type_index)) != 0;

      let has_required_memory_properties =
        (mem_properties.memory_types[mem_type_index].property_flags & required_properties)
          == required_properties;

      if can_alloc_suitable_memory_type && has_required_memory_properties {
        return Ok(mem_type_index as u32);
      }
    }

    Err(SarektError::NoSuitableMemoryHeap)
  }
}
unsafe impl BufferLoader for VulkanBufferFunctions {
  type BBH = BufferAndMemory;

  fn load_buffer<BufElem: Sized>(
    &self, buffer_type: BufferType, buffer: &[BufElem],
  ) -> SarektResult<Self::BBH> {
    // Create the buffer.
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

    let vulkan_buffer = unsafe { self.logical_device.create_buffer(&buffer_ci, None)? };

    // Allocate Memory for the buffer.
    let mem_reqs = unsafe {
      self
        .logical_device
        .get_buffer_memory_requirements(vulkan_buffer)
    };
    let heap_index = self.find_memory_type(
      mem_reqs.memory_type_bits,
      vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
    )?;

    let alloc_info = vk::MemoryAllocateInfo::builder()
      .allocation_size(mem_reqs.size) // Size might not be equal to buffer size, so use what Vulkan says to use.
      .memory_type_index(heap_index)
      .build();

    // TODO CRITICAL NEXT vulkan memory allocator, offset in bind buffer memory  and
    // map_memory will change too! Also deleter function, need to save offset in
    // BufferAndMemory.
    let buffer_memory = unsafe { self.logical_device.allocate_memory(&alloc_info, None)? };
    unsafe {
      self
        .logical_device
        .bind_buffer_memory(vulkan_buffer, buffer_memory, 0)?
    };

    // TODO CRITICAL staging buffer.
    // Copy over all the bytes from host memory to mapped device memory
    unsafe {
      let data = self.logical_device.map_memory(
        buffer_memory,
        0,
        buffer_ci.size,
        vk::MemoryMapFlags::empty(),
      )? as *mut BufElem;

      data.copy_from(buffer.as_ptr(), buffer_ci.size as usize);

      self.logical_device.unmap_memory(buffer_memory);
    }

    Ok(BufferAndMemory {
      buffer: vulkan_buffer,
      offset: 0u64,
      length: buffer.len() as u32,
      memory: buffer_memory,
    })
  }

  fn delete_buffer(&self, handle: Self::BBH) -> SarektResult<()> {
    info!("Deleting buffer and memory {:?}...", handle);
    unsafe { self.logical_device.free_memory(handle.memory, None) };
    unsafe { self.logical_device.destroy_buffer(handle.buffer, None) };
    Ok(())
  }
}

#[derive(Copy, Clone, Debug)]
pub struct BufferAndMemory {
  pub(crate) buffer: vk::Buffer,
  pub(crate) offset: u64,
  pub(crate) length: u32,
  memory: vk::DeviceMemory,
}

/// Allow vk::ShaderModule to be a backend handle for the
/// [ShaderStore](struct.ShaderStore.html).
unsafe impl BufferBackendHandle for BufferAndMemory {}
