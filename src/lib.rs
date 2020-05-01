//! A renderer toy project!  I hope to hide the fact that it only supports
//! Vulkan from you in the hopes it one day supports more. It barely even
//! supports that...
//!
//! See renderer crate for how to use.
#[macro_use]
extern crate memoffset;
#[macro_use]
extern crate derive_builder;

pub mod error;
pub mod image_data;
pub mod renderer;
