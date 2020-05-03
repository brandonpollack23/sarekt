use crate::{
  error::SarektResult,
  renderer::{
    config::PresentMode,
    vulkan::{
      images::ImageAndView,
      queues::QueueFamilyIndices,
      vulkan_renderer::{
        surface::SurfaceAndExtension,
        swap_chain::SwapchainAndExtension,
        vulkan_core::{VulkanCoreStructures, VulkanDeviceStructures},
      },
    },
  },
};
use ash::{version::DeviceV1_0, vk, Device};
use log::{info, warn};
use std::sync::Arc;

/// Render target related structures, such as the swapchain extension, the
/// extent, and the images themselves.
pub struct RenderTargetBundle {
  pub swapchain_and_extension: SwapchainAndExtension, // TODO(issue#9) OFFSCREEN option
  pub render_targets: Vec<ImageAndView>,              // aka SwapChainImages if presenting.
  pub extent: vk::Extent2D,
}
impl RenderTargetBundle {
  pub fn new(
    vulkan_core: &VulkanCoreStructures, device_bundle: &VulkanDeviceStructures,
    requested_width: u32, requested_height: u32, requested_present_mode: PresentMode,
  ) -> SarektResult<RenderTargetBundle> {
    let swapchain_extension = ash::extensions::khr::Swapchain::new(
      vulkan_core.instance.as_ref(),
      device_bundle.logical_device.as_ref(),
    );
    let (swapchain, format, extent) = Self::create_swapchain(
      &vulkan_core.surface_and_extension,
      &swapchain_extension,
      device_bundle.physical_device,
      &device_bundle.queue_families,
      requested_width,
      requested_height,
      requested_present_mode,
      None,
    )?;
    let swapchain_and_extension =
      SwapchainAndExtension::new(swapchain, format, swapchain_extension);

    // TODO(issue#9) OFFSCREEN if not swapchain create images that im rendering to.
    let render_target_images = unsafe {
      swapchain_and_extension
        .swapchain_functions
        .get_swapchain_images(swapchain_and_extension.swapchain)?
    };
    let render_targets = Self::create_render_target_image_views(
      &device_bundle.logical_device,
      render_target_images,
      swapchain_and_extension.format,
    )?;

    Ok(RenderTargetBundle {
      swapchain_and_extension,
      render_targets,
      extent,
    })
  }

  /// Gets the next image in the swapchain to draw to and associates the given
  /// semaphore and fence with it.
  pub fn acquire_next_image(
    &self, timeout: u64, image_available_semaphore: vk::Semaphore, image_available_fence: vk::Fence,
  ) -> SarektResult<(u32, bool)> {
    // TODO(issue#9) OFFSCREEN handle drawing without swapchain.
    unsafe {
      Ok(
        self
          .swapchain_and_extension
          .swapchain_functions
          .acquire_next_image(
            self.swapchain_and_extension.swapchain,
            timeout,
            image_available_semaphore,
            image_available_fence,
          )?,
      )
    }
  }

  /// Presents to the swapchain waiting on the device semaphore.
  pub fn queue_present(
    &self, image_index: usize, presentation_queue: vk::Queue, wait_semaphores: &[vk::Semaphore],
  ) -> SarektResult<()> {
    let swapchains = [self.swapchain_and_extension.swapchain];
    let image_indices = [image_index as u32];
    let present_info = vk::PresentInfoKHR::builder()
      .wait_semaphores(wait_semaphores)
      .swapchains(&swapchains)
      .image_indices(&image_indices)
      .build();
    unsafe {
      self
        .swapchain_and_extension
        .swapchain_functions
        .queue_present(presentation_queue, &present_info)?;
    }

    Ok(())
  }

  pub fn get_render_target_format(&self) -> vk::Format {
    self.swapchain_and_extension.format
  }

  /// Checks if the width and height given differ from the render target extent.
  pub fn extent_is_equal_to(&self, width: u32, height: u32) -> bool {
    self.extent.width == width && self.extent.height == height
  }

