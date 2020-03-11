use crate::{
  error::SarektResult,
  renderer::{
    buffers::{BufferBackendHandle, BufferHandle, BufferLoader},
    Renderer,
  },
};
use std::fmt::Debug;

// TODO CRITICAL uniforms which can be set (push constants or otherwise),
// anything else draw needs.

/// The object that is passed to Drawer's draw method.  Contains all the
/// necessary information to perform a draw command.
pub struct DrawableObject<'a, 'b, 'c, R: Renderer>
where
  R::BL: BufferLoader,
  <R::BL as BufferLoader>::BBH: BufferBackendHandle + Copy + Debug,
{
  pub(crate) vertex_buffer: <R::BL as BufferLoader>::BBH,
  pub(crate) index_buffer: Option<<R::BL as BufferLoader>::BBH>,
  pub(crate) uniform_buffer: Option<<R::BL as BufferLoader>::BBH>,
  _vertex_marker: std::marker::PhantomData<&'a BufferHandle<R::BL>>,
  _index_marker: std::marker::PhantomData<&'b BufferHandle<R::BL>>,
  _uniform_marker: std::marker::PhantomData<&'c BufferHandle<R::BL>>,
}
impl<'a, 'b, 'c, R: Renderer> DrawableObject<'a, 'b, 'c, R>
where
  R::BL: BufferLoader,
  <R::BL as BufferLoader>::BBH: BufferBackendHandle + Copy + Debug,
{
  pub fn new(
    renderer: &R, vertex_buffer_handle: &'a BufferHandle<R::BL>,
    uniform_buffer_handle: &'c Option<BufferHandle<R::BL>>,
  ) -> SarektResult<Self> {
    let vertex_buffer = renderer.get_buffer(vertex_buffer_handle)?;
    let uniform_buffer = if let Some(ub) = uniform_buffer_handle {
      Some(renderer.get_buffer(&ub)?)
    } else {
      None
    };

    Ok(Self {
      vertex_buffer,
      index_buffer: None,
      uniform_buffer,
      _vertex_marker: std::marker::PhantomData,
      _index_marker: std::marker::PhantomData,
      _uniform_marker: std::marker::PhantomData,
    })
  }

  pub fn new_indexed(
    renderer: &R, vertex_buffer: &'a BufferHandle<R::BL>, index_buffer: &'b BufferHandle<R::BL>,
    uniform_buffer_handle: &'c Option<BufferHandle<R::BL>>,
  ) -> SarektResult<Self> {
    let vertex_buffer = renderer.get_buffer(vertex_buffer)?;
    let index_buffer = renderer.get_buffer(index_buffer)?;
    let uniform_buffer = if let Some(ub) = uniform_buffer_handle {
      Some(renderer.get_buffer(&ub)?)
    } else {
      None
    };

    Ok(Self {
      vertex_buffer,
      index_buffer: Some(index_buffer),
      uniform_buffer,
      _vertex_marker: std::marker::PhantomData,
      _index_marker: std::marker::PhantomData,
      _uniform_marker: std::marker::PhantomData,
    })
  }
}
