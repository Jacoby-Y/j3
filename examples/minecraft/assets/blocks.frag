#version 330 core
in vec2 vUv;
out vec4 oColor;

uniform sampler2D uColor;
uniform sampler2D uDepth;
uniform sampler2DShadow uShadow;
uniform sampler2D uAtlas;
uniform sampler3D uVoxels;
uniform mat4 uLight;
uniform vec2 uResolution;
uniform vec3 uEye, uForward, uRight, uUp, uSun;
uniform vec4 uBlocks[64];
uniform int uBlockCount;
uniform float uIor, uRoughness, uShaftStrength, uExposure;

const float TAN30 = 0.57735026919;
const float NEAR = 0.15;
const float FAR = 200.0;

vec3 rayDirection(vec2 uv) {
    vec2 p = uv * 2.0 - 1.0;
    p.x *= uResolution.x / uResolution.y;
    return normalize(uForward + TAN30 * (p.x * uRight + p.y * uUp));
}

float sceneDistance(float depth, vec3 ray) {
    if (depth >= 0.99999) return 85.0;
    float z = depth * 2.0 - 1.0;
    float viewZ = 2.0 * NEAR * FAR / (FAR + NEAR - z * (FAR - NEAR));
    return viewZ / max(dot(ray, uForward), 0.08);
}

vec3 sky(vec3 d) {
    float horizon = smoothstep(-0.16, 0.65, d.y);
    vec3 c = mix(vec3(0.47, 0.56, 0.58), vec3(0.14, 0.35, 0.61), horizon);
    float halo = pow(max(dot(d, uSun), 0.0), 56.0);
    float disc = pow(max(dot(d, uSun), 0.0), 1200.0);
    return c + vec3(1.0, 0.69, 0.34) * halo * 0.6 + vec3(3.0, 2.2, 1.4) * disc;
}

float shadowAt(vec3 p) {
    vec4 h = uLight * vec4(p, 1.0);
    vec3 q = h.xyz / h.w * 0.5 + 0.5;
    if (any(lessThan(q, vec3(0.0))) || any(greaterThan(q, vec3(1.0)))) return 1.0;
    return texture(uShadow, vec3(q.xy, q.z - 0.0017));
}

// Ray/AABB hit. The outward normal belongs to the near face.
bool boxHit(vec3 ro, vec3 rd, vec3 b, out float nearT, out float farT, out vec3 normal) {
    vec3 safeDir = vec3(abs(rd.x) < 1e-5 ? 1e-5 : rd.x,
                        abs(rd.y) < 1e-5 ? 1e-5 : rd.y,
                        abs(rd.z) < 1e-5 ? 1e-5 : rd.z);
    vec3 t0 = (b - ro) / safeDir;
    vec3 t1 = (b + 1.0 - ro) / safeDir;
    vec3 lo = min(t0, t1);
    vec3 hi = max(t0, t1);
    nearT = max(max(lo.x, lo.y), lo.z);
    farT = min(min(hi.x, hi.y), hi.z);
    normal = vec3(0.0);
    if (lo.x >= lo.y && lo.x >= lo.z) normal.x = -sign(rd.x);
    else if (lo.y >= lo.z) normal.y = -sign(rd.y);
    else normal.z = -sign(rd.z);
    return farT > max(nearT, 0.0);
}

float firstBlock(vec3 ro, vec3 rd, float limit, out int hitId, out vec3 normal) {
    float best = limit;
    hitId = -1;
    normal = vec3(0.0);
    for (int i = 0; i < 64; i++) {
        if (i >= uBlockCount) break;
        float t0, t1;
        vec3 n;
        if (boxHit(ro, rd, uBlocks[i].xyz, t0, t1, n) && t0 > 0.001 && t0 < best) {
            best = t0;
            hitId = i;
            normal = n;
        }
    }
    return best;
}

vec3 baseScene(vec2 uv) {
    if (any(lessThanEqual(uv, vec2(0.0))) || any(greaterThanEqual(uv, vec2(1.0))))
        return sky(rayDirection(clamp(uv, 0.0, 1.0)));
    if (texture(uDepth, uv).r >= 0.99999) return sky(rayDirection(uv));
    return texture(uColor, uv).rgb;
}

vec3 shaftsFrom(vec3 ro, vec3 rd, float distance, vec2 seed) {
    if (uShaftStrength < 0.001) return vec3(0.0);
    float lengthT = min(distance, 28.0);
    if (lengthT < 0.01) return vec3(0.0);
    float stepT = lengthT / 16.0;
    float jitter = fract(sin(dot(seed, vec2(127.1, 311.7))) * 43758.5453);
    float sum = 0.0;
    for (int i = 0; i < 16; i++) {
        float t = (float(i) + jitter) * stepT;
        vec3 p = ro + rd * t;
        // The volume is strongest beneath the tree crowns and fades into the sky.
        float mist = 1.0 - smoothstep(15.0, 25.0, p.y);
        sum += shadowAt(p) * mist * exp(-t * 0.055) * stepT;
    }
    float phase = 0.32 + 0.68 * pow(max(dot(rd, uSun), 0.0), 6.0);
    return vec3(1.0, 0.71, 0.43) * sum * phase * uShaftStrength * 0.031;
}

