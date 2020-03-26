#version 450
// #extension GL_ARB_separate_shader_objects : enable

layout(binding = 0) uniform DefaultForwardShaderUniform {
  mat4 mvp;
  int enableColorMixing;
  int enableTextureMixing;
} ubo;

layout(location = 0)in vec3 inPosition;
layout(location = 1)in vec3 inColor;
layout(location = 2)in vec2 inTexCoord;

layout(location = 0)out vec3 fragColor;
flat layout(location = 1)out int enableTextureMixing;
layout(location = 2)out vec2 fragTexCoord;

void main() {
  gl_Position = ubo.mvp * vec4(inPosition, 1.0);

  if (ubo.enableColorMixing != 0) {
    fragColor = inColor;
  } else {
    fragColor = vec3(1.0);
  }

  enableTextureMixing = ubo.enableTextureMixing;

  fragTexCoord = inTexCoord;
}