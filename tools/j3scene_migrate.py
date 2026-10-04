#!/usr/bin/env python3
"""Convert v1 `.j3scene` files (node.<i>.<field>= lines) to the v2 tree format.

    j3scene_migrate.py [--check] [--fix-degrees] [--group MIN] [--selftest] PATH...

With --fix-degrees, objects whose stored rotation is over two turns (radians that hold degrees)
keep the number as degrees; on a file that is already v2 only that fix is applied.

The output must match what `scene3d.write` (j3c/stdlib/j3/scene3d.j3) writes byte for
byte: `j3c -e scene fmt` on a v1 copy and this script give the same file. Format spec:
j3c/editor/README.md, "Scene file format". v2 files are left alone.
"""
import math
import os
import re
import struct
import sys

GROUP, MESH, CAMERA, INSTANCE, LIGHT = range(5)
SHAPES = ("cube", "sphere", "cylinder", "cone", "capsule", "torus", "quad", "plane", "sky")
ALPHA = ("opaque", "cutout", "blend")
DEG = 57.29577951308232
TWO_TURNS = 4 * math.pi

# (v2 key, v1 keys, defaults) for vectors; (v2 key, v1 key, default, is_int) for scalars.
ENV_VECS = (
    ("sun", ("sunX", "sunY", "sunZ"), (0.45, 0.8, 0.25)),
    ("sunColor", ("sunR", "sunG", "sunB"), (1.0, 0.95, 0.85)),
    ("sky", ("skyR", "skyG", "skyB"), (0.12, 0.17, 0.24)),
)
ENV_SCALARS = (
    ("fogStart", "fogStart", 80.0, False),
    ("fogEnd", "fogEnd", 400.0, False),
    ("shadows", "shadows", 1, True),
    ("shadowSize", "shadowSize", 30.0, False),
    ("urp", "urp", 0, True),
    ("urpBounces", "urpBounces", 2, True),
    ("urpSamples", "urpSamples", 1, True),
    ("urpShadows", "urpShadows", 0, True),
    ("urpShadowNear", "urpShadowNear", 40.0, False),
)


def f32(x):
    try:
        return struct.unpack("f", struct.pack("f", x))[0]
    except OverflowError:
        return math.copysign(3.4028234663852886e38, x)


def fmt(n, neg, places):
    """`n` (f64, >= 0) to `places` decimals, trailing zeros trimmed, like Text.pushFloat."""
    k = 10 ** places
    s = int(min(n, 9e12) * k + 0.5)
    whole, frac = divmod(s, k)
    out = str(whole)
    if frac:
        out += "." + str(frac).zfill(places).rstrip("0")
    if s and neg:
        out = "-" + out
    return out


def num(x):
    x = f32(x)
    return fmt(abs(x), x < 0, 6)


def deg(rad):
    x = f32(rad)
    return fmt(abs(x) * DEG, x < 0, 3)


def deg_raw(x):
    x = f32(x)
    return fmt(abs(x), x < 0, 3)


def typed_float(x):
    s = num(x)
    return s if "." in s else s + ".0"


def quote(s):
    return '"' + s.replace("\\", "\\\\").replace('"', '\\"') + '"'


def name_text(s):
    if s == "" or any(c in s for c in ' "=\t') or s.startswith("#") or s.startswith("//"):
        return quote(s)
    return s


def value_text(s, path=False):
    return quote(s) if (" " in s or '"' in s or "\t" in s or (path and "=" in s)) else s


def vec(v):
    return ",".join(num(c) for c in v)


def to_float(s):
    try:
        return f32(float(s))
    except ValueError:
        return 0.0


def to_int(s, fallback=0):
    try:
        return int(s)
    except ValueError:
        try:
            return int(float(s))
        except ValueError:
            return fallback


