use crate::error::{SarektError, SarektResult};
use log::warn;
use slotmap::{DefaultKey, SlotMap};
use std::{
  fmt::Debug,
  sync::{Arc, RwLock},
};

/// A type that can be used to retrieve a buffer from the renderer and
/// BufferStore that will destroy the shader when it goes out of scope.
///
/// As always, In order to pass this around with multiple ownership, wrap it in
/// an Arc.
pub struct BufferHandle<BL>
where
  BL: BufferLoader,
  BL::BBH: BufferBackendHandle + Copy + Debug,
{
  inner_key: DefaultKey,
  buffer_store: Arc<RwLock<BufferStore<BL>>>,
}
impl<BL> Drop for BufferHandle<BL>
where
  BL: BufferLoader,
  BL::BBH: BufferBackendHandle + Copy + Debug,
{
  fn drop(&mut self) {
    let mut buffer_store_guard = self
      .buffer_store
      .write()
      .expect("Could not unlock BufferStore due to previous panic");
    match buffer_store_guard.destroy_buffer(self.inner_key) {
      // Already deleted, likely shutting down. Nothing to do.
      Err(SarektError::UnknownBuffer) => {}
      Err(e) => warn!("buffer not destroyed, maybe it was already? Error: {:?}", e),
      Ok(()) => {}
    }
  }
}

/// Which kind of buffer is this.  The Renderer and DrawableObject wil use this
/// information to utilize it correctly.
/// TODO TEXTURES, check vk::BufferUsageFlags for other types.
#[derive(Copy, Clone, Debug)]
pub enum BufferType {
  Vertex,
  Uniform,
  Index(IndexBufferElemSize),
}

#[derive(Copy, Clone, Debug)]
pub enum IndexBufferElemSize {
  UInt16,
  UInt32,
}

/// The handle that reperesents a buffer in the backend.
///
/// Unsafe because:
/// This must specifically be the handle used to delete your
/// buffer in the backend/GPU in [ShaderLoader](trait.BufferLoader.html).
pub unsafe trait BufferBackendHandle: Copy {}

/// A trait used by each implementation in order to load buffers in their own
/// way.
///
/// Unsafe because:
/// * The lifetimes of the functions to create them (which are
/// usually dynamically loaded) must outlive the Loader itself.
///
/// * BBH must be an implementer of
///   [ShaderBackendHandle](trait.ShaderBackendHandle.html)
///
///  * It is the responsibility of the implementor to drop anything loaded using
///    delete_buffer cleanly on all elements, if the ShaderHandle dropping
///    doesn't handle it.
pub unsafe trait BufferLoader {
  // Buffer Backend Handle
  type BBH;

  fn load_buffer<BufElem: Sized>(
    &self, buffer_type: BufferType, buffer: &[BufElem],
  ) -> SarektResult<Self::BBH>;

  fn delete_buffer(&self, handle: Self::BBH) -> SarektResult<()>;
}

/// A storage for all buffers to be loaded or destroyed from.  Returns a handle
/// that can be used to retrieve the associated buffer, which includes it's type
/// and it's handle to whichever backend you're using.
pub(crate) struct BufferStore<BL>
where
  BL: BufferLoader,
  BL::BBH: BufferBackendHandle + Copy + Debug,
{
  loaded_buffers: SlotMap<DefaultKey, Buffer<BL::BBH>>,
  buffer_loader: BL,
}
impl<BL> BufferStore<BL>
where
  BL: BufferLoader,
  BL::BBH: BufferBackendHandle + Copy + Debug,
{
  pub(crate) fn new(buffer_loader: BL) -> Self {
    Self {
      loaded_buffers: SlotMap::new(),
      buffer_loader,
    }
  }

  /// Load a buffer and allocate memory into the backend/GPU and return a
  /// handle.
  pub(crate) fn load_buffer<BufElem: Sized>(
    this: &Arc<RwLock<Self>>, buffer_type: BufferType, buffer: &[BufElem],
  ) -> SarektResult<BufferHandle<BL>> {
    let mut buffer_store = this
      .write()
      .expect("Could not unlock BufferStore due to previous panic");

    let buffer_backend_handle = buffer_store
      .buffer_loader
      .load_buffer(buffer_type, buffer)?;
    let inner_key = buffer_store
      .loaded_buffers
      .insert(Buffer::new(buffer_backend_handle, buffer_type));

    Ok(BufferHandle {
      inner_key,
      buffer_store: this.clone(),
    })
  }

  /// Destroy a buffer and free the memory associated with it from the
  /// backend/GPU.
  fn destroy_buffer(&mut self, inner_key: DefaultKey) -> SarektResult<()> {
    let buffer = self.loaded_buffers.remove(inner_key);
    if buffer.is_none() {
      return Err(SarektError::UnknownBuffer);
    }
    self
      .buffer_loader
      .delete_buffer(buffer.unwrap().buffer_handle)?;
    Ok(())
  }

  /// Does what it says on the tin, but for all the buffers.  See
  /// destroy_buffers.
  pub(crate) unsafe fn destroy_all_buffers(&mut self) {
    for buffer in self.loaded_buffers.iter() {
      if let Err(err) = self.buffer_loader.delete_buffer(buffer.1.buffer_handle) {
        warn!(
          "Buffer not destroyed, maybe it was already? Error: {:?}",
          err
        );
      }
    }
  }

  /// Retrieves the buffer associated with the handle to be bound etc.
  pub(crate) fn get_buffer(&self, handle: &BufferHandle<BL>) -> SarektResult<&Buffer<BL::BBH>> {
    let buffer = self.loaded_buffers.get(handle.inner_key);
    if let Some(buffer) = buffer {
      return Ok(buffer);
    }
    Err(SarektError::UnknownBuffer)
  }
}

/// The Buffer in terms of its backend handle and the type of buffer.
#[derive(Copy, Clone, Debug)]
pub(crate) struct Buffer<BBH: BufferBackendHandle + Copy> {
  pub buffer_handle: BBH,
  pub buffer_type: BufferType,
}
impl<BBH: BufferBackendHandle + Copy> Buffer<BBH> {
  fn new(buffer_handle: BBH, buffer_type: BufferType) -> Self {
    Self {
      buffer_handle,
      buffer_type,
    }
  }
}
