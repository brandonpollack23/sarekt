# Sarekt

## Overview

This is my third attempt to make something in Vulkan.  I think I've accumulated enough knowledge and 
wherewithal to make something worthwhile.

This is a renderer that has Vulkan first as it's backend that I want to keep relatively low level.
I think it'll expose a lot at first, and slowly begin wrapping this again in another wrapper 
I'll call "Shran", another Star Trek character.

Shran wrapper.  Get it?

## Usage

This readme is minimal.  Cargo doc is your friend.  This is far from done.

The most up to date documentation/usage you'll get is by checkout out the examples (later is better).  So far the best one is 03_vertex_loading.

Right now it supports basic shapes in Normalized Device Coordinates.  No asset loading. No Uniform Buffers, Not even Index Buffers (yet).

Only one pipeline and render pass type.

Really it's pretty much useless.  But soon.  SOON.

## Hero Dependencies
See the dependencies of this project.  Seriously the Rust community is just fantastic.
* [ash](https://crates.io/crates/ash) Rediculously good vulkan bindings with builders and rock solid velocity.
* [vk-mem](https://crates.io/crates/vk-mem) Solid rust wrapper around an amazing allocation library so I dont have to make GPU malloc all by myself and can get to writing Vulkan code!
* [vk-shader-macros](https://crates.io/crates/vk-shader-macros) Shader macros that compile GLSL to SPIR-V and include the bytes for me.  Juicy.
* [log](https://crates.io/crates/log) Abstract at compile time logging.
* [winit](https://crates.io/crates/winit) Platform agnostic window and vulkan context creation.

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