  /// Recreates teh swapchain using the new parameters and returns the old
  /// swapchain and images/views.
  ///
  /// Unsafe because of FFI use and the returned swapchain must be cleaned up.
  pub unsafe fn recreate_swapchain(
    &mut self, vulkan_core: &VulkanCoreStructures, device_bundle: &VulkanDeviceStructures,
    requested_width: u32, requested_height: u32, requested_present_mode: PresentMode,
  ) -> SarektResult<(vk::SwapchainKHR, Vec<ImageAndView>)> {
    let old_swapchain = self.swapchain_and_extension.swapchain;

    let (new_swapchain, new_format, new_extent) = RenderTargetBundle::create_swapchain(
      &vulkan_core.surface_and_extension,
      &self.swapchain_and_extension.swapchain_functions,
      device_bundle.physical_device,
      &device_bundle.queue_families,
      requested_width,
      requested_height,
      requested_present_mode,
      Some(old_swapchain),
    )?;

    self.swapchain_and_extension.swapchain = new_swapchain;
    self.swapchain_and_extension.format = new_format;
    self.extent = new_extent;

    // TODO(issue#9) OFFSCREEN if not swapchain create images that im rendering to.
    let render_target_images = self
      .swapchain_and_extension
      .swapchain_functions
      .get_swapchain_images(new_swapchain)?;

    let mut render_targets = Self::create_render_target_image_views(
      &device_bundle.logical_device,
      render_target_images,
      new_format,
    )?;
    std::mem::swap(&mut self.render_targets, &mut render_targets);

    Ok((old_swapchain, render_targets))
  }

  /// Useful during swapchain recreation, but the specific render targets and
  /// swapchain to delete are specified, since the current ones are always
  /// contained in the struct.
  pub unsafe fn cleanup_render_targets(
    &self, device_bundle: &VulkanDeviceStructures, render_targets: &[ImageAndView],
    swapchain: vk::SwapchainKHR,
  ) {
    info!("Destrying render target views...");
    for view in render_targets.iter() {
      device_bundle
        .logical_device
        .destroy_image_view(view.view, None);
    }
    // TODO(issue#9) OFFSCREEN if images and not swapchain destroy images.

    // TODO(issue#9) OFFSCREEN if there is one, if not destroy images (as above todo
    // states).
    info!("Destrying swapchain...");
    let swapchain_functions = &self.swapchain_and_extension.swapchain_functions;
    swapchain_functions.destroy_swapchain(swapchain, None);
  }

