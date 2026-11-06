#version 300 es
// gpu_glsl_test.KITCHEN (fragment)
precision highp float;precision highp int;precision highp sampler2D;struct KV {
    vec4 position;
    vec2 uv;
    uint id;
    vec3 n;
};
struct Light {
    vec3 dir;
    float power;
};
in vec2 v_uv;
flat in uint v_id;
in vec3 v_n;
layout(location=0) out vec4 oColor;
uniform vec4[2] uLights;
uniform sampler2D uImg;
const float TAU = 6.28318;
const float PI = 3.14159;
// gpu_glsl_test.j3:95
void bump(inout float x, float by) {
    x += by;
}
// gpu_glsl_test.j3:100
float lighting(Light l, vec3 n) {
    return max(dot(n, normalize(l.dir)), 0.0) * l.power;
}
// gpu_glsl_test.j3:111
vec4 kfs(KV v) {
    vec4[2] lights = vec4[2](uLights[0], uLights[1]);
    float acc = 0.0;
    int i = 0;
    while (i < 2) {
        Light l = Light(lights[i].xyz, lights[i].w);
        bump(acc, lighting(l, normalize(v.n)));
        i += 1;
    }
    ivec2 size = textureSize(uImg, 0);
    vec4 t = texelFetch(uImg, ivec2(int((v.id & 3u)), 0), 0);
    float k = (v.id > 2u) ? 1.0 : acc;
    bool flag = ((v.id & 1u) == 1u) && (acc > 0.1);
    float shade = flag ? (TAU / PI) : 1.0;
    return vec4(((t.rgb * shade) * k), (float(size.x) / 256.0));
}
void main() {
    KV p_0 = KV(gl_FragCoord, v_uv, v_id, v_n);
    oColor = kfs(p_0);
}
