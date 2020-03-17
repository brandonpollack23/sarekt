use crate::{
  error::SarektResult,
  renderer::{
    buffers::{BufferBackendHandleTrait, BufferHandle, BufferLoader, UniformBufferHandle},
    vertex_bindings::DefaultForwardShaderUniforms,
    Renderer, VulkanRenderer,
  },
};
use std::fmt::Debug;

// TODO UNIFORMS MACRO multiple uniforms, use a macro_rules to define this class
// for various tuples of uniforms.

/// The object that is passed to Drawer's draw method.  Contains all the
/// necessary information to perform a draw command.
pub struct DrawableObject<
  'a,
  'b,
  'c,
  R: Renderer = VulkanRenderer,
  UniformBufElem: Sized + Copy = DefaultForwardShaderUniforms,
> where
  R::BL: BufferLoader,
  <R::BL as BufferLoader>::BufferBackendHandle: BufferBackendHandleTrait + Copy + Debug,
{
  pub(crate) vertex_buffer: <R::BL as BufferLoader>::BufferBackendHandle,
  pub(crate) index_buffer: Option<<R::BL as BufferLoader>::BufferBackendHandle>,
  // TODO NOW LAST update doc
  pub(crate) uniform_buffer: <R::BL as BufferLoader>::UniformBufferDataHandle,
  _vertex_marker: std::marker::PhantomData<&'a BufferHandle<R::BL>>,
  _index_marker: std::marker::PhantomData<&'b BufferHandle<R::BL>>,
  _uniform_marker: std::marker::PhantomData<&'c BufferHandle<R::BL>>,
  _uniform_type: std::marker::PhantomData<UniformBufElem>,
}
impl<'a, 'b, 'c, R: Renderer, UniformBufElem: Sized + Copy>
  DrawableObject<'a, 'b, 'c, R, UniformBufElem>
where
  R::BL: BufferLoader,
  <R::BL as BufferLoader>::BufferBackendHandle: BufferBackendHandleTrait + Copy + Debug,
{
  pub fn new(
    renderer: &R, vertex_buffer_handle: &'a BufferHandle<R::BL>,
    uniform_buffer_handle: &'c UniformBufferHandle<R::BL, UniformBufElem>,
  ) -> SarektResult<Self> {
    let vertex_buffer = renderer.get_buffer(vertex_buffer_handle)?;
    let uniform_buffer = renderer.get_uniform_buffer(uniform_buffer_handle)?;

    // TODO NOW LAST seperate markers/type data into an inner.
    Ok(Self {
      vertex_buffer,
      index_buffer: None,
      uniform_buffer,
      _vertex_marker: std::marker::PhantomData,
      _index_marker: std::marker::PhantomData,
      _uniform_marker: std::marker::PhantomData,
      _uniform_type: std::marker::PhantomData,
    })
  }

  pub fn new_indexed(
    renderer: &R, vertex_buffer: &'a BufferHandle<R::BL>, index_buffer: &'b BufferHandle<R::BL>,
    uniform_buffer_handle: &'c UniformBufferHandle<R::BL, UniformBufElem>,
  ) -> SarektResult<Self> {
    let vertex_buffer = renderer.get_buffer(vertex_buffer)?;
    let index_buffer = renderer.get_buffer(index_buffer)?;
    let uniform_buffer = renderer.get_uniform_buffer(uniform_buffer_handle)?;

    Ok(Self {
      vertex_buffer,
      index_buffer: Some(index_buffer),
      uniform_buffer,
      _vertex_marker: std::marker::PhantomData,
      _index_marker: std::marker::PhantomData,
      _uniform_marker: std::marker::PhantomData,
      _uniform_type: std::marker::PhantomData,
    })
  }

  // TODO BUFFERS BACKLOG for UniformBufferHandle/DataHandle can specify
  // push_constant type and switch on that in update uniform.
  // TODO PERFORMANCE allow setting at offsets/fields in uniform so you don't have
  // to copy over the whole thing.
  pub fn set_uniform(&self, renderer: &R, data: &UniformBufElem) -> SarektResult<()> {
    renderer.set_uniform(&self.uniform_buffer, data)
  }
}