int voxel(ivec3 cell) {
    if (any(lessThan(cell, ivec3(0))) || any(greaterThanEqual(cell, ivec3(48, 32, 48)))) return 0;
    return int(texelFetch(uVoxels, ivec3(cell.x, cell.z, cell.y), 0).r * 255.0 + 0.5);
}

// DDA over the same edited cells used by the raster mesh and game collision.
bool traceVoxel(vec3 origin, vec3 direction, float limit,
                out vec3 hitPoint, out vec3 hitNormal, out int hitId, out float hitT) {
    vec3 ro = origin + direction * 0.012;
    ivec3 cell = ivec3(floor(ro));
    ivec3 stepDir = ivec3(sign(direction));
    vec3 inv = 1.0 / max(abs(direction), vec3(1e-5));
    vec3 boundary = vec3(cell) + max(vec3(stepDir), vec3(0.0));
    vec3 nextT = abs(boundary - ro) * inv;
    vec3 deltaT = inv;
    hitNormal = vec3(0.0);
    hitT = 0.0;
    for (int i = 0; i < 110; i++) {
        if (hitT > limit) break;
        int id = voxel(cell);
        if (id > 0) {
            hitId = id;
            hitPoint = ro + direction * hitT;
            return true;
        }
        if (nextT.x <= nextT.y && nextT.x <= nextT.z) {
            hitT = nextT.x;
            nextT.x += deltaT.x;
            cell.x += stepDir.x;
            hitNormal = vec3(-float(stepDir.x), 0.0, 0.0);
        } else if (nextT.y <= nextT.z) {
            hitT = nextT.y;
            nextT.y += deltaT.y;
            cell.y += stepDir.y;
            hitNormal = vec3(0.0, -float(stepDir.y), 0.0);
        } else {
            hitT = nextT.z;
            nextT.z += deltaT.z;
            cell.z += stepDir.z;
            hitNormal = vec3(0.0, 0.0, -float(stepDir.z));
        }
    }
    hitPoint = ro + direction * limit;
    hitId = 0;
    hitT = limit;
    return false;
}

int tileFor(int id, vec3 n) {
    if (id == 1) {
        if (n.y > 0.5) return 0;
        if (n.y < -0.5) return 2;
        return 1;
    }
    if (id == 2) return 2;
    if (id == 3) return 3;
    if (id == 4) return 4;
    if (id == 5) return abs(n.y) > 0.5 ? 6 : 5;
    if (id == 6) return 7;
    if (id == 7) return 8;
    if (id == 10) return 9;
    return 10;
}

vec3 voxelColor(vec3 p, vec3 n, int id) {
    if (id == 8) return vec3(0.21, 0.63, 0.71);
    if (id == 9) return vec3(0.52, 0.58, 0.59);
    int tile = tileFor(id, n);
    vec3 f = fract(p + vec3(0.0003));
    vec2 face = abs(n.x) > 0.5 ? f.zy : (abs(n.y) > 0.5 ? f.xz : f.xy);
    vec2 uv = (vec2(float(tile % 8), float(tile / 8)) * 32.0 + 2.0 + face * 28.0) / vec2(256.0, 64.0);
    vec3 albedo = texture(uAtlas, uv).rgb;
    float lit = max(dot(n, uSun), 0.0) * shadowAt(p + n * 0.01);
    vec3 ambient = mix(vec3(0.10, 0.15, 0.19), vec3(0.24, 0.30, 0.33), n.y * 0.5 + 0.5);
    return albedo * (ambient + vec3(0.94, 0.82, 0.63) * lit);
}

vec3 reflectedScene(vec3 origin, vec3 direction, vec2 seed, float rough) {
    vec3 p, n;
    int id;
    float hitT;
    vec3 color = sky(direction);
    if (traceVoxel(origin, direction, 64.0, p, n, id, hitT)) {
        color = voxelColor(p, n, id);
        color = mix(color, sky(direction) * 0.32, rough * 0.38);
    }
    return color + shaftsFrom(origin, direction, hitT, seed + vec2(39.0, 71.0));
}

