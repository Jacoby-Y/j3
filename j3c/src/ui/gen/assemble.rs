use super::*;

impl<'a> G<'a> {
    pub(super) fn assemble(&mut self, fns: Out) -> R<Out> {
        let c = self.c;
        let name = c.id.clone();
        let src = self.src;
        let fi = self.fi;
        let mut o = Out::with_ctx(fi);
        o.s(&format!("// ---- {}\n", self.files[fi].path));
        o.s(&format!("pub const SCOPE_{name}: int = {};\n", c.scope));
        if let Some((a, b)) = c.mk.style {
            o.set(fi, a, b);
            o.s(&format!("const CSS_{name}: string = {};\n", lit(src[a..b].trim_matches('\n'))));
            // lines above the trimmed style text: `file:line` of rules and warnings are the `.j3ui`'s own
            let top = a + (src[a..b].len() - src[a..b].trim_start_matches('\n').len());
            o.s(&format!("const LINE0_{name}: int = {};\n", src[..top].matches('\n').count()));
        } else {
            o.s(&format!("const CSS_{name}: string = \"\";\nconst LINE0_{name}: int = 0;\n"));
        }
        o.set(fi, 0, 0);
        o.s(&format!("\npub struct {name}Props {{\n"));
        for p in &c.sc.props {
            o.set(fi, p.name_r.0, p.name_r.1);
            o.s(&format!("    {}: ", p.name));
            match &p.hidden {
                Some(t) => o.s(t),
                None => o.copy(fi, src, p.ty.0, p.ty.1),
            }
            o.s(";\n");
        }
        o.set(fi, 0, 0);
        if let Some(pi) = self.slots.pass.get(&c.name) {
            o.s(&pass::fields(pi));
        }
        o.s(&format!("}}\n\npub struct {name}Events {{\n"));
        for (e, r) in &c.sc.events {
            o.set(fi, r.0, r.1);
            o.s(&format!("    {e}: bool;\n"));
            for p in c.sc.ev_params.iter().filter(|p| p.ev == *e) {
                o.set(fi, p.name_r.0, p.name_r.1);
                o.s(&format!("    {e}_{}: ", p.name));
                o.copy(fi, src, p.ty.0, p.ty.1);
                o.s(";\n");
            }
        }
        o.set(fi, 0, 0);
        o.s("}\n\n");
        // `const`: a literal is a module const; anything else (an array) is a singleton filled by `styles`
        // (J3 consts hold scalars only), which runs before the first `update`
        for l in &c.sc.consts {
            let ty = match (l.ty, l.guess) {
                (Some(t), _) => src[t.0..t.1].to_string(),
                (None, Some(g)) => g.to_string(),
                (None, None) => return Err(UiError { lo: l.name_r.0, hi: l.name_r.1, msg: format!("can't tell the type of `const {}`", l.name), help: Some(format!("write it out: `const {}: Type = ...;`", l.name)) }),
            };
            o.set(fi, l.name_r.0, l.name_r.1);
            if l.guess.is_some() {
                o.s(&format!("const {name}_{}: {ty} = ", l.name));
                o.copy(fi, src, l.init.0, l.init.1);
                o.s(";\n\n");
            } else {
                o.s(&format!("#[singleton]\nlet {name}_{}: {ty};\n\n", l.name));
            }
        }
        o.set(fi, 0, 0);
        // what `on:name` sees as `event`
        for (e, _) in c.sc.events.iter().filter(|e| c.sc.ev_params.iter().any(|p| p.ev == e.0)) {
            o.s(&format!("pub struct {name}Event_{e} {{\n"));
            for p in c.sc.ev_params.iter().filter(|p| p.ev == *e) {
                o.set(fi, p.name_r.0, p.name_r.1);
                o.s(&format!("    {}: ", p.name));
                o.copy(fi, src, p.ty.0, p.ty.1);
                o.s(";\n");
            }
            o.set(fi, 0, 0);
            o.s("}\n\n");
        }
        for ed in self.eachs.iter().filter(|e| e.rows) {
            let id = ed.id;
            o.s(&format!("struct {name}_R{id} {{\n    uid: int;\n    comp: int;\n    dom: &Dom;\n{}", self.state_fields(id)));
            o.s(&format!("\n    fn make(mem: &Arena, d: &Dom, uid: int, comp: int): &{name}_R{id} {{\n        let a = mem.alloc::<{name}_R{id}>(1);\n        a[0] = {name}_R{id} {{\n            uid: uid,\n            comp: comp,\n            dom: d,\n{}        }};\n        return &a[0];\n    }}\n", self.state_inits(id)));
            o.s(&format!("\n    fn reset(this: &{name}_R{id}, uid: int, comp: int) {{\n        this.uid = uid;\n        this.comp = comp;\n{}    }}\n", self.state_resets(id, "this")));
            o.s(&format!("\n    fn take(pool: &List<&{name}_R{id}>, mem: &Arena, d: &Dom, uid: int, comp: int): &{name}_R{id} {{\n        if pool.len > 0 {{\n            pool.len -= 1;\n            let r = pool.items[pool.len];\n            r.reset(uid, comp);\n            return r;\n        }}\n        return {name}_R{id}.make(mem, d, uid, comp);\n    }}\n"));
            o.s(&format!("\n    fn destroy(this: &{name}_R{id}) {{\n{}        if this.uid >= 0 {{\n            this.dom.groupFree(this.uid);\n            this.uid = -1;\n        }}\n    }}\n}}\n\n", self.state_destroy(id, "this")));
        }
        o.s(&format!("pub struct {name} {{\n    mem: &Arena;\n    ready: bool;\n    dm: int;\n    p: &{name}Props;\n"));
        if self.root {
            o.s("    css: &Css;\n    dom: Dom;\n");
        }
        for l in &c.sc.lets {
            o.set(fi, l.name_r.0, l.name_r.1);
            o.s(&format!("    {}: ", l.name));
            match (l.ty, l.guess) {
                (Some(t), _) => o.copy(fi, src, t.0, t.1),
                (None, Some(g)) => o.s(g),
                (None, None) => {
                    return Err(UiError { lo: l.name_r.0, hi: l.name_r.1, msg: format!("can't tell the type of `let {}`", l.name), help: Some(format!("write it out: `let {}: Type = ...;`", l.name)) });
                }
            }
            o.s(";\n");
        }
        o.set(fi, 0, 0);
        for t in &self.tags {
            o.s(&format!("    {}: int;\n", field_name("tag_", &self.tags, t)));
        }
        for t in &self.classes {
            o.s(&format!("    {}: int;\n", field_name("cls_", &self.classes, t)));
        }
        o.s(&self.state_fields(0));
        if self.has_slot {
            o.s("    slotSite: int;\n    slotOwn: int;\n    slotI: int;\n");
            for (k, _, _) in &self.slot_marks {
                o.s(&format!("    slotNow{k}: bool;\n    slotWas{k}: bool;\n"));
            }
        }
        let bridge = (self.owns_slot || !self.names.emitting.is_empty()) && !c.sc.events.is_empty();
        if bridge {
            o.s(&format!("    sev: {name}Events;\n"));
        }
        // new
        o.s(&format!("\n    fn new(mem: &Arena): {name} {{\n        let ps = mem.alloc::<{name}Props>(1);\n"));
        if self.root {
            o.s("        let cs = mem.alloc::<Css>(1);\n        cs[0] = Css.new(mem);\n        let css = &cs[0];\n");
            // the built-in popup sheet goes first, so the theme overrides it
            o.s("        uastyle.load(css);\n");
            if let Some(tp) = self.theme_path {
                o.s(&format!("        parse(css, THEME, {}, 0);\n", lit(tp)));
            }
            o.s(&format!("        {name}.styles(css);\n"));
        }
        o.s(&format!("        return {name} {{\n            mem: mem, ready: false, dm: 0, p: &ps[0],\n"));
        if self.root {
            o.s("            css: css, dom: Dom.new(mem, css),\n");
        }
        for l in &c.sc.lets {
            o.set(fi, l.init.0, l.init.1);
            o.s(&format!("            {}: ", l.name));
            o.copy(fi, src, l.init.0, l.init.1);
            o.s(",\n");
        }
        o.set(fi, 0, 0);
        for t in &self.tags {
            o.s(&format!("            {}: 0,\n", field_name("tag_", &self.tags, t)));
        }
        for t in &self.classes {
            o.s(&format!("            {}: 0,\n", field_name("cls_", &self.classes, t)));
        }
        o.s(&self.state_inits(0));
        if self.has_slot {
            o.s("            slotSite: 0, slotOwn: 0, slotI: 0,\n");
            for (k, _, _) in &self.slot_marks {
                o.s(&format!("            slotNow{k}: false, slotWas{k}: false,\n"));
            }
        }
        if bridge {
            o.s(&format!("            sev: {name}Events {{ {} }},\n", ev_init(&c.sc, src)));
        }
        o.s("        };\n    }\n");
        // reset: `new` again without allocating (a pooled `{#each}` row reuses its components)
        o.s(&format!("\n    fn j3Reset(this: &{name}) {{\n        let mem = this.mem;\n        this.ready = false;\n"));
        for l in &c.sc.lets {
            o.set(fi, l.init.0, l.init.1);
            o.s(&format!("        this.{} = ", l.name));
            o.copy(fi, src, l.init.0, l.init.1);
            o.s(";\n");
        }
        o.set(fi, 0, 0);
        o.s(&self.state_resets(0, "this"));
        if self.has_slot {
            o.s("        this.slotSite = 0;\n        this.slotOwn = 0;\n        this.slotI = 0;\n");
            for (k, _, _) in &self.slot_marks {
                o.s(&format!("        this.slotNow{k} = false;\n        this.slotWas{k} = false;\n"));
            }
        }
        if bridge {
            o.s(&format!("        this.sev = {name}Events {{ {} }};\n", ev_init(&c.sc, src)));
        }
        o.s("    }\n");
        // styles
        o.s(&format!("\n    fn styles(css: &Css) {{\n        parseAt(css, CSS_{name}, {}, SCOPE_{name}, LINE0_{name});\n", lit(&self.style_path())));
        for l in c.sc.consts.iter().filter(|l| l.guess.is_none()) {
            o.set(fi, l.init.0, l.init.1);
            o.s(&format!("        {name}_{} = ", l.name));
            o.copy(fi, src, l.init.0, l.init.1);
            o.s(";\n");
        }
        o.set(fi, 0, 0);
        let mut done: Vec<&str> = Vec::new();
        for (_, child, _) in &self.insts {
            if !done.contains(&child.as_str()) {
                done.push(child);
                o.s(&format!("        {child}.styles(css);\n"));
            }
        }
        o.s("    }\n");
        // init
        o.s(&format!("\n    fn init(this: &{name}, d: &Dom) {{\n        this.ready = true;\n        this.dm = d as int;\n"));
        for t in &self.tags {
            o.s(&format!("        this.{} = d.tag({});\n", field_name("tag_", &self.tags, t), lit(t)));
        }
        for t in &self.classes {
            o.s(&format!("        this.{} = d.css.intern({});\n", field_name("cls_", &self.classes, t), lit(t)));
        }
        o.s("    }\n");
        o.append(fns);
        let sf = std::mem::take(&mut self.slot_fns);
        o.append(sf);
        if self.root {
            o.set(fi, 0, 0);
            o.s(&format!("\n    fn render(this: &{name}, size: Vec2, p: {name}Props) {{\n        let d = &this.dom;\n        if hotDue(this.css) {{\n            this.hot();\n        }}\n        d.beginFrame(size);\n        inputs.begin(d);\n        let ev = this.update(d, p);\n        d.layout();\n        d.setPaintHook({name}PaintHook, this as ptr);\n        d.paintAll();\n        inputs.overlay(d);\n        d.endFrame();\n    }}\n"));
            // style hot reload: compare each style source on disk with the embedded text
            o.s(&format!("\n    fn hot(this: &{name}) {{\n        let css = this.css;\n        let ch = false;\n"));
            if let Some(tp) = self.theme_path {
                o.s(&format!("        if hotFile(css, {}, THEME, 0, false) {{\n            ch = true;\n        }}\n", lit(tp)));
            }
            for k in self.all {
                let text = &self.files[k.fi].text;
                let (a, b) = k.mk.style.unwrap_or((text.len(), text.len()));
                let mut h: i64 = 7;
                for &x in text.as_bytes()[..a].iter().chain(text.as_bytes()[b..].iter()) {
                    h = (h * 31 + x as i64) % 1_000_000_007;
                }
                o.s(&format!("        if hotFile(css, {}, CSS_{}, {h}, true) {{\n            ch = true;\n        }}\n", lit(&self.files[k.fi].path), k.id));
            }
            if self.theme_path.is_some() {
                o.s(&format!("        if ch {{\n            css.reset();\n            css.hotWarn = 0;\n            uastyle.load(css);\n            parse(css, THEME, {}, 0);\n            {name}.styles(css);\n            this.dom.restyle();\n            hotWarns(css);\n        }}\n    }}\n", lit(self.theme_path.unwrap())));
            } else {
                o.s(&format!("        if ch {{\n            css.reset();\n            css.hotWarn = 0;\n            uastyle.load(css);\n            {name}.styles(css);\n            this.dom.restyle();\n            hotWarns(css);\n        }}\n    }}\n"));
            }
        }
        o.set(fi, 0, 0);
        let has = |f: &str| c.sc.fns.iter().any(|x| x.name == f);
        o.s(&format!("\n    fn update(this: &{name}, d: &Dom, p: {name}Props): {name}Events {{\n        let mounting = !this.ready;\n        if mounting {{\n            this.init(d);\n        }}\n        *this.p = p;\n"));
        for p in &self.used_u {
            o.s(&format!("        let {p} = p.{p};\n"));
        }
        o.s(&format!("        let ev = {name}Events {{ {} }};\n", ev_init(&c.sc, src)));
        if has("onFrame") {
            o.s("        this.onFrame(d.dt());\n");
        }
        let upd = std::mem::take(&mut self.upd);
        o.append(upd);
        o.set(fi, 0, 0);
        for (k, n, calls) in &self.slot_marks {
            o.s(&format!("        if this.slotWas{k} && !this.slotNow{k} {{\n"));
            for (i, (id, owner)) in calls.iter().enumerate() {
                o.s(&format!("            {}if this.slotSite == {id} {{\n                ((this.slotOwn as ptr) as &{owner}).slot{id}_{n}_leave(d, this.slotI);\n            }}", if i > 0 { " else " } else { "" }));
                if i + 1 == calls.len() {
                    o.s("\n");
                }
            }
            o.s(&format!("        }}\n        this.slotWas{k} = this.slotNow{k};\n        this.slotNow{k} = false;\n"));
        }
        // before the merge below: an `emit` in onMount lands in `sev` too
        if has("onMount") {
            o.s("        if mounting {\n            this.onMount();\n        }\n");
        }
        if bridge {
            for (e, _) in &c.sc.events {
                let copy: String = c.sc.ev_params.iter().filter(|p| p.ev == *e).map(|p| format!("            ev.{e}_{0} = this.sev.{e}_{0};\n", p.name)).collect();
                o.s(&format!("        if this.sev.{e} {{\n            ev.{e} = true;\n{copy}            this.sev.{e} = false;\n        }}\n"));
            }
        }
        o.s("        return ev;\n    }\n");
        // children first, then this one; `ready` makes a second call (a branch left twice) a no-op
        o.s(&format!("\n    fn destroy(this: &{name}) {{\n        if !this.ready {{\n            return;\n        }}\n"));
        o.s(&self.state_destroy(0, "this"));
        if has("onDestroy") {
            o.s("        this.onDestroy();\n");
        }
        o.s("        this.ready = false;\n    }\n");
        o.s(&format!("\n    fn paint(this: &{name}, d: &Dom) {{\n"));
        for p in &self.used_p {
            o.s(&format!("        let {p} = this.p.{p};\n"));
        }
        let pnt = std::mem::take(&mut self.pnt);
        o.append(pnt);
        o.set(fi, 0, 0);
        o.s("    }\n");
        for (n, sites) in &self.names.slot_has {
            let test = if sites.is_empty() { "false".to_string() } else { sites.iter().map(|id| format!("this.slotSite == {id}")).collect::<Vec<_>>().join(" || ") };
            o.s(&format!("\n    fn {}(this: &{name}): bool {{\n        return {test};\n    }}\n", slot_has_fn(n)));
        }
        o.s("}\n\n");
        if self.root {
            // `<canvas>` bodies are generated code: the runtime paints the boxes and calls back here to reach each body (`Dom.paintAll`)
            o.s(&format!("fn {name}PaintHook(p: ptr) {{\n    let r = p as &{name};\n    r.paint(&r.dom);\n}}\n\n"));
        }
        Ok(o)
    }