class Node:
    def __init__(self, slot):
        self.id = slot + 1
        self.parent = 0
        self.kind = GROUP
        self.name = "Object"
        self.asset = ""
        self.texture = ""
        self.script = ""
        self.double_sided = False
        self.pos = [0.0, 0.0, 0.0]
        self.rot = [0.0, 0.0, 0.0]
        self.scale = [1.0, 1.0, 1.0]
        self.fov, self.near, self.far = 60.0, 0.15, 200.0
        self.scene = ""
        self.color = [1.0, 0.9, 0.75]
        self.tint = [1.0, 1.0, 1.0]
        self.range = 8.0
        self.alpha = 0
        self.opacity = 1.0
        self.material = ""
        self.effects = ""
        self.tags = ""
        self.note = ""
        self.degrees = False  # rotation was stored in degrees by mistake (--fix-degrees)
        self.params = []  # (field, suffix, value)
        self.overs = []  # (rest, suffix, value)


AXES = {"px": ("pos", 0), "py": ("pos", 1), "pz": ("pos", 2), "rx": ("rot", 0), "ry": ("rot", 1),
        "rz": ("rot", 2), "sx": ("scale", 0), "sy": ("scale", 1), "sz": ("scale", 2),
        "cr": ("color", 0), "cg": ("color", 1), "cb": ("color", 2), "tr": ("tint", 0),
        "tg": ("tint", 1), "tb": ("tint", 2)}
FLOATS = ("fov", "near", "far", "range", "opacity")
STRINGS = ("name", "asset", "texture", "script", "scene", "material", "effects", "tags")


def set_field(n, field, value):
    if field == "id":
        n.id = to_int(value, n.id)
    elif field == "parent":
        n.parent = to_int(value)
    elif field == "kind":
        n.kind = to_int(value)
    elif field == "doubleSided":
        n.double_sided = to_int(value) != 0
    elif field == "alpha":
        n.alpha = to_int(value)
    elif field in AXES:
        attr, i = AXES[field]
        getattr(n, attr)[i] = to_float(value)
    elif field in FLOATS:
        setattr(n, field, to_float(value))
    elif field in STRINGS:
        setattr(n, field, value)


NODE_LINE = re.compile(r"^node\.(\d+)\.([^ =]*)[^=]*=(.*)$")
DATA_LINE = re.compile(r"^([^ =]+)(?: ([a-z]))?=(.*)$")


def parse_v1(text):
    """(nodes, next_id, env, passthrough, dropped) from v1 text."""
    nodes, env, rest, dropped = [], {}, [], 0
    next_id = None
    params, overs = {}, {}
    for line in text.split("\n"):
        line = line.rstrip("\r")
        if not line:
            continue
        m = NODE_LINE.match(line)
        if m:
            slot = int(m.group(1))
            while len(nodes) <= slot:
                nodes.append(Node(len(nodes)))
            set_field(nodes[slot], m.group(2), m.group(3))
            continue
        m = DATA_LINE.match(line)
        if not m:
            continue
        key, suffix, value = m.group(1), m.group(2) or "", m.group(3)
        if key in ("scene.version", "scene.count"):
            continue
        if key == "scene.nextId":
            next_id = to_int(value)
        elif key.startswith("scene.env."):
            env[key[10:]] = value
        elif key.startswith("param."):
            pid, _, field = key[6:].partition(".")
            if pid.isdigit() and field:
                params.setdefault(int(pid), []).append((field, suffix, value))
        elif key.startswith("over."):
            iid, _, tail = key[5:].partition(".")
            if iid.isdigit() and tail:
                overs.setdefault(int(iid), []).append((tail, suffix, value))
        elif key.startswith("prop."):
            rest.append(line)  # a scene's prop declarations; every other unknown line is dropped, as scene3d does
    by_id = {n.id: n for n in nodes}
    for pid, items in params.items():
        if pid in by_id:
            by_id[pid].params = items
        else:
            dropped += len(items)
    for iid, items in overs.items():
        if iid in by_id and by_id[iid].kind == INSTANCE:
            by_id[iid].overs = items
        else:
            dropped += len(items)
    if next_id is None:
        next_id = len(nodes) + 1
    for n in nodes:
        next_id = max(next_id, n.id + 1)
    return nodes, next_id, env, rest, dropped


