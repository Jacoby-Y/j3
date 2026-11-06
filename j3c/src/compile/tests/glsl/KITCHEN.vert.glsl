#version 330 core
// gpu_glsl_test.KITCHEN (vertex)
struct Mesh {
    vec3 pos;
    vec2 uv;
    uint id;
};
struct Model {
    mat4 m;
    vec4 tint;
};
struct KV {
    vec4 position;
    vec2 uv;
    uint id;
    vec3 n;
};
layout(location=0) in vec3 a_m_pos;
layout(location=1) in vec2 a_m_uv;
layout(location=2) in uint a_m_id;
layout(location=3) in mat4 a_i_m;
layout(location=7) in vec4 a_i_tint;
out vec2 v_uv;
flat out uint v_id;
out vec3 v_n;
uniform mat4 uViewProj;
// gpu_glsl_test.j3:105
KV kvs(Mesh m, Model i) {
    vec4 w = i.m * vec4(m.pos, 1.0);
    return KV((uViewProj * w), m.uv, m.id, w.xyz);
}
void main() {
    Mesh p_0 = Mesh(a_m_pos, a_m_uv, a_m_id);
    Model p_1 = Model(a_i_m, a_i_tint);
    KV p_ret = kvs(p_0, p_1);
    gl_Position = p_ret.position;
    v_uv = p_ret.uv;
    v_id = p_ret.id;
    v_n = p_ret.n;
}
