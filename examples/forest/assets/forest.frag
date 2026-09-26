#version 330 core
in vec2 vUv;
out vec4 oColor;

uniform sampler2D uColor;
uniform sampler2D uDepth;
uniform sampler2DShadow uShadow;
uniform sampler2D uEnv;
uniform mat4 uLight;
uniform vec2 uResolution;
uniform vec3 uEye, uForward, uRight, uUp, uSun;
uniform vec4 uGlass, uMetal;
uniform float uIor, uRoughness, uShaftStrength, uExposure, uTime;

const float PI = 3.14159265359;
const float TAN30 = 0.57735026919;
const float NEAR = 0.15;
const float FAR = 200.0;

vec3 rayDirection(vec2 uv) {
    vec2 p = uv * 2.0 - 1.0;
    p.x *= uResolution.x / uResolution.y;
    return normalize(uForward + TAN30 * (p.x * uRight + p.y * uUp));
}

float sceneDistance(float depth, vec3 ray) {
    if (depth >= 0.99999) return 35.0;
    float z = depth * 2.0 - 1.0;
    float viewZ = 2.0 * NEAR * FAR / (FAR + NEAR - z * (FAR - NEAR));
    return viewZ / max(dot(ray, uForward), 0.1);
}

float sphereNear(vec3 ro, vec3 rd, vec4 sphere) {
    vec3 oc = ro - sphere.xyz;
    float b = dot(oc, rd);
    float c = dot(oc, oc) - sphere.w * sphere.w;
    float h = b * b - c;
    if (h < 0.0) return 1e5;
    float t = -b - sqrt(h);
    if (t < 0.001) t = -b + sqrt(h);
    return t > 0.001 ? t : 1e5;
}

float shadowAt(vec3 p) {
    vec4 projected = uLight * vec4(p, 1.0);
    vec3 q = projected.xyz / projected.w * 0.5 + 0.5;
    if (q.x < 0.0 || q.x > 1.0 || q.y < 0.0 || q.y > 1.0 || q.z > 1.0) return 1.0;
    return texture(uShadow, vec3(q.xy, q.z - 0.0017));
}

vec3 environmentRough(vec3 dir, float roughness) {
    vec2 uv = vec2(atan(dir.z, dir.x) / (2.0 * PI) + 0.5,
                   acos(clamp(dir.y, -1.0, 1.0)) / PI);
    // Poly Haven's tonemapped photograph is stored in sRGB.
    return pow(textureLod(uEnv, uv, roughness * 6.0).rgb, vec3(2.2)) * 1.15;
}

vec3 environment(vec3 dir) { return environmentRough(dir, 0.0); }

vec2 projectDirection(vec3 direction) {
    float forward = max(dot(direction, uForward), 0.05);
    vec2 p = vec2(dot(direction, uRight), dot(direction, uUp));
    p /= forward * TAN30;
    p.x /= uResolution.x / uResolution.y;
    return p * 0.5 + 0.5;
}

vec3 backdrop(vec2 uv) {
    uv = clamp(uv, vec2(0.002), vec2(0.998));
    float depth = texture(uDepth, uv).r;
    if (depth >= 0.99999) {
        return environment(rayDirection(uv)) * 0.38;
    }
    vec3 color = texture(uColor, uv).rgb;
    vec3 cameraRay = rayDirection(uv);
    vec3 point = uEye + cameraRay * sceneDistance(depth, cameraRay);
    // Apply sphere shadows wherever the mesh scene is sampled, including
    // through glass and in screen-space reflections.
    if (sphereNear(point + uSun * 0.01, uSun, uMetal) < 12.0) color *= 0.40;
    if (sphereNear(point + uSun * 0.01, uSun, uGlass) < 12.0)
        color *= vec3(0.72, 0.86, 0.93);
    return color;
}

