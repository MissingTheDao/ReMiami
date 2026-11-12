#version 450
// Test shader for retro-wgpu: shows the previous frame's original image (OriginalHistory1),
// scaled by a parameter.
#pragma parameter GAIN "Gain" 1.0 0.0 1.0 0.5
layout(std140, set = 0, binding = 0) uniform UBO {
    mat4 MVP;
    float GAIN;
} global;

#pragma stage vertex
layout(location = 0) in vec4 Position;
layout(location = 1) in vec2 TexCoord;
layout(location = 0) out vec2 vTexCoord;
void main() {
    gl_Position = global.MVP * Position;
    vTexCoord = TexCoord;
}

#pragma stage fragment
layout(location = 0) in vec2 vTexCoord;
layout(location = 0) out vec4 FragColor;
layout(set = 0, binding = 2) uniform sampler2D OriginalHistory1;
void main() {
    FragColor = vec4(texture(OriginalHistory1, vTexCoord).rgb * global.GAIN, 1.0);
}
