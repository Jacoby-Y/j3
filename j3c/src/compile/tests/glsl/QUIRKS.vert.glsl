#version 330 core
// gpu_glsl_test.QUIRKS (vertex)
struct A {
    vec2 p;
};
struct W {
    vec4 position;
    vec2 j3_mix;
};
layout(location=0) in vec2 a_j3_input_p;
layout(location=1) in vec2 a_r_p;
out vec2 v_j3_mix;
// gpu_glsl_test.j3:135
W vq(A j3_input, A r) {
    return W(vec4((j3_input.p + r.p), 0.0, 1.0), r.p);
}
void main() {
    A p_0 = A(a_j3_input_p);
    A p_1 = A(a_r_p);
    W p_ret = vq(p_0, p_1);
    gl_Position = p_ret.position;
    v_j3_mix = p_ret.j3_mix;
}
