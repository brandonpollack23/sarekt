use crate::{
  error::{SarektError, SarektResult},
  image_data::{ImageData, ImageDataFormat},
};
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
pub struct BufferImageHandle<BL>
where
  BL: BufferAndImageLoader,
  BL::BackendHandle: BackendHandleTrait + Copy + Debug,
{
  inner_key: DefaultKey,
  resource_type: ResourceType,
  buffer_store: Arc<RwLock<BufferImageStore<BL>>>,
}
impl<BL> Drop for BufferImageHandle<BL>
where
  BL: BufferAndImageLoader,
  BL::BackendHandle: BackendHandleTrait + Copy + Debug,
{
  fn drop(&mut self) {
    let mut buffer_store_guard = self
      .buffer_store
      .write()
      .expect("Could not unlock BufferStore due to previous panic");

    let result = match self.resource_type {
      ResourceType::Buffer(_) => buffer_store_guard.destroy_buffer(self.inner_key),
      ResourceType::Image => buffer_store_guard.destroy_image(self.inner_key),
    };

    match result {
      // Already deleted, likely shutting down. Nothing to do.
      Err(SarektError::UnknownResource) => {}
      Err(e) => warn!(
        "resource not destroyed, maybe it was already? Error: {:?}",
        e
      ),
      Ok(()) => {}
    }
  }
}

/// Which kind of buffer or image is this.  The Renderer and DrawableObject wil
/// use this information to utilize it correctly.
/// TODO TEXTURES, check vk::BufferUsageFlags for other types.
#[derive(Copy, Clone, Debug)]
pub enum ResourceType {
  Image,
  Buffer(BufferType),
}

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

/// The handle that represents a buffer or image in the backend.
///
/// Unsafe because:
/// This must specifically be the handle used to delete your
/// image or buffer in the backend/GPU in
/// [ShaderLoader](trait.BufferLoader.html).
pub unsafe trait BackendHandleTrait: Copy {}

/// A special handle for uniform buffers.  On some backends there are special
/// cases needed to be handled more so than other (vertex and index) buffers.
/// BufferLoader is the backing loader for the buffer and BufElem is the type
/// that the buffer contains.
///
/// For example, on Vulkan more than one frame can be in flight so this needs to
/// actually create uniform buffers for each framebuffer.
#[derive(Clone, Debug)]
pub struct UniformBufferHandle<BL: BufferAndImageLoader, BufElem: Sized + Copy> {
  pub uniform_buffer_backend_handle: BL::UniformBufferHandle,
  _marker: std::marker::PhantomData<BufElem>,
}
impl<BL: BufferAndImageLoader, BufElem: Sized + Copy> UniformBufferHandle<BL, BufElem> {
  pub fn new(uniform_buffer_backend_handle: BL::UniformBufferHandle) -> Self {
    Self {
      uniform_buffer_backend_handle,
      _marker: std::marker::PhantomData,
    }
  }
}

/// A trait used by each implementation in order to load buffers and images in
/// their own way.
///
/// Unsafe because:
/// * The lifetimes of the functions to create them (which are
/// usually dynamically loaded) must outlive the Loader itself.
///
/// * BackendHandle must be an implementer of
///   [ShaderBackendHandle](trait.ShaderBackendHandle.html)
///
///  * It is the responsibility of the implementor to drop anything loaded using
///    delete_buffer_or_image cleanly on all elements, if the ShaderHandle
///    dropping doesn't handle it.
///
///  * Cleanup must be called if the application controls object lifetimes.
pub unsafe trait BufferAndImageLoader {
  type BackendHandle;
  type UniformBufferDataHandle: Debug;
  type UniformBufferHandle;

  /// TODO PERFORMANCE some platforms might not actually ever benefit from
  /// staging.  Detect?

  /// # Safety
  /// Must call before exiting.
  unsafe fn cleanup(&self) -> SarektResult<()>;

  /// Loads a buffer using a staging buffer and then transfers it into GPU only
  /// memory for efficiency.
  fn load_buffer_with_staging<BufElem: Sized + Copy>(
    &self, buffer_type: BufferType, buffer: &[BufElem],
  ) -> SarektResult<Self::BackendHandle>;

  /// Loads a buffer without staging.  Frequently updated buffers will just be
  /// slowed down by waiting for transfer, such as uniform buffers.
  fn load_buffer_without_staging<BufElem: Sized + Copy>(
    &self, buffer_type: BufferType, buffer: &[BufElem],
  ) -> SarektResult<Self::BackendHandle>;

