#version 300 es
// gpu_glsl_test.QUIRKS (fragment)
precision highp float;precision highp int;struct W {
    vec4 position;
    vec2 j3_mix;
};
in vec2 v_j3_mix;
layout(location=0) out vec4 oColor;
// gpu_glsl_test.j3:140
vec4 fq(W j3_output) {
    return vec4(j3_output.j3_mix, 0.0, 1.0);
}
void main() {
    W p_0 = W(gl_FragCoord, v_j3_mix);
    oColor = fq(p_0);
}
