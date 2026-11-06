#version 330 core
// gpu_glsl_test.PROG (vertex)
struct Quad {
    vec2 q;
};
struct Inst {
    vec4 rect;
    vec2 k;
};
struct V {
    vec4 position;
    vec2 uv;
    vec4 rect;
};
layout(location=0) in vec2 a_q_q;
layout(location=1) in vec4 a_i_rect;
layout(location=2) in vec2 a_i_k;
out vec2 v_uv;
flat out vec4 v_rect;
uniform vec2 uScreen;
// gpu_glsl_test.j3:41
V vs(Quad q, Inst i) {
    vec2 p = i.rect.xy + (q.q * i.rect.zw);
    vec2 ndc = ((p / uScreen) * 2.0) - 1.0;
    return V(vec4(ndc.x, (-ndc.y), 0.0, 1.0), q.q, i.rect);
}
void main() {
    Quad p_0 = Quad(a_q_q);
    Inst p_1 = Inst(a_i_rect, a_i_k);
    V p_ret = vs(p_0, p_1);
    gl_Position = p_ret.position;
    v_uv = p_ret.uv;
    v_rect = p_ret.rect;
}