  /// Same as `load_buffer_with_staging` but loads an r8g8b8a8 32 bit format
  /// image instead.
  fn load_image_with_staging_initialization(
    &self, pixels: impl ImageData, magnification_filter: MagnificationMinificationFilter,
    minification_filter: MagnificationMinificationFilter, address_x: TextureAddressMode,
    address_y: TextureAddressMode, address_z: TextureAddressMode,
  ) -> SarektResult<Self::BackendHandle>;

  /// Loads an image, much like `load_image_with_staging_initialization`, but
  /// does not give it any initial value, only a size and format.  This is
  /// useful for initializing internally used attachments, depth buffers, etc.
  fn create_uninitialized_image(
    &self, dimensions: (u32, u32), format: ImageDataFormat,
  ) -> SarektResult<Self::BackendHandle>;

  /// Deletes that resource, baby!
  fn delete_buffer_or_image(&self, handle: Self::BackendHandle) -> SarektResult<()>;
}

/// A storage for all buffers to be loaded or destroyed from.  Returns a handle
/// that can be used to retrieve the associated buffer, which includes it's type
/// and it's handle to whichever backend you're using.
pub struct BufferImageStore<BL>
where
  BL: BufferAndImageLoader,
  BL::BackendHandle: BackendHandleTrait + Copy + Debug,
{
  loaded_buffers_and_images: SlotMap<DefaultKey, BufferOrImage<BL::BackendHandle>>,
  buffer_image_loader: BL,
}
impl<BL> BufferImageStore<BL>
where
  BL: BufferAndImageLoader,
  BL::BackendHandle: BackendHandleTrait + Copy + Debug,
{
  pub fn new(buffer_loader: BL) -> Self {
    Self {
      loaded_buffers_and_images: SlotMap::new(),
      buffer_image_loader: buffer_loader,
    }
  }

  /// Must be called by the backend when cleaning up all resources, if they are
  /// managed by the application (as in Vulkan/D3D12).
  ///
  /// # Safety
  /// Unsafe because afterwards the object becomes invalid and should not be
  /// used again.
  pub unsafe fn cleanup(&mut self) -> SarektResult<()> {
    self.buffer_image_loader.cleanup()?;
    self.destroy_all_images_and_buffers();
    Ok(())
  }

  /// Load a buffer and allocate memory into the backend/GPU and return a
  /// handle.
  pub fn load_buffer_with_staging<BufElem: Sized + Copy>(
    this: &Arc<RwLock<Self>>, buffer_type: BufferType, buffer: &[BufElem],
  ) -> SarektResult<BufferImageHandle<BL>> {
    let mut buffer_store = this
      .write()
      .expect("Could not unlock BufferStore due to previous panic");

    let buffer_backend_handle = buffer_store
      .buffer_image_loader
      .load_buffer_with_staging(buffer_type, buffer)?;
    let inner_key = buffer_store
      .loaded_buffers_and_images
      .insert(BufferOrImage::new(
        buffer_backend_handle,
        ResourceType::Buffer(buffer_type),
      ));

    Ok(BufferImageHandle {
      inner_key,
      resource_type: ResourceType::Buffer(buffer_type),
      buffer_store: this.clone(),
    })
  }

  pub fn load_buffer_without_staging<BufElem: Sized + Copy>(
    this: &Arc<RwLock<Self>>, buffer_type: BufferType, buffer: &[BufElem],
  ) -> SarektResult<BufferImageHandle<BL>> {
    let mut buffer_store = this
      .write()
      .expect("Could not unlock BufferStore due to previous panic");

    let buffer_backend_handle = buffer_store
      .buffer_image_loader
      .load_buffer_without_staging(buffer_type, buffer)?;
    let inner_key = buffer_store
      .loaded_buffers_and_images
      .insert(BufferOrImage::new(
        buffer_backend_handle,
        ResourceType::Buffer(buffer_type),
      ));

    Ok(BufferImageHandle {
      inner_key,
      resource_type: ResourceType::Buffer(buffer_type),
      buffer_store: this.clone(),
    })
  }

  /// Destroy a buffer and free the memory associated with it from the
  /// backend/GPU.
  fn destroy_buffer(&mut self, inner_key: DefaultKey) -> SarektResult<()> {
    let buffer = self.loaded_buffers_and_images.remove(inner_key);
    if buffer.is_none() {
      return Err(SarektError::UnknownResource);
    }

    self
      .buffer_image_loader
      .delete_buffer_or_image(buffer.unwrap().handle)
  }

  /// Same as `load_buffer_with_staging` but loads an r8b8g8a8 image instead.
  pub fn load_image_with_staging_initialization(
    this: &Arc<RwLock<Self>>, pixels: impl ImageData,
    magnification_filter: MagnificationMinificationFilter,
    minification_filter: MagnificationMinificationFilter, address_x: TextureAddressMode,
    address_y: TextureAddressMode, address_z: TextureAddressMode,
  ) -> SarektResult<BufferImageHandle<BL>> {
    let mut buffer_store = this
      .write()
      .expect("Could not unlock BufferStore due to previous panic");

    let buffer_backend_handle = buffer_store
      .buffer_image_loader
      .load_image_with_staging_initialization(
        pixels,
        magnification_filter,
        minification_filter,
        address_x,
        address_y,
        address_z,
      )?;
    let inner_key = buffer_store
      .loaded_buffers_and_images
      .insert(BufferOrImage::new(
        buffer_backend_handle,
        ResourceType::Image,
      ));

    Ok(BufferImageHandle {
      inner_key,
      resource_type: ResourceType::Image,
      buffer_store: this.clone(),
    })
  }

  pub fn create_uninitialized_image(
    this: &Arc<RwLock<Self>>, dimensions: (u32, u32), format: ImageDataFormat,
  ) -> SarektResult<BufferImageHandle<BL>> {
    let mut buffer_store = this
      .write()
      .expect("Could not unlock BufferStore due to previous panic");

    let buffer_backend_handle = buffer_store
      .buffer_image_loader
      .create_uninitialized_image(dimensions, format)?;
    let inner_key = buffer_store
      .loaded_buffers_and_images
      .insert(BufferOrImage::new(
        buffer_backend_handle,
        ResourceType::Image,
      ));

    Ok(BufferImageHandle {
      inner_key,
      resource_type: ResourceType::Image,
      buffer_store: this.clone(),
    })
  }

  /// Same as `destroy_buffer` but for images.
  fn destroy_image(&mut self, inner_key: DefaultKey) -> SarektResult<()> {
    let image = self.loaded_buffers_and_images.remove(inner_key);
    if image.is_none() {
      return Err(SarektError::UnknownResource);
    }

    self
      .buffer_image_loader
      .delete_buffer_or_image(image.unwrap().handle)
  }

  /// Retrieves the buffer associated with the handle to be bound etc.
  pub(crate) fn get_buffer(
    &self, handle: &BufferImageHandle<BL>,
  ) -> SarektResult<&BufferOrImage<BL::BackendHandle>> {
    if !matches!(handle.resource_type, ResourceType::Buffer(_)) {
      return Err(SarektError::IncorrectResourceType);
    }

    let buffer = self.loaded_buffers_and_images.get(handle.inner_key);
    if let Some(buffer) = buffer {
      return Ok(buffer);
    }
    Err(SarektError::UnknownResource)
  }

  /// Same as `get_buffer` but for images.
  pub(crate) fn get_image(
    &self, handle: &BufferImageHandle<BL>,
  ) -> SarektResult<&BufferOrImage<BL::BackendHandle>> {
    if !matches!(handle.resource_type, ResourceType::Image) {
      return Err(SarektError::IncorrectResourceType);
    }

    let image = self.loaded_buffers_and_images.get(handle.inner_key);
    if let Some(image) = image {
      return Ok(image);
    }
    Err(SarektError::UnknownResource)
  }

  /// Does what it says on the tin, but for all the buffers.  See
  /// destroy_buffers.
  pub fn destroy_all_images_and_buffers(&mut self) {
    for resource in self.loaded_buffers_and_images.iter() {
      if let Err(err) = self
        .buffer_image_loader
        .delete_buffer_or_image(resource.1.handle)
      {
        warn!(
          "Buffer/image not destroyed, maybe it was already? Error: {:?}",
          err
        );
      }
    }

    self.loaded_buffers_and_images.clear();
  }
}

/// The Buffer in terms of its backend handle and the type of buffer.
#[derive(Copy, Clone, Debug)]
pub(crate) struct BufferOrImage<BackendHandle: BackendHandleTrait + Copy> {
  pub handle: BackendHandle,
  pub resource_type: ResourceType,
}
impl<BackendHandle: BackendHandleTrait + Copy> BufferOrImage<BackendHandle> {
  fn new(buffer_handle: BackendHandle, buffer_type: ResourceType) -> Self {
    Self {
      handle: buffer_handle,
      resource_type: buffer_type,
    }
  }
}

/// What filtering strategy to use on uv texture filtering.
pub enum MagnificationMinificationFilter {
  /// Linear interpolation
  Linear,
  /// Nearest pixel
  Nearest,
}

/// What to do when u/v are greater than extent.
/// TODO IMAGES clamp to border/border color?
pub enum TextureAddressMode {
  Repeat,
  MirroredRepeat,
  ClampToEdge,
  MirroredClampToEdge,
}
