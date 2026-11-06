#version 330 core
// gpu_glsl_test.FULL (fragment)
struct V {
    vec4 position;
    vec2 uv;
    vec4 rect;
};
in vec2 v_uv;
flat in vec4 v_rect;
layout(location=0) out vec4 oColor;
// gpu_glsl_test.j3:66
vec4 fsKinds(V v) {
    int kind = int(v.rect.x);
    vec4 c = vec4(0.0);
    switch (kind) {
        case 0: {
            c = vec4(v.uv, 0.0, 1.0);
            break;
        }
        case 1: {
            c = vec4(1.0);
            break;
        }
        default: {
            c = vec4(mix(vec3(0.0), vec3(1.0), 0.5), 1.0);
            break;
        }
    }
    return c;
}
void main() {
    V p_0 = V(gl_FragCoord, v_uv, v_rect);
    oColor = fsKinds(p_0);
}
