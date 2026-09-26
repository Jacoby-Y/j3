#version 330 core
in vec2 vUv;
out vec4 oColor;

uniform vec2 uResolution;
uniform vec3 uEye, uForward, uRight, uUp, uWaterColor;
uniform int uSphereCount, uBounces, uAaSamples;
uniform vec4 uSphere[16], uMatA[16], uMatB[16];
uniform float uTime, uWind, uSwell, uChoppiness, uSunHeight;

const float PI = 3.14159265359;
struct Hit { float t; vec3 p; vec3 n; int kind; int id; };
struct Work { vec3 o; vec3 d; vec3 weight; int depth; int medium; };

float hash21(vec2 p) {
    p = fract(p * vec2(123.34, 345.45));
    p += dot(p, p + 34.345);
    return fract(p.x * p.y);
}

float noise(vec2 p) {
    vec2 i = floor(p), f = fract(p);
    f = f*f*(3.0-2.0*f);
    return mix(mix(hash21(i), hash21(i+vec2(1,0)), f.x),
               mix(hash21(i+vec2(0,1)), hash21(i+vec2(1,1)), f.x), f.y);
}

float clouds(vec2 p) {
    float n = 0.5*noise(p*0.34);
    n += 0.3*noise(p*0.71+17.0);
    n += 0.2*noise(p*1.52-9.0);
    return smoothstep(0.48, 0.68, n);
}

vec3 sunDirection() { return normalize(vec3(-0.46, uSunHeight, -0.8)); }

vec3 sky(vec3 d) {
    vec3 sun = sunDirection();
    float h = max(d.y, 0.0);
    vec3 horizon = vec3(0.83, 0.84, 0.78);
    vec3 zenith = vec3(0.12, 0.35, 0.68);
    vec3 col = mix(horizon, zenith, pow(h, 0.43));
    col += vec3(0.78, 0.48, 0.24)*pow(max(dot(d, sun), 0.0), 9.0)*0.24;
    if (d.y > 0.04) {
        vec2 cp = d.xz/max(d.y, 0.08) + vec2(uTime*0.006, 0.0);
        float c = clouds(cp);
        col = mix(col, vec3(1.12, 1.05, 0.94), c*0.72*smoothstep(0.04, 0.24, d.y));
    }
    float sd = max(dot(d, sun), 0.0);
    col += vec3(1.0, 0.82, 0.54)*pow(sd, 240.0)*3.8;
    col += vec3(1.0, 0.64, 0.32)*pow(sd, 3500.0)*18.0;
    return col;
}

// Each component follows deep-water dispersion: angular frequency sqrt(g*k).
// The second harmonic sharpens crests without requiring a displaced mesh.
vec3 wavePart(vec2 p, vec2 dir, float k, float amplitude) {
    float phase = dot(p, dir)*k - uTime*sqrt(9.81*k);
    float c = uChoppiness;
    float height = amplitude*(sin(phase) + 0.22*c*sin(2.0*phase));
    float slope = amplitude*k*(cos(phase) + 0.44*c*cos(2.0*phase));
    return vec3(height, slope*dir.x, slope*dir.y);
}

vec3 waveData(vec2 p) {
    float s = uSwell, w = uWind;
    vec3 v = wavePart(p, normalize(vec2(0.91, 0.41)), 0.72, 0.20*s);
    v += wavePart(p, normalize(vec2(0.68, 0.73)), 1.18, 0.12*s);
    v += wavePart(p, normalize(vec2(0.99, -0.15)), 1.92, 0.065*w);
    v += wavePart(p, normalize(vec2(0.43, 0.90)), 2.90, 0.032*w);
    v += wavePart(p, normalize(vec2(-0.26, 0.97)), 4.65, 0.014*w);
    return v;
}

vec3 seaNormal(vec3 p, float distance) {
    vec3 v = waveData(p.xz);
    float detail = 1.0-smoothstep(18.0, 60.0, distance);
    v += detail*wavePart(p.xz, normalize(vec2(0.82, -0.57)), 7.8, 0.006*uWind);
    return normalize(vec3(-v.y, 1.0, -v.z));
}

