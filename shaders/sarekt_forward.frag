#version 450
#extension GL_ARB_separate_shader_objects : enable

#ifdef VULKAN
layout(binding = 0)
#endif
uniform DefaultForwardShaderUniform {
  mat4 mvp;
  int enableColorMixing;
  int enableTextureMixing;
} ubo;
#ifdef VULKAN

layout(binding = 1)
#endif
uniform sampler2D texSampler;

layout(location = 0)in vec3 fragColor;
layout(location = 1)in vec2 fragTexCoord;

layout(location = 0)out vec4 outColor;

void main() {
  vec3 colorFromFragColor = fragColor;

  vec4 colorFromTexture;
  if (ubo.enableTextureMixing == 1) {
    colorFromTexture = texture(texSampler, fragTexCoord);
  } else {
    colorFromTexture = vec4(1.0);
  }

  // Alpha is from the texture alone.
  outColor = vec4(colorFromFragColor * colorFromTexture.rgb, colorFromTexture.a);
}