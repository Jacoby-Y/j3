#version 330 core
// gpu_glsl_test.PROG (fragment)
struct V {
    vec4 position;
    vec2 uv;
    vec4 rect;
};
in vec2 v_uv;
flat in vec4 v_rect;
layout(location=0) out vec4 oColor;
uniform sampler2D uTex;
uniform vec4 uTint;
const float KERNEL[3] = float[3](0.25, 0.5, 0.25);
// gpu_glsl_test.j3:34
vec4 shade(vec4 j3_input, float j3_sample) {
    vec4 j3_mix = j3_input;
    j3_mix.rgb = (j3_mix.rgb * j3_sample) * uTint.rgb;
    return j3_mix;
}
// gpu_glsl_test.j3:48
vec4 fs(V v) {
    vec4 c = texture(uTex, v.uv);
    float a = 0.0;
    for (int k = 0; k < 3; k++) {
        a += KERNEL[k] * c.a;
    }
    if (a < 0.01) {
        discard;
    }
    return shade(c, a);
}
void main() {
    V p_0 = V(gl_FragCoord, v_uv, v_rect);
    oColor = fs(p_0);
}