def env_line(env):
    out = []
    for key, keys, defaults in ENV_VECS:
        vals = [to_float(env.get(k, "")) if k in env else f32(d) for k, d in zip(keys, defaults)]
        text = vec(vals)
        if text != vec(defaults):
            out.append(f"{key}={text}")
    for key, k, default, is_int in ENV_SCALARS:
        if k not in env:
            continue
        if is_int:
            value = to_int(env[k])
            if key in ("shadows", "urp", "urpShadows"):
                value = 1 if value != 0 else 0
            text, base = str(value), str(default)
        else:
            text, base = num(to_float(env[k])), num(default)
        if text != base:
            out.append(f"{key}={text}")
    return "env " + " ".join(out) if out else ""


def short(value, folder, ext):
    """`assets/materials/grain.j3mat` -> `grain`; anything else stays as it is."""
    head, tail = folder + "/", "." + ext
    if value.startswith(head) and value.endswith(tail):
        mid = value[len(head):-len(tail)]
        if mid and "/" not in mid and "." not in mid:
            return mid
    return value


def typed(suffix, value):
    if suffix == "f":
        return typed_float(to_float(value))
    if suffix == "s":
        return quote(value)
    if suffix == "r":
        return "#" + str(to_int(value))
    return str(to_int(value))


def vec_or_uniform(v):
    parts = [num(c) for c in v]
    return parts[0] if parts[0] == parts[1] == parts[2] else ",".join(parts)


def node_tokens(n):
    """(kind words, key=value tokens) for `n`, defaults and inapplicable fields left out."""
    words, toks = [], []
    k = n.kind if n.kind in (MESH, CAMERA, INSTANCE, LIGHT) else GROUP
    if k == MESH:
        if n.asset.startswith("builtin:") and n.asset[8:] in SHAPES:
            words.append(n.asset[8:])
        else:
            words.append("mesh")
            if n.asset:
                words.append(value_text(n.asset, True))
    elif k == INSTANCE:
        words.append("place")
        if n.scene:
            words.append(value_text(n.scene, True))
    else:
        words.append({GROUP: "group", CAMERA: "camera", LIGHT: "light"}[k])
    deg_rot = [deg_raw(r) if n.degrees else deg(r) for r in n.rot]
    if any(d != "0" for d in deg_rot):
        toks.append("rot=" + ",".join(deg_rot))
    pos = vec(n.pos)
    if pos != "0,0,0":
        toks.insert(0, "pos=" + pos)
    if vec(n.scale) != "1,1,1":
        toks.append("scale=" + vec_or_uniform(n.scale))
    if k == MESH:
        if n.material:
            toks.append("material=" + value_text(short(n.material, "assets/materials", "j3mat")))
        if n.texture:
            toks.append("texture=" + value_text(n.texture))
    if k in (MESH, INSTANCE) and vec(n.tint) != "1,1,1":
        toks.append("tint=" + vec_or_uniform(n.tint))
    if k == LIGHT:
        if vec(n.color) != vec([1.0, 0.9, 0.75]):
            toks.append("color=" + vec(n.color))
        if num(n.range) != "8":
            toks.append("range=" + num(n.range))
    if k == CAMERA:
        for key, value, default in (("fov", n.fov, 60.0), ("near", n.near, 0.15), ("far", n.far, 200.0)):
            if num(value) != num(default):
                toks.append(f"{key}={num(value)}")
        if n.effects:
            parts = [short(p, "assets/fx", "j3fx") for p in n.effects.split("|")]
            toks.append("fx=" + value_text("|".join(parts)))
    if k == MESH:
        if n.alpha != 0:
            toks.append("alpha=" + ALPHA[n.alpha if 0 <= n.alpha < 3 else 0])
            if num(n.opacity) != "1":
                toks.append("opacity=" + num(n.opacity))
        if n.double_sided:
            toks.append("doubleSided=1")
    if n.script:
        toks.append("script=" + value_text(n.script))
    if n.tags:
        toks.append("tags=" + value_text(n.tags))
    plain = sorted((p for p in n.params if not p[0].startswith("fx.")), key=lambda p: p[0])
    fx = sorted((p for p in n.params if p[0].startswith("fx.")), key=lambda p: p[0])
    toks += [f"param.{f}={typed(s, v)}" for f, s, v in plain]
    toks += [f"{f}={typed(s, v)}" for f, s, v in fx]
    if k == INSTANCE:
        props = sorted((o for o in n.overs if o[0].startswith("prop.")), key=lambda o: o[0])
        rest = sorted((o for o in n.overs if not o[0].startswith("prop.")), key=lambda o: o[0])
        toks += [f"{r}={quote(v)}" for r, s, v in props]
        toks += [f"over.{r}={typed(s, v)}" for r, s, v in rest]
    return words, toks


