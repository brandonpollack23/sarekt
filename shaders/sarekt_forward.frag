#version 450
#extension GL_ARB_separate_shader_objects : enable

layout(binding = 1)uniform sampler2D texSampler;

layout(location = 0)in vec3 fragColor;
flat layout(location = 1)in int enableTextureMixing;
layout(location = 2)in vec2 fragTexCoord;

layout(location = 0)out vec4 outColor;

void main() {
  // TODO CRITICAL SHADERS enable/disable fragColor mixing.
  vec3 colorFromFragColor = fragColor;

  vec4 colorFromTexture;
  if (enableTextureMixing == 1) {
    colorFromTexture = texture(texSampler, fragTexCoord);
  } else {
    colorFromTexture = vec4(1.0);
  }

  // Alpha is from the texture alone.
  outColor = vec4(colorFromFragColor * colorFromTexture.rgb, colorFromTexture.a);
}