    /// Struct fields for the state held by container `cont' (0 is the component, else a row).
    pub(super) fn state_fields(&self, cont: usize) -> String {
        let name = &self.c.id;
        let mut o = String::new();
        for (id, child, c) in &self.insts {
            if *c == cont {
                o.push_str(&format!("    c{id}: {child};\n"));
            }
        }
        for (id, c) in &self.ifs {
            if *c == cont {
                o.push_str(&format!("    if{id}: int;\n"));
            }
        }
        for a in self.acts.iter().filter(|a| a.container == cont) {
            o.push_str(&format!("    ua{0}: int;\n    us{0}: int;\n", a.k));
            match a.ty.as_deref() {
                Some("string") => o.push_str(&format!("    up{0}: []u8;\n    ul{0}: int;\n", a.k)),
                Some(t) => o.push_str(&format!("    up{}: {t};\n", a.k)),
                None => {}
            }
        }
        for b in self.binds.iter().filter(|b| b.container == cont) {
            o.push_str(&format!("    bh{}: int;\n", b.k));
        }
        for e in self.eachs.iter().filter(|e| e.container == cont) {
            let id = e.id;
            o.push_str(&format!("    cnt{id}: int;\n"));
            if e.rows {
                o.push_str(&format!("    e{id}: List<&{name}_R{id}>;\n    fp{id}: List<&{name}_R{id}>;\n"));
            }
            if e.keyed {
                o.push_str(&format!("    eb{id}: List<&{name}_R{id}>;\n    kd{id}: Keyed;\n"));
            }
            if e.virt {
                o.push_str(&format!("    vl{id}: VList;\n"));
            }
        }
        o
    }

