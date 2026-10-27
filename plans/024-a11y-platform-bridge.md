# Plan 024: accessibility platform bridge

Stub, written 2026-10-20. Step 2 of plan 023 old stage 12 (now stage 13). Step 1 (the semantic tree) is DONE: read its **Done** note in `023-done.md` first.

**Goal.** A screen reader (Orca first) can find a `.j3ui` window, read each control's role, name and states, follow focus, and
act on controls (click, focus, set value) without the app doing anything but turning the bridge on.

**Approach.** AccessKit is a Rust crate, so the no-crates rule rules it out. Linux first: AT-SPI over D-Bus through `libdbus-1`
FFI (`use lib "dbus-1"`, like the other native libraries), in a new `j3.ui.a11y_atspi` module behind the same
`[ui]`/`App` switch, off by default and absent from a build that does not name it. Register on the accessibility bus
(`org.a11y.Bus`), export `org.a11y.atspi.Accessible`, `Component`, `Action`, `Value`, `Text`/`EditableText` for the focused
box, and emit `focus:`, `state-changed:`, `children-changed:` events. Test with Orca and `accerciser` by hand, and headless with a
scripted `dbus-send` client that reads the tree and compares it with `dumpA11y` output. Windows (UI Automation, COM) and macOS
(NSAccessibility, Objective-C runtime) are separate follow-ups on the same shim.

**What step 1 gives it.** `j3.ui.a11y.role/name/actions` per node, the tree shape (`dump`: roles are
sparse, plain containers are transparent), `perform(d, id, A_FOCUS|A_CLICK|A_SET, num, text)` for every inbound action (click goes
through the keyboard path, so it behaves like Enter), `isFocusable`/`allowedByModal` for focus and modal rules, and
`domhtml.dumpA11y` goldens to prove the tree does not change under it. Nothing is computed per frame; the bridge must keep that:
build nodes lazily on a D-Bus request and send events only on the frames where focus or a state actually changed.

**Gaps to close first (each small).** Stable ids for AT (a node id is reused after it is freed; use a generation or the keyed
`gk`), bounds (`absX/absY/w/h` are there), `aria-selected`/`aria-live`/`aria-describedby`, `<label for>`, a text interface that
reads a text box's caret and selection (never a password's), a way to wake the loop when AT sends an action while idle.

**Size.** XL: about 3 to 4 weeks for AT-SPI alone (libdbus marshalling is most of it); each other platform about the same again.

Status: TODO; not started during the plan 023 popup work. The semantic-tree prerequisite (023 **old** stage 12
step 1) is DONE; remaining prerequisite gaps are tracked in 023 **current** stage 13. See
[`023-handoff.md`](023-handoff.md) for the latest session boundary; the next 023 chunk is 4, not this bridge.