def emit_node(n, depth, children, out, seen):
    seen.add(id(n))
    pad = "  " * depth
    for line in n.note.split("\n") if n.note else []:
        out.append(pad + ("// " + line if line else "//"))
    words, toks = node_tokens(n)
    out.append(pad + " ".join([name_text(n.name)] + words + toks + [f"#{n.id}"]))
    for c in children.get(n.id, []):
        if id(c) not in seen:
            emit_node(c, depth + 1, children, out, seen)


def write_v2(nodes, next_id, env, rest):
    by_id = {n.id: n for n in nodes}
    children, roots = {}, []
    for n in nodes:
        if n.parent in by_id and n.parent != n.id:
            children.setdefault(n.parent, []).append(n)
        else:
            roots.append(n)
    out = [f"j3scene 2 next={next_id}"]
    line = env_line(env)
    if line:
        out.append(line)
    seen = set()
    prev_kids = False
    first = True

    def top(n):
        nonlocal prev_kids, first
        kids = bool(children.get(n.id))
        if first or prev_kids or kids:
            out.append("")
        first = False
        emit_node(n, 0, children, out, seen)
        prev_kids = kids

    for n in roots:
        top(n)
    for n in nodes:  # a parent cycle leaves nodes unreached; keep them at the top
        if id(n) not in seen:
            top(n)
    if rest:
        out.append("")
        out += sorted(rest, key=lambda s: s.encode())
    return "\n".join(out) + "\n"


def group_prefixes(nodes, next_id, minimum):
    """Nest top-level objects sharing a name prefix (digits dropped) under a new group."""
    by_prefix = {}
    for n in nodes:
        if n.parent == 0:
            prefix = n.name.rstrip("0123456789")
            if prefix:
                by_prefix.setdefault(prefix, []).append(n)
    out, done = [], set()
    ids = {n.id for n in nodes}
    for n in nodes:
        members = by_prefix.get(n.name.rstrip("0123456789"), []) if n.parent == 0 else []
        if len(members) >= minimum and n.name.rstrip("0123456789") not in done:
            prefix = n.name.rstrip("0123456789")
            done.add(prefix)
            while next_id in ids:
                next_id += 1
            g = Node(next_id - 1)
            g.id, g.name = next_id, prefix + ("Group" if prefix.endswith("s") else "s")
            ids.add(next_id)
            next_id += 1
            out.append(g)
            for m in members:
                m.parent = g.id
        out.append(n)
    return out, next_id


def convert(text, fix_degrees=False, group=0, warn=print):
    nodes, next_id, env, rest, dropped = parse_v1(text)
    if dropped:
        warn(f"dropped {dropped} param/override lines of missing objects")
    flagged = [n for n in nodes if any(abs(f32(r)) > TWO_TURNS for r in n.rot)]
    for n in flagged:
        n.degrees = fix_degrees
    if flagged and not fix_degrees:
        names = ", ".join(n.name for n in flagged[:3])
        warn(f"{len(flagged)} objects store degrees as radians ({names}, ...); --fix-degrees reads them as degrees")
    if group:
        nodes, next_id = group_prefixes(nodes, next_id, group)
    return write_v2(nodes, next_id, env, rest)


def is_v2(text):
    return text.startswith("j3scene 2")


ROT = re.compile(r" rot=(-?[0-9.]+),(-?[0-9.]+),(-?[0-9.]+)")