    /// Initial values for `state_fields`, as struct-literal lines (`mem` is in scope).
    pub(super) fn state_inits(&self, cont: usize) -> String {
        let name = &self.c.id;
        let mut o = String::new();
        for (id, child, c) in &self.insts {
            if *c == cont {
                o.push_str(&format!("            c{id}: {child}.new(mem),\n"));
            }
        }
        for (id, c) in &self.ifs {
            if *c == cont {
                o.push_str(&format!("            if{id}: -1,\n"));
            }
        }
        for a in self.acts.iter().filter(|a| a.container == cont) {
            o.push_str(&format!("            ua{0}: -1,\n            us{0}: 0,\n", a.k));
            match a.ty.as_deref() {
                Some("string") => o.push_str(&format!("            up{0}: mem.alloc::<u8>(0),\n            ul{0}: 0,\n", a.k)),
                Some(t) => o.push_str(&format!("            up{}: {},\n", a.k, ev_default(t))),
                None => {}
            }
        }
        for b in self.binds.iter().filter(|b| b.container == cont) {
            o.push_str(&format!("            bh{}: -1,\n", b.k));
        }
        for e in self.eachs.iter().filter(|e| e.container == cont) {
            let id = e.id;
            o.push_str(&format!("            cnt{id}: 0,\n"));
            if e.rows {
                o.push_str(&format!("            e{id}: List<&{name}_R{id}>.new(mem),\n            fp{id}: List<&{name}_R{id}>.new(mem),\n"));
            }
            if e.keyed {
                o.push_str(&format!("            eb{id}: List<&{name}_R{id}>.new(mem),\n            kd{id}: Keyed.new(mem),\n"));
            }
            if e.virt {
                o.push_str(&format!("            vl{id}: VList.new(mem),\n"));
            }
        }
        o
    }

