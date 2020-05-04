/// Sarekt configuration.  Sane defaults provided (no AA, etc).
#[derive(Builder)]
#[builder(default)]
pub struct Config {
  pub requested_width: u32,
  pub requested_height: u32,
  pub application_details: ApplicationDetails<'static>,
  pub engine_details: EngineDetails<'static>,
  pub present_mode: PresentMode,
  pub aa_config: AntiAliasingConfig,
}
impl Config {
  pub fn builder() -> ConfigBuilder {
    ConfigBuilder::default()
  }
}
impl<'a> Default for Config {
  fn default() -> Self {
    Self {
      requested_width: 800,
      requested_height: 600,
      application_details: ApplicationDetails::default(),
      engine_details: EngineDetails::default(),
      present_mode: PresentMode::default(),
      aa_config: AntiAliasingConfig::default(),
    }
  }
}

// ================================================================================
//  Version struct
// ================================================================================
/// A simple version with major, minor and patch fields for specifying
/// information about your application.
#[derive(Copy, Clone)]
pub struct Version {
  major: u32,
  minor: u32,
  patch: u32,
}
impl Version {
  pub fn new(major: u32, minor: u32, patch: u32) -> Self {
    Self {
      major,
      minor,
      patch,
    }
  }
}
impl Default for Version {
  fn default() -> Self {
    Self {
      major: 0,
      minor: 1,
      patch: 0,
    }
  }
}

// ================================================================================
//  ApplicationDetails Struct
// ================================================================================
/// Application Details and version for your application.
#[derive(Copy, Clone)]
pub struct ApplicationDetails<'a> {
  pub name: &'a str,
  pub version: Version,
}
impl<'a> ApplicationDetails<'a> {
  pub fn new(name: &'a str, version: Version) -> Self {
    Self { name, version }
  }

  /// Get Major Minor Patch in a single u32.
  pub fn get_u32_version(self) -> u32 {
    ash::vk::make_version(self.version.major, self.version.minor, self.version.patch)
  }
}
impl<'a> Default for ApplicationDetails<'a> {
  fn default() -> Self {
    Self {
      name: "Nameless Application",
      version: Version::new(0, 1, 0),
    }
  }
}

// ================================================================================
//  EngineDetails Struct
// ================================================================================
/// Application Details and version for your engine.
#[derive(Copy, Clone)]
pub struct EngineDetails<'a> {
  pub name: &'a str,
  pub version: Version,
}
impl<'a> EngineDetails<'a> {
  pub fn new(name: &'a str, version: Version) -> Self {
    Self { name, version }
  }

  /// Get Major Minor Patch in a single u32.
  pub fn get_u32_version(self) -> u32 {
    ash::vk::make_version(self.version.major, self.version.minor, self.version.patch)
  }
}
impl<'a> Default for EngineDetails<'a> {
  fn default() -> Self {
    Self {
      name: "Nameless Engine",
      version: Version::new(0, 1, 0),
    }
  }
}

/// Determines Present mode, default is Mailbox if possible to allow for
/// framerate equal to screen refresh while continuing to draw.
#[derive(Copy, Clone)]
pub enum PresentMode {
  Immediate,
  Mailbox,
  Fifo,
}
impl Default for PresentMode {
  fn default() -> PresentMode {
    PresentMode::Mailbox
  }
}

/// Configuration for AA.  Must be a power of 2.
/// TODO(issue#32) make issue for SSAA.
/// TODO(issue#33) other AA styles.
#[derive(Copy, Clone)]
pub enum AntiAliasingConfig {
  MSAA(NumSamples),
  // TODO(issue#32) just here to get rid of lint errors, remove when there are more types.
  Unreachable,
}
impl Default for AntiAliasingConfig {
  fn default() -> AntiAliasingConfig {
    AntiAliasingConfig::MSAA(NumSamples::default())
  }
}
#[derive(Copy, Clone)]
pub enum NumSamples {
  One,
  Two,
  Four,
  Eight,
}
impl Default for NumSamples {
  fn default() -> NumSamples {
    NumSamples::One
  }
}
