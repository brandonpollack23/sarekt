use itertools::izip;
use log::{info, warn, Level};
use sarekt::{
  self,
  error::{SarektError, SarektResult},
  image_data::ImageData,
  renderer::{
    buffers_and_images::{
      BufferType, IndexBufferElemSize, MagnificationMinificationFilter, TextureAddressMode,
    },
    drawable_object::DrawableObject,
    vertex_bindings::{DefaultForwardShaderLayout, DefaultForwardShaderVertex},
    Drawer, Renderer, VulkanRenderer,
  },
};
use std::{collections::HashMap, f32, fs::File, io::Read, sync::Arc, time::Instant};
use ultraviolet as uv;
use wavefront_obj as obj;
use winit::{
  dpi::{LogicalSize, PhysicalSize},
  event::{ElementState, Event, VirtualKeyCode, WindowEvent},
  event_loop::{ControlFlow, EventLoop},
  platform::desktop::EventLoopExtDesktop,
  window::{WindowBuilder, WindowId},
};

const WIDTH: u32 = 1600;
const HEIGHT: u32 = 1200;

const GLB_MODEL_FILE_NAME: &str = "models/chalet.glb";
const OBJ_MODEL_FILE_NAME: &str = "models/chalet.obj";
const MODEL_TEXTURE_FILE_NAME: &str = "textures/chalet.jpg";

fn main() {
  simple_logger::init_with_level(Level::Info).unwrap();
  main_loop();
}

/// Takes full control of the executing thread and runs the event loop for it.
fn main_loop() {
  let args: Vec<String> = std::env::args().collect();
  let show_fps = args.contains(&"fps".to_owned());
  let use_glb = args.contains(&"glb".to_owned());
  if args.len() > 1 && !show_fps && !use_glb {
    panic!("Illegal arguments provided: {:#?}", args);
  }
  info!("Show FPS: {}", show_fps);
  info!("Use GLTF Model Type: {}", use_glb);

  info!("Running main loop...");

  let mut ar = WIDTH as f32 / HEIGHT as f32;

  // Build Window.
  let mut event_loop = EventLoop::new();
  let window = Arc::new(
    WindowBuilder::new()
      .with_inner_size(LogicalSize::new(WIDTH, HEIGHT))
      .build(&event_loop)
      .unwrap(),
  );

  // Build Renderer.
  let mut renderer = VulkanRenderer::new(window.clone(), WIDTH, HEIGHT).unwrap();

  // Create Vertex Resources.
  let (model_vertices, model_indices) = if use_glb {
    load_glb_model(GLB_MODEL_FILE_NAME)
  } else {
    load_obj_models(OBJ_MODEL_FILE_NAME)
  };
  info!("Model file loaded");
  let model_index_buffer = model_indices.map(|mi| {
    renderer
      .load_buffer(BufferType::Index(IndexBufferElemSize::UInt32), &mi)
      .unwrap()
  });
  let model_buffer = renderer
    .load_buffer(BufferType::Vertex, &model_vertices)
    .unwrap();

  // Create MVP uniform.
  let uniform_handle = renderer
    .load_uniform_buffer(DefaultForwardShaderLayout::default())
    .unwrap();

  // Load textures and create image.
  let model_texture_file = image::open(MODEL_TEXTURE_FILE_NAME).unwrap();
  let mip_levels = get_mip_levels(model_texture_file.dimensions());
  let model_texture = renderer
    .load_image_with_staging_initialization(
      model_texture_file,
      MagnificationMinificationFilter::Linear,
      MagnificationMinificationFilter::Linear,
      TextureAddressMode::ClampToEdge,
      TextureAddressMode::ClampToEdge,
      TextureAddressMode::ClampToEdge,
      mip_levels,
    )
    .unwrap();

  let mut drawable_object_builder = DrawableObject::builder(&renderer)
    .uniform_buffer(&uniform_handle)
    .vertex_buffer(&model_buffer)
    .texture_image(&model_texture);
  if model_index_buffer.is_some() {
    drawable_object_builder =
      drawable_object_builder.index_buffer(model_index_buffer.as_ref().unwrap());
  }
  let drawable_object = drawable_object_builder.build().unwrap();

  let start_time = Instant::now();
  let mut last_frame_time = start_time;
  let mut frame_number = 0;
  let mut fps_average = 0f32;

  let mut camera_height = -0.5f32;

  // Run the loop.
  event_loop.run_return(move |event, _, control_flow| {
    // By default continuously run this event loop, even if the OS hasn't
    // distributed an event, that way we will draw as fast as possible.
    *control_flow = ControlFlow::Poll;

    match event {
      Event::MainEventsCleared => {
        // All the main events to process are done we can do "work" now (game
        // engine state update etc.)
        let now = Instant::now();
        let time_since_start_secs = ((now - start_time).as_millis() as f32) / 1000f32;

        if show_fps {
          let time_since_last_frame_secs = ((now - last_frame_time).as_nanos() as f32) / 1e9f32;
          let fps = 1f32 / time_since_last_frame_secs;
          if frame_number == 0 {
            fps_average = 0f32;
          } else {
            fps_average =
              ((frame_number as f32 * fps_average) + fps) / (frame_number as f32 + 1f32);
          }
          frame_number += 1;

          info!("Frame Period: {}", time_since_last_frame_secs);
          info!("FPS: {}", fps);
          info!("FPS averaged: {}", fps_average);
          last_frame_time = now;
        }

        // Rise to max height then gently go back down.
        let camera_rate = 0.25f32;
        let min_camera_height = -0.5f32;
        let camera_range = 2f32;
        camera_height =
          (camera_rate * time_since_start_secs) % (2.0f32 * camera_range) + min_camera_height;
        if camera_height >= (camera_range + min_camera_height) {
          camera_height = (2.0f32 * (camera_range + min_camera_height)) - camera_height;
        }

        let rotation = (std::f32::consts::PI + std::f32::consts::PI * time_since_start_secs / 8f32)
          % (2f32 * std::f32::consts::PI);
        update_uniforms(
          &renderer,
          &drawable_object,
          uv::Vec3::new(0f32, -1f32, -1.5f32),
          rotation,
          camera_height,
          false,
          ar,
        )
        .unwrap();

        renderer.draw(&drawable_object).unwrap();

        // At the end of work request redraw.
        window.request_redraw();
      }

      Event::RedrawRequested(_) => {
        // Redraw requested, this is called after MainEventsCleared.
        renderer.frame().unwrap_or_else(|err| {
          match err {
            SarektError::SwapchainOutOfDate | SarektError::SuboptimalSwapchain => {
              // Handle window resize etc.
              warn!("Tried to render without processing window resize event!");

              let PhysicalSize { width, height } = window.inner_size();
              renderer
                .recreate_swapchain(width, height)
                .expect("Error recreating swapchain");
            }
            e => panic!("Frame had an unrecoverable error! {}", e),
          }
        });
      }

      Event::WindowEvent { window_id, event } => {
        main_loop_window_event(&event, &window_id, control_flow, &mut renderer, &mut ar)
          .expect("Error processing window event.");
      }

      Event::LoopDestroyed => {
        // Explicitly call exit so resources are cleaned up.
        std::process::exit(0);
      }
      _ => (),
    }
  });
}