vec3 shaftsFrom(vec3 origin, vec3 direction, float distance, vec2 seed) {
    float limit = clamp(distance, 0.0, 25.0);
    if (limit < 0.01) return vec3(0.0);
    float stepSize = limit / 20.0;
    float jitter = fract(sin(dot(seed, vec2(127.1, 311.7))) * 43758.5453);
    float light = 0.0;
    for (int i = 0; i < 20; i++) {
        float t = (float(i) + jitter) * stepSize;
        vec3 point = origin + direction * t;
        float visibility = shadowAt(point);
        if (sphereNear(point, uSun, uMetal) < 10.0) visibility *= 0.3;
        if (sphereNear(point, uSun, uGlass) < 10.0) visibility *= 0.78;
        float belowCanopy = 1.0 - smoothstep(2.0, 5.1, point.y);
        light += visibility * belowCanopy * exp(-0.065 * t) * stepSize;
    }
    float phase = 0.43 + 0.57 * pow(max(dot(direction, uSun), 0.0), 7.0);
    return vec3(1.0, 0.47, 0.19) * light * uShaftStrength * phase * 0.135;
}

// Find visible mesh color along a reflected world-space ray. The depth buffer
// only covers the current camera view; the environment is used outside it.
bool sceneAlongRay(vec3 origin, vec3 direction, out vec2 hitUv, out float hitT) {
    for (int i = 0; i < 24; i++) {
        float t = 0.25 + float(i) * 0.85;
        vec3 point = origin + direction * t;
        vec3 toPoint = point - uEye;
        if (dot(toPoint, uForward) <= 0.1) break;
        vec2 uv = projectDirection(normalize(toPoint));
        if (uv.x <= 0.002 || uv.x >= 0.998 || uv.y <= 0.002 || uv.y >= 0.998) break;
        float depth = texture(uDepth, uv).r;
        if (depth >= 0.99999) continue;
        vec3 cameraRay = rayDirection(uv);
        float surfaceZ = sceneDistance(depth, cameraRay) * dot(cameraRay, uForward);
        float gap = dot(toPoint, uForward) - surfaceZ;
        if (gap >= 0.0 && gap < 0.75) {
            hitUv = uv;
            hitT = t;
            return true;
        }
    }
    hitUv = vec2(0.0);
    hitT = 25.0;
    return false;
}

float fresnel(float cosine, float ior) {
    float f0 = pow((ior - 1.0) / (ior + 1.0), 2.0);
    return f0 + (1.0 - f0) * pow(1.0 - clamp(cosine, 0.0, 1.0), 5.0);
}

vec3 shadeGlass(vec3 rd, vec3 p, vec3 n, float sceneT, float glassT, vec2 seed) {
    float cosi = max(dot(-rd, n), 0.0);
    float f = fresnel(cosi, uIor);
    vec3 inside = refract(rd, n, 1.0 / uIor);
    // Intersect the inside ray with the back of the same sphere.
    vec3 entry = p + inside * 0.002;
    vec3 oc = entry - uGlass.xyz;
    float exitT = -dot(oc, inside) + sqrt(max(0.0,
        dot(oc, inside) * dot(oc, inside) - dot(oc, oc) + uGlass.w * uGlass.w));
    vec3 exitPoint = entry + inside * exitT;
    vec3 exitNormal = normalize(exitPoint - uGlass.xyz);
    vec3 outgoing = refract(inside, -exitNormal, uIor);
    if (length(outgoing) < 0.1) outgoing = reflect(inside, -exitNormal);
    // The mesh scene is already rendered behind the analytic glass surface.
    // Project a displaced transmitted ray into it for visible leaf refraction.
    vec3 backgroundPoint = exitPoint + outgoing * max(sceneT - glassT, 1.0);
    vec2 refractedUv = projectDirection(normalize(backgroundPoint - uEye));
    bool onScreen = refractedUv.x > 0.002 && refractedUv.x < 0.998
                 && refractedUv.y > 0.002 && refractedUv.y < 0.998;
    vec3 transmitted = onScreen ? backdrop(refractedUv) : environment(outgoing) * 0.38;
    float transmittedT = max(sceneT - glassT, 1.0);
    if (onScreen) {
        float depth = texture(uDepth, refractedUv).r;
        if (depth < 0.99999) {
            vec3 cameraRay = rayDirection(refractedUv);
            vec3 surface = uEye + cameraRay * sceneDistance(depth, cameraRay);
            transmittedT = max(dot(surface - exitPoint, outgoing), 0.5);
        }
    }
    transmitted += shaftsFrom(exitPoint + outgoing * 0.01, outgoing,
                              transmittedT, seed + vec2(41.0, 17.0));
    vec3 absorption = pow(vec3(0.91, 0.96, 0.985), vec3(exitT * 1.8));
    vec3 reflectedRay = reflect(rd, n);
    vec3 reflected = environment(reflectedRay);
    vec2 hitUv;
    float hitT;
    if (sceneAlongRay(p + n * 0.01, reflectedRay, hitUv, hitT)) {
        reflected = mix(reflected, backdrop(hitUv), 0.55);
    }
    reflected += shaftsFrom(p + n * 0.01, reflectedRay, hitT,
                            seed + vec2(13.0, 67.0));
    float sunSpec = pow(max(dot(reflectedRay, uSun), 0.0), 150.0);
    return transmitted * absorption * (1.0 - f) + reflected * f * 1.12
        + vec3(1.0, 0.79, 0.54) * sunSpec * 1.5;
}

