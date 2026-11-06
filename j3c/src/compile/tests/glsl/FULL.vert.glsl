#version 330 core
// gpu_glsl_test.FULL (vertex)
struct V {
    vec4 position;
    vec2 uv;
    vec4 rect;
};
out vec2 v_uv;
flat out vec4 v_rect;
// gpu_glsl_test.j3:59
V vsFull() {
    int id = gl_VertexID;
    vec2 p = vec2(float(((id << 1) & 2)), float((id & 2)));
    return V(vec4(((p * 2.0) - 1.0), 0.0, 1.0), p, vec4(0.0));
}
void main() {
    V p_ret = vsFull();
    gl_Position = p_ret.position;
    v_uv = p_ret.uv;
    v_rect = p_ret.rect;
}
