# Sarekt

[![Crates.io](https://img.shields.io/crates/v/sarekt.svg)](https://crates.io/crates/sarekt)
[![](https://tokei.rs/b1/github/brandonpollack23/sarekt)](https://github.com/brandonpollack23/sarekt)

## Overview

This is my third attempt to make something in Vulkan.  I think I've accumulated enough knowledge and 
wherewithal to make something worthwhile.

This is a renderer that has Vulkan first as it's backend that I want to keep relatively low level.
I think it'll expose a lot at first, and slowly begin wrapping this again in another wrapper 
I'll call "Shran", another Star Trek character.

Shran wrapper.  Get it?

## Usage

This readme is minimal.  Cargo doc is your friend.  This is far from done.

The most up to date documentation/usage you'll get is by checkout out the 
examples (later is better).  So far the best one is 03_vertex_loading.

Sarekt can load arbitrary models, textures, and uniforms and display them.

Textures can be any image format supported by the image crate and will be
 converted

Only one pipeline and render pass type.

## Hero Dependencies
See the dependencies of this project.  Seriously the Rust community is just 
fantastic.
* [ash](https://crates.io/crates/ash) Rediculously good vulkan bindings with 
builders and rock solid velocity.
* [vk-mem](https://crates.io/crates/vk-mem) Solid rust wrapper around an
 amazing allocation library so I dont have to make GPU malloc all by myself
  and can get to writing Vulkan code!  I fixed a bug and somehow become a
   contributor to this one
* [vk-shader-macros](https://crates.io/crates/vk-shader-macros) Shader macros that compile GLSL to SPIR-V and include the bytes for me.  Juicy.
* [log](https://crates.io/crates/log) Abstract at compile time logging.
* [winit](https://crates.io/crates/winit) Platform agnostic window and vulkan context creation.
* [slotmap](https://crates.io/crates/slotmap) A great generational index
 store, useful for handles
* [wavefront_obj](https://crates.io/crates/wavefront_obj) Model loader for obj
* [gltf](https://crates.io/crates/gltf) Model loader for GlTf
* [image](https://crates.io/crates/image) Loading image data.

There are more but these ones I rely on the most, please check them all out.

## Name
"Sarek" is the Vulcan father of the character "Spok"  of Star Trek. The added t also makes it a portmanteu with rect(angle).

I know and I'm sorry.

## Examples
All the examples that are straightforward with no params have no section here, just run'em with `cargo run --example NAME`

Those that have arguments just pass them like so:<br/>
`cargo run --example 06_textures -- colors`

### Example 6, Textures
This is where things finally start to get fun,there's a param to enable color mixing with the texture.

Starting with this example, the application coordinate space is right handed y 
up for simplicity, the ultraviolet library perspective functions correct it for the appropriate backend.

arguments:
* *colors* - turns on the color mixing from raw colors, a simple multiplicative color mix in the default forward shader

### Example 7 Depth buffer

Here we can see the depth buffer working in action.  Camera moves so you an
 see the 3d effect.
 
 Flags:
 * *fps* -- display fps
 
 ### Example 8 Model loading.
 
 Load a real 3d model (PreBaked lighting only)
 
 Flags:
 * *glb*  -- load glb version of the model.
 * *fps*  -- show fps
