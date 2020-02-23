//! This is the main renderer module of Sarekt.
//!
//! As I learn vulkan and rendering better, so too will this documentation
//! improve.  For now in order to use Sarekt follow these steps:
//! 1) Create your system window (I reccomened using
//! [winit](https://www.crates.io/crates/winit))
//!
//! 2) Create the renderer with either `new`  or `new_detailed`, passing in your
//! system window.
//! ```rust,no_run
//! # use sarekt::renderer::Renderer;
//! # use std::sync::Arc;
//! # use winit::window::WindowBuilder;
//! # use winit::event_loop::EventLoop;
//! let event_loop = EventLoop::new();
//! let window = Arc::new(WindowBuilder::new().build(&event_loop).unwrap());
//! Renderer::new(window.clone()).unwrap();
//! ```
//!
//! 3) That's all you can do.
//!
//! I hope to support some stuff like:
//! - [ ] Actually Rendering something
//! - [ ] Actually rendering something of your choosing
//! - [ ] Loading spirv shaders and generating internal information needed for
//!   them (for Vulkan that's descriptor sets/layouts)
//! - [ ] Moar.
//!
//! Things on the TODO list are in a repo TODO file.
mod renderer;

pub use renderer::Renderer;