    /// Fresh state for container `cont` without allocating: components reset, `{#if}`s unset, lists emptied
    /// (their dead rows go to the pool).
    pub(super) fn state_resets(&self, cont: usize, who: &str) -> String {
        let mut o = String::new();
        for (id, _, c) in &self.insts {
            if *c == cont {
                o.push_str(&format!("        {who}.c{id}.j3Reset();\n"));
            }
        }
        for (id, c) in &self.ifs {
            if *c == cont {
                o.push_str(&format!("        {who}.if{id} = -1;\n"));
            }
        }
        for a in self.acts.iter().filter(|a| a.container == cont) {
            o.push_str(&format!("        {who}.ua{0} = -1;\n        {who}.us{0} = 0;\n", a.k));
            if a.ty.as_deref() == Some("string") {
                o.push_str(&format!("        {who}.ul{} = 0;\n", a.k));
            }
        }
        for b in self.binds.iter().filter(|b| b.container == cont) {
            o.push_str(&format!("        {who}.bh{} = -1;\n", b.k));
        }
        for e in self.eachs.iter().filter(|e| e.container == cont) {
            let id = e.id;
            o.push_str(&format!("        {who}.cnt{id} = 0;\n"));
            if e.rows {
                o.push_str(&format!("        for q{id} in 0..{who}.e{id}.len {{\n            {who}.fp{id}.push({who}.e{id}.items[q{id}]);\n        }}\n        {who}.e{id}.len = 0;\n"));
            }
            if e.keyed {
                o.push_str(&format!("        {who}.eb{id}.len = 0;\n        {who}.kd{id}.reset();\n"));
            }
            if e.virt {
                o.push_str(&format!("        {who}.vl{id}.reset();\n"));
            }
        }
        o
    }