  // ================================================================================
  //  Presentation and Swapchain Helper Methods
  // ================================================================================
  /// Based on the capabilities of the surface, the physical device, and the
  /// configuration of sarekt, creates a swapchain with the appropriate
  /// configuration (format, color space, present mode, and extent).
  fn create_swapchain(
    surface_and_extension: &SurfaceAndExtension,
    swapchain_extension: &ash::extensions::khr::Swapchain, physical_device: vk::PhysicalDevice,
    queue_family_indices: &QueueFamilyIndices, requested_width: u32, requested_height: u32,
    requested_present_mode: PresentMode, old_swapchain: Option<vk::SwapchainKHR>,
  ) -> SarektResult<(vk::SwapchainKHR, vk::Format, vk::Extent2D)> {
    let swapchain_support =
      VulkanDeviceStructures::query_swap_chain_support(surface_and_extension, physical_device)?;

    let format = Self::choose_swap_surface_format(&swapchain_support.formats);
    let present_mode =
      Self::choose_presentation_mode(&swapchain_support.present_modes, requested_present_mode);
    let extent = Self::choose_swap_extent(
      &swapchain_support.capabilities,
      requested_width,
      requested_height,
    );

    // Select minimum number of images to render to.  For triple buffering this
    // would be 3, etc. But don't exceed the max.  Implementation may create more
    // than this depending on present mode.
    // [vulkan tutorial](https://vulkan-tutorial.com/Drawing_a_triangle/Presentation/Swap_chain)
    // recommends setting this to min + 1 because if we select minimum we may wait
    // on internal driver operations.
    let max_image_count = swapchain_support.capabilities.max_image_count;
    let max_image_count = if max_image_count == 0 {
      u32::max_value()
    } else {
      max_image_count
    };
    let min_image_count = (swapchain_support.capabilities.min_image_count + 1).min(max_image_count);

    let sharing_mode = if queue_family_indices.graphics_queue_family.unwrap()
      != queue_family_indices.presentation_queue_family.unwrap()
    {
      // Concurrent sharing mode because the images will need to be accessed by more
      // than one queue family.
      vk::SharingMode::CONCURRENT
    } else {
      // Exclusive (probly) has best performance, not sharing the image with other
      // queue families.
      vk::SharingMode::EXCLUSIVE
    };

    let swapchain_ci = vk::SwapchainCreateInfoKHR::builder()
      .surface(surface_and_extension.surface)
      .min_image_count(min_image_count)
      .image_format(format.format)
      .image_color_space(format.color_space)
      .image_extent(extent)
      .image_array_layers(1) // Number of views (multiview/stereo surface for 3D applications with glasses or maybe VR).
      .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT) // We'll just be rendering colors to this.  We could render to another image and transfer here after post processing but we're not.
      .image_sharing_mode(sharing_mode)
      .queue_family_indices(&queue_family_indices.as_vec().unwrap())
      .pre_transform(swapchain_support.capabilities.current_transform) // Match the transform of the swapchain, I'm not trying to redner upside down!
      .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE) // No alpha blending within the window system for now.
      .present_mode(present_mode)
      .clipped(true) // Go ahead and discard rendering ops we dont need (window half off screen).
      .old_swapchain(old_swapchain.unwrap_or_else(vk::SwapchainKHR::null)) // Pass old swapchain for recreation.
      .build();

    let swapchain = unsafe { swapchain_extension.create_swapchain(&swapchain_ci, None)? };
    Ok((swapchain, format.format, extent))
  }

  /// If drawing to a surface, chooses the best format from the ones available
  /// for the surface.  Tries to use B8G8R8A8_SRGB format with SRGB_NONLINEAR
  /// colorspace.
  ///
  /// If that isn't available, for now we just use the 0th SurfaceFormatKHR.
  fn choose_swap_surface_format(
    available_formats: &[vk::SurfaceFormatKHR],
  ) -> vk::SurfaceFormatKHR {
    *available_formats
      .iter()
      .find(|format| {
        format.format == vk::Format::B8G8R8A8_UNORM
          && format.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR
      })
      .unwrap_or(&available_formats[0])
  }

  /// Selects Mailbox if available, but if not tries to fallback to FIFO. See the [spec](https://renderdoc.org/vkspec_chunked/chap32.html#VkPresentModeKHR) for details on modes.
  ///
  /// TODO(issue#18) CONFIG support immediate mode if possible and allow the
  /// user to have tearing if they wish.
  fn choose_presentation_mode(
    available_presentation_modes: &[vk::PresentModeKHR], requested_present_mode: PresentMode,
  ) -> vk::PresentModeKHR {
    let present_mode = *available_presentation_modes
      .iter()
      .find(|&pm| match (requested_present_mode, pm) {
        (PresentMode::Mailbox, &vk::PresentModeKHR::MAILBOX) => true,
        (PresentMode::Immediate, &vk::PresentModeKHR::IMMEDIATE) => true,
        (PresentMode::Fifo, &vk::PresentModeKHR::FIFO) => true,
        _ => false,
      })
      .unwrap_or(&vk::PresentModeKHR::FIFO);

    info!("Selecting present mode: {:?}", present_mode);
    present_mode
  }

  /// Selects the resolution of the swap chain images.
  /// This is almost always equal to the resolution of the Surface we're drawing
  /// too, but we need to double check since some window managers allow us to
  /// differ.
  fn choose_swap_extent(
    capabilities: &vk::SurfaceCapabilitiesKHR, requested_width: u32, requested_height: u32,
  ) -> vk::Extent2D {
    if capabilities.current_extent.width != u32::max_value() {
      return capabilities.current_extent;
    }
    // The window system indicates that we can specify our own extent if this is
    // true
    let clipped_requested_width = requested_width.min(capabilities.max_image_extent.width);
    let width = capabilities
      .min_image_extent
      .width
      .max(clipped_requested_width);
    let clipped_requested_height = requested_height.min(capabilities.max_image_extent.height);
    let height = capabilities
      .min_image_extent
      .height
      .max(clipped_requested_height);

    if width != requested_width || height != requested_height {
      warn!(
        "Could not create a swapchain with the requested height and width, rendering to a \
         resolution of {}x{} instead",
        width, height
      );
    }

    vk::Extent2D::builder().width(width).height(height).build()
  }

  /// Given the render target images and format, create an image view suitable
  /// for rendering on. (one level, no mipmapping, color bit access).
  fn create_render_target_image_views(
    logical_device: &Arc<Device>, targets: Vec<vk::Image>, format: vk::Format,
  ) -> SarektResult<Vec<ImageAndView>> {
    let mut views = Vec::with_capacity(targets.len());
    for &image in targets.iter() {
      // Not swizzling rgba around.
      let component_mapping = vk::ComponentMapping::default();
      let image_subresource_range = vk::ImageSubresourceRange::builder()
        .aspect_mask(vk::ImageAspectFlags::COLOR) // We're writing color to this view
        .base_mip_level(0) // access to all mipmap levels
        .level_count(1) // Only one level, no mipmapping
        .base_array_layer(0) // access to all layers
        .layer_count(1) // Only one layer. (not sterescopic)
        .build();

      let ci = vk::ImageViewCreateInfo::builder()
        .image(image)
        .view_type(vk::ImageViewType::TYPE_2D)
        .format(format)
        .components(component_mapping)
        .subresource_range(image_subresource_range);

      let view = unsafe { logical_device.create_image_view(&ci, None)? };
      unsafe { views.push(ImageAndView::new(image, view)) };
    }
    Ok(views)
  }
}
