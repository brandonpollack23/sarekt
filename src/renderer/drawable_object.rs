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
pub struct DrawableObject<'a, R: Renderer>
where
  R::BL: BufferLoader,
  <R::BL as BufferLoader>::BBH: BufferBackendHandle + Copy + Debug,
{
  pub(crate) vertex_buffer: <R::BL as BufferLoader>::BBH,
  pub(crate) index_buffer: Option<<R::BL as BufferLoader>::BBH>,
  _marker: std::marker::PhantomData<&'a BufferHandle<R::BL>>,
}
impl<'a, R: Renderer> DrawableObject<'a, R>
where
  R::BL: BufferLoader,
  <R::BL as BufferLoader>::BBH: BufferBackendHandle + Copy + Debug,
{
  pub fn new(renderer: &R, vertex_buffer: &'a BufferHandle<R::BL>) -> SarektResult<Self> {
    let vertex_buffer = renderer.get_buffer(vertex_buffer)?;
    Ok(Self {
      vertex_buffer,
      index_buffer: None,
      _marker: std::marker::PhantomData,
    })
  }

  // pub fn new_indexed(
  //   renderer: &R, vertex_buffer: &BufferHandle<R::BL>, index_buffer:
  // &BufferHandle<R::BL>, ) -> SarektResult<Self> {
  //   let vertex_buffer = renderer.get_buffer(vertex_buffer)?;
  //   let index_buffer = renderer.get_buffer(index_buffer)?;
  //   Ok(Self {
  //     vertex_buffer,
  //     index_buffer: Some(index_buffer),
  //   })
  // }
}