float glassEdge(vec3 p, vec3 n) {
    vec3 q = fract(p + vec3(0.0002));
    vec2 face = abs(n.x) > 0.5 ? q.yz : (abs(n.y) > 0.5 ? q.xz : q.xy);
    float edge = min(min(face.x, 1.0 - face.x), min(face.y, 1.0 - face.y));
    return 1.0 - smoothstep(0.018, 0.065, edge);
}

vec3 shadeGlass(vec3 rd, vec3 p, vec3 n, vec3 box, vec2 seed) {
    float f0 = pow((uIor - 1.0) / (uIor + 1.0), 2.0);
    float fresnel = f0 + (1.0 - f0) * pow(1.0 - max(dot(-rd, n), 0.0), 5.0);
    vec3 inside = refract(rd, n, 1.0 / uIor);
    if (length(inside) < 0.1) inside = rd;
    vec3 entry = p + inside * 0.003;
    float t0, t1;
    vec3 ignoreNormal;
    boxHit(entry, inside, box, t0, t1, ignoreNormal);
    vec3 exitPoint = entry + inside * t1;
    vec3 q = exitPoint - box;
    vec3 exitNormal = vec3(0.0);
    if (min(q.x, 1.0 - q.x) < 0.002) exitNormal.x = q.x < 0.5 ? -1.0 : 1.0;
    else if (min(q.y, 1.0 - q.y) < 0.002) exitNormal.y = q.y < 0.5 ? -1.0 : 1.0;
    else exitNormal.z = q.z < 0.5 ? -1.0 : 1.0;
    vec3 outgoing = refract(inside, -exitNormal, uIor);
    if (length(outgoing) < 0.1) outgoing = reflect(inside, -exitNormal);

    vec3 hitPoint, hitNormal;
    int hitId;
    float refractT;
    vec3 transmitted = sky(outgoing);
    if (traceVoxel(exitPoint + outgoing * 0.02, outgoing, 64.0,
                   hitPoint, hitNormal, hitId, refractT))
        transmitted = voxelColor(hitPoint, hitNormal, hitId);
    transmitted += shaftsFrom(exitPoint + outgoing * 0.02, outgoing,
                              refractT, seed + vec2(13.0, 29.0));
    vec3 reflectedRay = reflect(rd, n);
    vec3 reflected = reflectedScene(p + n * 0.02, reflectedRay, seed, 0.0);
    vec3 absorption = pow(vec3(0.78, 0.96, 0.98), vec3(max(t1, 0.0)));
    float edge = glassEdge(p, n);
    vec3 glass = mix(transmitted * absorption, reflected, fresnel);
    glass = mix(glass, glass * 0.48 + vec3(0.03, 0.29, 0.32), edge * 0.7);
    return glass + vec3(1.0, 0.87, 0.65) * pow(max(dot(reflectedRay, uSun), 0.0), 220.0) * 1.3;
}

vec3 shadeMetal(vec3 rd, vec3 p, vec3 n, vec2 seed) {
    vec3 reflectedRay = reflect(rd, n);
    vec3 scene = reflectedScene(p + n * 0.02, reflectedRay, seed, uRoughness);
    vec3 f0 = vec3(0.78, 0.82, 0.83);
    vec3 fresnel = f0 + (1.0 - f0) * pow(1.0 - max(dot(-rd, n), 0.0), 5.0);
    float brush = sin(dot(p.xz, vec2(160.0, 93.0))) * 0.035;
    float rim = glassEdge(p, n);
    vec3 metal = scene * fresnel * (1.05 + brush) + vec3(0.08, 0.11, 0.12);
    metal *= mix(1.0, 0.86, rim);
    float spec = pow(max(dot(reflectedRay, uSun), 0.0), mix(280.0, 24.0, uRoughness));
    return metal + vec3(1.0, 0.90, 0.68) * spec * (1.8 - uRoughness);
}

vec3 filmic(vec3 x) {
    x = max(x, vec3(0.0));
    return clamp((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14), 0.0, 1.0);
}

void main() {
    vec3 rd = rayDirection(vUv);
    float sceneT = sceneDistance(texture(uDepth, vUv).r, rd);
    vec3 color = baseScene(vUv);
    int id;
    vec3 normal;
    float blockT = firstBlock(uEye, rd, sceneT, id, normal);
    float visibleT = sceneT;
    if (id >= 0) {
        vec3 p = uEye + rd * blockT;
        if (uBlocks[id].w < 1.5)
            color = shadeGlass(rd, p, normal, uBlocks[id].xyz, gl_FragCoord.xy);
        else
            color = shadeMetal(rd, p, normal, gl_FragCoord.xy);
        visibleT = blockT;
    }
    color += shaftsFrom(uEye, rd, visibleT, gl_FragCoord.xy);
    color = filmic(color * uExposure);
    oColor = vec4(pow(color, vec3(1.0 / 2.2)), 1.0);
}