/// Handles all winit window specific events.
fn main_loop_window_event(
  event: &WindowEvent, _id: &WindowId, control_flow: &mut winit::event_loop::ControlFlow,
  renderer: &mut VulkanRenderer, ar: &mut f32,
) -> SarektResult<()> {
  match event {
    WindowEvent::CloseRequested => {
      // When the window system requests a close, signal to winit that we'd like to
      // close the window.
      info!("Exiting due to close request event from window system...");
      *control_flow = ControlFlow::Exit;
    }

    WindowEvent::KeyboardInput { input, .. } => {
      // When the keyboard input is a press on the escape key, exit and print the
      // line.
      if let (Some(VirtualKeyCode::Escape), ElementState::Pressed) =
        (input.virtual_keycode, input.state)
      {
        info!("Exiting due to escape press...");
        *control_flow = ControlFlow::Exit
      }
    }

    WindowEvent::Resized(size) => {
      // If the size is 0, minimization or something like that happened so I
      // toggle drawing.
      info!("Window resized, recreating renderer swapchain...");
      let enabled = !(size.height == 0 && size.width == 0);
      if enabled {
        *ar = size.width as f32 / size.height as f32;
      }
      renderer.set_rendering_enabled(enabled);
      return renderer.recreate_swapchain(size.width, size.height);
    }
    _ => (),
  }

  Ok(())
}

fn update_uniforms(
  renderer: &VulkanRenderer, object: &DrawableObject<VulkanRenderer, DefaultForwardShaderLayout>,
  position: uv::Vec3, rotation: f32, camera_height: f32, enable_colors: bool, ar: f32,
) -> SarektResult<()> {
  // Pi radians per second around the y axis.
  let total_rotation =
    uv::Mat4::from_rotation_y(rotation) * uv::Mat4::from_rotation_x(-std::f32::consts::PI / 2f32);
  let model_matrix = uv::Mat4::from_translation(position) * total_rotation;

  let view_matrix = uv::Mat4::look_at(
    /* eye= */ uv::Vec3::new(0.0f32, camera_height, 0.0f32),
    /* at= */ position,
    /* up= */ uv::Vec3::unit_y(),
  );
  // TODO BACKENDS this proj should be conditional on backend.
  let perspective_matrix =
    uv::projection::rh_yup::perspective_vk(std::f32::consts::PI / 2f32, ar, 0.1f32, 10f32);

  let uniform = DefaultForwardShaderLayout::new(
    perspective_matrix * view_matrix * model_matrix,
    enable_colors,
    /* enable_texture_mixing= */ true,
  );
  object.set_uniform(renderer, &uniform)
}