float glassFresnel(float ior, float cosine) {
    float f0 = pow((ior-1.0)/(ior+1.0), 2.0);
    return f0 + (1.0-f0)*pow(1.0-clamp(cosine, 0.0, 1.0), 5.0);
}

// Bracket the moving height field inside its known vertical envelope, then bisect.
// This keeps reflected silhouettes attached to the same surface as the shading.
float seaDistance(vec3 ro, vec3 rd, float farT) {
    if (rd.y >= -0.003 || ro.y < -1.35) return 1e20;
    float start = max(0.002, (1.35-ro.y)/rd.y);
    float end = min(farT, min(170.0, (-1.35-ro.y)/rd.y));
    if (end <= start) return 1e20;
    float prevT = start;
    if (ro.y + rd.y*start - waveData((ro+rd*start).xz).x <= 0.0) return start;
    for (int i=1; i<=48; ++i) {
        float t = mix(start, end, float(i)/48.0);
        float f = ro.y + rd.y*t - waveData((ro+rd*t).xz).x;
        if (f <= 0.0) {
            float lo = prevT, hi = t;
            for (int j=0; j<7; ++j) {
                float mid = 0.5*(lo+hi);
                float mf = ro.y + rd.y*mid - waveData((ro+rd*mid).xz).x;
                if (mf > 0.0) lo = mid; else hi = mid;
            }
            return 0.5*(lo+hi);
        }
        prevT = t;
    }
    return 1e20;
}

Hit closest(vec3 ro, vec3 rd) {
    Hit h; h.t=1e20; h.kind=-1; h.id=-1;
    for (int i=0; i<16; ++i) {
        if (i >= uSphereCount) break;
        vec3 oc = ro-uSphere[i].xyz;
        float b = dot(oc, rd);
        float disc = b*b-dot(oc,oc)+uSphere[i].w*uSphere[i].w;
        if (disc < 0.0) continue;
        float s = sqrt(disc), t = -b-s;
        if (t < 0.002) t = -b+s;
        if (t > 0.002 && t < h.t) {
            h.t=t; h.p=ro+rd*t; h.n=normalize(h.p-uSphere[i].xyz);
            h.kind=int(uMatB[i].x+0.5); h.id=i;
        }
    }
    float water = seaDistance(ro, rd, h.t);
    if (water < h.t) {
        h.t=water; h.p=ro+rd*water; h.n=seaNormal(h.p, water);
        h.kind=5; h.id=-1;
    }
    return h;
}

// A sphere dims the direct sun on the waves. Glass passes tinted light.
vec3 sunVisibility(vec3 p) {
    vec3 d = sunDirection();
    vec3 visibility = vec3(1.0);
    for (int i=0; i<16; ++i) {
        if (i >= uSphereCount) break;
        vec3 oc = p-uSphere[i].xyz;
        float b = dot(oc,d);
        float disc = b*b-dot(oc,oc)+uSphere[i].w*uSphere[i].w;
        if (disc <= 0.0 || -b+sqrt(disc) <= 0.0) continue;
        if (int(uMatB[i].x+0.5) == 3) {
            float path = 2.0*sqrt(disc);
            visibility *= pow(max(uMatA[i].rgb, vec3(0.01)), vec3(path))*0.9;
        } else visibility *= 0.13;
    }
    return visibility;
}

vec3 waterBody(Hit h, vec3 ray) {
    vec3 n = h.n;
    vec3 under = refract(ray, n, 1.0/1.333);
    float path = clamp((-5.0-h.p.y)/min(under.y,-0.08), 0.0, 45.0);
    vec3 extinction = exp(-vec3(0.42,0.17,0.09)*path);
    vec3 floorColor = vec3(0.33,0.35,0.27)*extinction;
    vec3 deep = uWaterColor*(1.0-extinction);
    float sun = max(dot(n, sunDirection()),0.0);
    vec3 lit = sunVisibility(h.p+n*0.015);
    vec3 body = (deep+floorColor)*(0.50+0.78*sun*lit);
    vec3 w = waveData(h.p.xz);
    float steep = length(w.yz);
    float foam = smoothstep(0.35, 0.7, w.x+steep*0.24)*smoothstep(0.7, 1.3, uWind);
    body = mix(body, vec3(0.76,0.88,0.89)*(0.55+0.45*sun), foam*0.43);
    return body;
}