vec3 shadeMetal(vec3 rd, vec3 p, vec3 n, vec2 seed) {
    vec3 refl = reflect(rd, n);
    vec3 mirrored = environmentRough(refl, uRoughness);
    vec2 hitUv;
    float hitT;
    if (sceneAlongRay(p + n * 0.01, refl, hitUv, hitT)) {
        mirrored = mix(mirrored, backdrop(hitUv), 0.55 * (1.0 - uRoughness));
    }
    // Reflected rays see the shadowed air between the sphere and the scene.
    mirrored += shaftsFrom(p + n * 0.01, refl, hitT,
                           seed + vec2(71.0, 29.0)) * (1.0 - 0.3 * uRoughness);
    vec3 base = vec3(0.95, 0.72, 0.43);
    float sunSpec = pow(max(dot(refl, uSun), 0.0), mix(210.0, 9.0, uRoughness));
    float skyFill = 0.12 + 0.18 * max(n.y, 0.0);
    return mirrored * base * 1.25 + base * skyFill
        + vec3(1.0, 0.70, 0.39) * sunSpec * (2.2 - uRoughness);
}

vec3 filmic(vec3 x) {
    x = max(x, vec3(0.0));
    return clamp((x * (2.51*x + 0.03)) / (x * (2.43*x + 0.59) + 0.14), 0.0, 1.0);
}

void main() {
    vec2 uv = vUv;
    vec3 rd = rayDirection(uv);
    float rawDepth = texture(uDepth, uv).r;
    float sceneT = sceneDistance(rawDepth, rd);
    vec3 color = backdrop(uv);

    float glassT = sphereNear(uEye, rd, uGlass);
    float metalT = sphereNear(uEye, rd, uMetal);
    float visibleT = sceneT;
    if (glassT < sceneT && glassT < metalT) {
        vec3 p = uEye + rd * glassT;
        color = shadeGlass(rd, p, normalize(p - uGlass.xyz), sceneT, glassT,
                           gl_FragCoord.xy);
        visibleT = glassT;
    } else if (metalT < sceneT) {
        vec3 p = uEye + rd * metalT;
        color = shadeMetal(rd, p, normalize(p - uMetal.xyz), gl_FragCoord.xy);
        visibleT = metalT;
    }

    color += shaftsFrom(uEye, rd, visibleT, gl_FragCoord.xy);
    color *= uExposure;
    color = filmic(color);
    // A slight warm grade keeps the scene orange without flattening the bark.
    color *= vec3(1.04, 0.985, 0.92);
    oColor = vec4(pow(clamp(color, 0.0, 1.0), vec3(1.0 / 2.2)), 1.0);
}