/// For now only use the first object in the obj file.
/// Returns (vertices, vertex_indicies, texture_coordinate indices)
fn load_obj_models(obj_file_path: &str) -> (Vec<DefaultForwardShaderVertex>, Option<Vec<u32>>) {
  let mut model_file = File::open(obj_file_path).unwrap();
  let mut model_file_text = String::new();
  model_file.read_to_string(&mut model_file_text).unwrap();

  let obj_set = obj::obj::parse(&model_file_text).unwrap();
  if obj_set.objects.len() != 1 {
    panic!(
      "The model you attempted to load has more than one object in it, implying it is a scene, if \
       you wish to use it as a single model, modify the application code to ignore that or join \
       your meshes into a single model"
    );
  }

  info!("Loaded model {}", OBJ_MODEL_FILE_NAME);
  let mut vertices: Vec<DefaultForwardShaderVertex> = Vec::new();
  let mut indices: Vec<u32> = Vec::new();

  // Map of inserted (obj_vertex_index, obj_texture_index) to index in the
  // vertices array im building.
  let mut inserted_indices: HashMap<(usize, usize), usize> = HashMap::new();
  let model_vertices = &obj_set.objects[0].vertices;
  for geo in obj_set.objects[0].geometry.iter() {
    // For every set of geometry (regardless of material for now).
    for shape in geo.shapes.iter() {
      // For every face/shape in the set of geometry.
      match shape.primitive {
        obj::obj::Primitive::Triangle(x, y, z) => {
          for &vert in [x, y, z].iter() {
            // We're only building a buffer of indices and vertices which contain position
            // and tex coord.
            let index_key = (vert.0, vert.1.unwrap());
            if let Some(&vtx_index) = inserted_indices.get(&index_key) {
              // Already loaded this (vertex index, texture index) combo, just add it to the
              // index buffer.
              indices.push(vtx_index as _);
              continue;
            }

            // This is a new unique vertex (where a vertex is both a position and it's
            // texture coordinate) so add it to the vertex buffer and the index buffer.
            let current_vertex = model_vertices[vert.0];
            let vertex_as_float = [
              current_vertex.x as f32,
              current_vertex.y as f32,
              current_vertex.z as f32,
            ];
            let texture_vertices = &obj_set.objects[0].tex_vertices;
            let tex_vertex = texture_vertices[vert.1.unwrap()];
            // TODO BACKENDS only flip on coordinate systems that should.
            let texture_vertex_as_float = [tex_vertex.u as f32, 1f32 - tex_vertex.v as f32];

            // Ignoring normals, there is no shading in this example.

            // Keep track of which keys were inserted and add this vertex to the index
            // buffer.
            inserted_indices.insert(index_key, vertices.len());
            indices.push(vertices.len() as _);

            // Add to the vertex buffer.
            vertices.push(DefaultForwardShaderVertex::new_with_texture(
              &vertex_as_float,
              &texture_vertex_as_float,
            ));
          }
        }
        _ => warn!("Unsupported primitive!"),
      }
    }
  }

  info!(
    "Vertices/indices in model: {}, {}",
    vertices.len(),
    indices.len()
  );
  (vertices, Some(indices))
}

/// Returns (vertices, vertex_indicies, texture_coordinate indices)
fn load_glb_model(gltf_file_path: &str) -> (Vec<DefaultForwardShaderVertex>, Option<Vec<u32>>) {
  let (document, buffers, _) = gltf::import(gltf_file_path).unwrap();

  if document.scenes().len() != 1 || document.scenes().next().unwrap().nodes().len() != 1 {
    panic!(
      "The model you attempted to load has more than one scene or node in it, if you wish to use \
       it as a single model, modify the application code to ignore that or join your meshes into \
       a single model"
    );
  }

  let mesh = document.meshes().nth(0).unwrap();

  info!("Loaded model {}", gltf_file_path);
  let mut vertices: Vec<DefaultForwardShaderVertex> = Vec::new();
  let mut indices: Option<Vec<u32>> = None;

  for primitive in mesh.primitives() {
    let reader = primitive.reader(|buffer| Some(&buffers[buffer.index()]));
    let positions = reader.read_positions().unwrap();
    let tex_coords = reader.read_tex_coords(0).unwrap().into_f32();
    for (position, tex_coord) in izip!(positions, tex_coords) {
      vertices.push(DefaultForwardShaderVertex::new_with_texture(
        &position, &tex_coord,
      ));
    }

    reader
      .read_indices()
      .map(|it| indices.get_or_insert(Vec::new()).extend(&mut it.into_u32()));
  }

  info!(
    "Vertices/indices in model: {}, {:?}",
    vertices.len(),
    indices.as_ref().map(|i| i.len())
  );
  (vertices, indices)
}

fn get_mip_levels(dimensions: (u32, u32)) -> u32 {
  let w = dimensions.0;
  let h = dimensions.1;
  (w.max(h) as f32).log2().floor() as u32 + 1
}
