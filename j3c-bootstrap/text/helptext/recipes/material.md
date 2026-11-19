A mesh's `material` field points at a `.j3mat`: `key=value` lines, where missing keys keep
their defaults. The editor's material tab writes them; by hand, a toon-shaded material:

==> assets/materials/toon.j3mat <==
tint.r f=0.9
tint.g f=0.5
tint.b f=0.3
shine f=0.2
fx s=assets/fx/toon.j3fx
fx.bands f=3

==> assets/fx/toon.j3fx <==
kind=material
param.bands=int:3
---
vec3 effect(Surface s, vec3 lit) {
  float l = max(dot(lit, vec3(0.299, 0.587, 0.114)), 0.001);
  return lit * (ceil(l * float(uFx_bands)) / float(uFx_bands)) / l;
}

Use it: `j3c -e scene set Crate material=assets/materials/toon.j3mat`, then
`j3c -e render --focus Crate`. Other keys: glow.r/g/b, reflect, rough, ior (glass),
opacity, texture s=<png>. A `displace` function under `#ifdef J3_VERTEX` moves vertices
(and their shadows). GLSL errors print with the `.j3fx` line, and a broken effect draws
as if it weren't there. Formats and every setting: `j3c doc editor`, Materials and Effects.