def fix_v2_degrees(text, warn=print):
    """A v2 file whose `rot=` holds radian values converted as if they were degrees (the editor
    writes any v1 file this way): objects over two turns (720) get the number back, as degrees."""
    fixed = 0

    def swap(m):
        nonlocal fixed
        v = [float(g) for g in m.groups()]
        if all(abs(x) <= 720 for x in v):
            return m.group(0)
        fixed += 1
        return " rot=" + ",".join(fmt(abs(x) / DEG, x < 0, 3) for x in v)

    out = "\n".join(ROT.sub(swap, line) for line in text.split("\n"))
    if fixed:
        warn(f"{fixed} objects had radian rotations over two turns; read them as degrees")
    return out


def files(paths):
    for p in paths:
        if os.path.isdir(p):
            for root, dirs, names in os.walk(p):
                dirs[:] = [d for d in dirs if d not in ("target", ".git")]
                for name in sorted(names):
                    if name.endswith(".j3scene"):
                        yield os.path.join(root, name)
        else:
            yield p


SAMPLE = """scene.version=1
scene.count=3
scene.nextId=9
node.0.id=1
node.0.parent=0
node.0.kind=1
node.0.name s=Big Door
node.0.asset s=builtin:cube
node.0.texture s=
node.0.script s=scripts/door.j3s
node.0.doubleSided=0
node.0.px f=1.5
node.0.py f=0.0
node.0.pz f=0.0
node.0.rx f=0.0
node.0.ry f=1.570796
node.0.rz f=0.0
node.0.sx f=2.0
node.0.sy f=2.0
node.0.sz f=2.0
node.0.fov f=60.0
node.0.near f=0.15
node.0.far f=320.0
node.0.material s=assets/materials/grain.j3mat
node.0.tr f=0.5
node.0.tg f=0.5
node.0.tb f=0.5
node.1.id=2
node.1.parent=1
node.1.kind=4
node.1.name s=Lamp
node.1.py f=3.0
node.1.cr f=1.0
node.1.cg f=0.9
node.1.cb f=0.75
node.1.range f=12.0
node.2.id=8
node.2.parent=0
node.2.kind=3
node.2.name s=Kiosk
node.2.scene s=scenes/kiosk.j3scene
scene.env.sunX f=0.55
scene.env.fogEnd f=300.0
param.1.speed f=3
param.1.label s=hi "there"
over.8.prop.label s=Ammo Shop
over.8.2.px f=4.0
prop.door.type s=bool
"""

EXPECTED = """j3scene 2 next=9
env sun=0.55,0.8,0.25 fogEnd=300

"Big Door" cube pos=1.5,0,0 rot=0,90,0 scale=2 material=grain tint=0.5 script=scripts/door.j3s param.label="hi \\"there\\"" param.speed=3.0 #1
  Lamp light pos=0,3,0 range=12 #2

Kiosk place scenes/kiosk.j3scene prop.label="Ammo Shop" over.2.px=4.0 #8

prop.door.type s=bool
"""


def selftest():
    got = convert(SAMPLE, warn=lambda m: None)
    assert got == EXPECTED, "\n--- got ---\n" + got + "--- expected ---\n" + EXPECTED
    assert convert(SAMPLE, warn=lambda m: None) == got
    print("selftest ok")


def main(argv):
    if "--selftest" in argv:
        return selftest()
    check = "--check" in argv
    fix = "--fix-degrees" in argv
    group = 0
    paths = []
    it = iter(argv)
    for a in it:
        if a == "--group":
            group = int(next(it))
        elif not a.startswith("--"):
            paths.append(a)
    if not paths:
        print(__doc__)
        return 1
    before = after = 0
    for path in files(paths):
        with open(path, encoding="utf-8", newline="") as f:
            text = f.read()
        if is_v2(text):
            if not fix:
                continue
            out = fix_v2_degrees(text, lambda m, p=path: print(f"{p}: {m}"))
            if out == text:
                continue
        else:
            out = convert(text, fix, group, lambda m, p=path: print(f"{p}: {m}"))
        before += len(text)
        after += len(out)
        print(f"{path}: {len(text)} -> {len(out)} bytes")
        if not check:
            with open(path, "w", encoding="utf-8", newline="") as f:
                f.write(out)
    print(f"total: {before} -> {after} bytes")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