vec3 pathColor(vec3 ro, vec3 rd, vec2 seed) {
    Work stack[16]; int sp=1;
    stack[0]=Work(ro,rd,vec3(1.0),0,-1);
    vec3 result=vec3(0.0);
    for (int path=0; path<32; ++path) {
        if (sp == 0) break;
        Work w=stack[--sp];
        for (int bounce=0; bounce<6; ++bounce) {
            if (w.depth >= uBounces || max(w.weight.r,max(w.weight.g,w.weight.b)) < 0.004) break;
            Hit h=closest(w.o,w.d);
            if (w.medium >= 0 && h.kind >= 0)
                w.weight *= pow(max(uMatA[w.medium].rgb,vec3(0.001)),vec3(h.t));
            if (h.kind < 0) { result += w.weight*sky(w.d); break; }
            vec3 n = dot(w.d,h.n)<0.0 ? h.n : -h.n;
            if (h.kind == 5) {
                float fres = glassFresnel(1.333, dot(-w.d,n));
                result += w.weight*(1.0-fres)*waterBody(h,w.d);
                w.o=h.p+n*0.008; w.d=reflect(w.d,n);
                w.weight *= fres; w.depth++;
                continue;
            }
            if (h.kind == 2) {
                vec3 refl=reflect(w.d,n);
                float rough=clamp(uMatA[h.id].w,0.0,0.8);
                vec3 jitter=vec3(hash21(seed+vec2(float(path),float(bounce))),
                                 hash21(seed+vec2(float(bounce)+23.0,float(path))),
                                 hash21(seed+vec2(float(path)+47.0,float(bounce))));
                w.o=h.p+n*0.004;
                w.d=normalize(refl+(jitter*2.0-1.0)*rough*0.17);
                w.weight *= uMatA[h.id].rgb; w.depth++;
                continue;
            }
            float ior=max(uMatB[h.id].y,1.01);
            bool entering=dot(w.d,h.n)<0.0;
            float eta=entering ? 1.0/ior : ior;
            float cosine=clamp(dot(-w.d,n),0.0,1.0);
            float fres=glassFresnel(ior,cosine);
            vec3 refr=refract(w.d,n,eta);
            vec3 refl=reflect(w.d,n);
            if (length(refr)<0.1) {
                w.o=h.p+n*0.004; w.d=refl; w.depth++;
                continue;
            }
            if (sp<16 && fres>0.01)
                stack[sp++]=Work(h.p+n*0.004,refl,w.weight*fres,w.depth+1,w.medium);
            w.o=h.p-n*0.004; w.d=normalize(refr);
            w.weight *= 1.0-fres;
            w.medium=entering ? h.id : -1;
            w.depth++;
        }
    }
    return result;
}

void main() {
    vec3 sum=vec3(0.0);
    for (int s=0; s<4; ++s) {
        if (s>=uAaSamples) break;
        vec2 jitter=vec2(0.0);
        if (uAaSamples>1)
            jitter=(s==0 ? vec2(-0.25,-0.25) : s==1 ? vec2(0.25,0.25) :
                    s==2 ? vec2(-0.25,0.25) : vec2(0.25,-0.25))/uResolution;
        vec2 p=(vUv+jitter)*2.0-1.0;
        p.x*=uResolution.x/uResolution.y;
        vec3 rd=normalize(uForward+p.x*uRight*0.8+p.y*uUp*0.8);
        sum+=pathColor(uEye,rd,gl_FragCoord.xy+float(s)*19.0);
    }
    vec3 linear=sum/float(max(uAaSamples,1));
    vec3 mapped=(linear*(2.51*linear+0.03))/(linear*(2.43*linear+0.59)+0.14);
    oColor=vec4(pow(clamp(mapped,0.0,1.0),vec3(1.0/2.2)),1.0);
}