    /// `destroy()` calls for what container `cont` holds: components, then every row of its lists.
    pub(super) fn state_destroy(&self, cont: usize, who: &str) -> String {
        let mut o = String::new();
        // a row reaches its component through `comp`
        let comp = if cont == 0 { "this".to_string() } else { format!("(({who}.comp as ptr) as &{})", self.c.id) };
        for a in self.acts.iter().filter(|a| a.container == cont && a.destroy) {
            o.push_str(&format!("        if {who}.ua{0} >= 0 && {who}.us{0} == 1 {{\n            {comp}.{1}_destroy({who}.ua{0});\n        }}\n", a.k, a.name));
        }
        for a in self.acts.iter().filter(|a| a.container == cont) {
            o.push_str(&format!("        {who}.ua{0} = -1;\n        {who}.us{0} = 0;\n", a.k));
        }
        for (id, _, c) in &self.insts {
            if *c == cont {
                o.push_str(&format!("        {who}.c{id}.destroy();\n"));
            }
        }
        for e in self.eachs.iter().filter(|e| e.container == cont && e.rows) {
            let id = e.id;
            o.push_str(&format!("        for q{id} in 0..{who}.e{id}.len {{\n            {who}.e{id}.items[q{id}].destroy();\n        }}\n"));
        }
        o
    }

    pub(super) fn style_path(&self) -> String {
        self.files[self.fi].path.clone()
    }
}
