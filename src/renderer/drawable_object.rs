use std::fmt::Debug;

use crate::{
  error::SarektResult,
  renderer::{
    buffers_and_images::{
      BackendHandleTrait, BufferAndImageLoader, BufferImageHandle, UniformBufferHandle,
    },
    vertex_bindings::DefaultForwardShaderLayout,
    Renderer, VulkanRenderer,
  },
};

// TODO UNIFORMS MACRO multiple uniforms, use a macro_rules to define this class
// for various tuples of uniforms.

/// The object that is passed to Drawer's draw method.  Contains all the
/// necessary information to perform a draw command.
///
/// vertex_buffer is the list of vertices for the mesh to draw, index_buffer is
/// optional and contains the order of indices to make the mesh in the vertex
/// buffer, and uniform_buffer contains the uniform data for the associated
/// shaders/pipeline.
///
/// This struct is constructed using references and the lifetime specifications
/// will not allow this class to outlive them.
pub struct DrawableObject<
  'a,
  'b,
  'c,
  'd,
  R: Renderer = VulkanRenderer,
  DescriptorLayoutStruct: Sized + Copy = DefaultForwardShaderLayout,
> where
  R::BL: BufferAndImageLoader,
  <R::BL as BufferAndImageLoader>::BackendHandle: BackendHandleTrait + Copy + Debug,
{
  pub(crate) vertex_buffer: <R::BL as BufferAndImageLoader>::BackendHandle,
  pub(crate) index_buffer: Option<<R::BL as BufferAndImageLoader>::BackendHandle>,
  pub(crate) uniform_buffer: <R::BL as BufferAndImageLoader>::UniformBufferDataHandle,
  pub(crate) texture_image: Option<<R::BL as BufferAndImageLoader>::BackendHandle>,

  _vertex_marker: std::marker::PhantomData<&'a BufferImageHandle<R::BL>>,
  _index_marker: std::marker::PhantomData<&'b BufferImageHandle<R::BL>>,
  _uniform_marker: std::marker::PhantomData<&'c BufferImageHandle<R::BL>>,
  _texture_image_marker: std::marker::PhantomData<&'d BufferImageHandle<R::BL>>,

  _uniform_type: std::marker::PhantomData<DescriptorLayoutStruct>,
}
impl<'a, 'b, 'c, 'd, R: Renderer, DescriptorLayoutStruct: Sized + Copy>
  DrawableObject<'a, 'b, 'c, 'd, R, DescriptorLayoutStruct>
where
  R::BL: BufferAndImageLoader,
  <R::BL as BufferAndImageLoader>::BackendHandle: BackendHandleTrait + Copy + Debug,
{
  pub fn builder<'r>(
    renderer: &'r R,
  ) -> DrawableObjectBuilder<'r, 'a, 'b, 'c, 'd, R, DescriptorLayoutStruct> {
    DrawableObjectBuilder {
      renderer: Some(renderer),
      vertex_buffer: None,
      index_buffer: None,
      uniform_buffer: None,
      texture_image: None,
    }
  }

  pub fn new(
    renderer: &R, vertex_buffer: &'a BufferImageHandle<R::BL>,
    index_buffer: Option<&'b BufferImageHandle<R::BL>>,
    uniform_buffer_handle: &'c UniformBufferHandle<R::BL, DescriptorLayoutStruct>,
    texture_image: Option<&'d BufferImageHandle<R::BL>>,
  ) -> SarektResult<Self> {
    let vertex_buffer = renderer.get_buffer(vertex_buffer)?;
    let index_buffer = index_buffer
      .map(|ibh| renderer.get_buffer(ibh))
      .transpose()?;
    let uniform_buffer = renderer.get_uniform_buffer(uniform_buffer_handle)?;
    let texture_image = texture_image
      .map(|tih| renderer.get_image(tih))
      .transpose()?;

    Ok(Self {
      vertex_buffer,
      index_buffer,
      uniform_buffer,
      texture_image,

      _vertex_marker: std::marker::PhantomData,
      _index_marker: std::marker::PhantomData,
      _uniform_marker: std::marker::PhantomData,
      _texture_image_marker: std::marker::PhantomData,

      _uniform_type: std::marker::PhantomData,
    })
  }

  // TODO BUFFERS BACKLOG for UniformBufferHandle/DataHandle can specify
  // push_constant type and switch on that in update uniform.
  // TODO PERFORMANCE allow setting at offsets/fields in uniform so you don't have
  // to copy over the whole thing.
  pub fn set_uniform(&self, renderer: &R, data: &DescriptorLayoutStruct) -> SarektResult<()> {
    renderer.set_uniform(&self.uniform_buffer, data)
  }
}

#[derive(Copy, Clone)]
pub struct DrawableObjectBuilder<
  'r,
  'a,
  'b,
  'c,
  'd,
  R: Renderer,
  DescriptorLayoutStruct: Sized + Copy,
> where
  R::BL: BufferAndImageLoader,
  <R::BL as BufferAndImageLoader>::BackendHandle: BackendHandleTrait + Copy + Debug,
{
  pub renderer: Option<&'r R>,
  pub vertex_buffer: Option<&'a BufferImageHandle<R::BL>>,
  pub index_buffer: Option<&'b BufferImageHandle<R::BL>>,
  pub uniform_buffer: Option<&'c UniformBufferHandle<R::BL, DescriptorLayoutStruct>>,
  pub texture_image: Option<&'d BufferImageHandle<R::BL>>,
}
impl<'r, 'a, 'b, 'c, 'd, R: Renderer, DescriptorLayoutStruct: Sized + Copy>
  DrawableObjectBuilder<'r, 'a, 'b, 'c, 'd, R, DescriptorLayoutStruct>
where
  R::BL: BufferAndImageLoader,
  <R::BL as BufferAndImageLoader>::BackendHandle: BackendHandleTrait + Copy + Debug,
{
  pub fn build(self) -> SarektResult<DrawableObject<'a, 'b, 'c, 'd, R, DescriptorLayoutStruct>> {
    DrawableObject::new(
      self.renderer.unwrap(),
      self.vertex_buffer.unwrap(),
      self.index_buffer,
      self.uniform_buffer.unwrap(),
      self.texture_image,
    )
  }

  pub fn vertex_buffer(
    mut self, vertex_buffer: &'a BufferImageHandle<R::BL>,
  ) -> DrawableObjectBuilder<'r, 'a, 'b, 'c, 'd, R, DescriptorLayoutStruct> {
    self.vertex_buffer = Some(vertex_buffer);
    self
  }

  pub fn index_buffer(mut self, index_buffer: &'b BufferImageHandle<R::BL>) -> Self {
    self.index_buffer = Some(index_buffer);
    self
  }

  pub fn uniform_buffer(
    mut self, uniform_buffer: &'c UniformBufferHandle<R::BL, DescriptorLayoutStruct>,
  ) -> Self {
    self.uniform_buffer = Some(uniform_buffer);
    self
  }

  pub fn texture_image(mut self, texture_image: &'d BufferImageHandle<R::BL>) -> Self {
    self.texture_image = Some(texture_image);
    self
  }
}
