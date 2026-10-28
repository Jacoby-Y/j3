`@script` makes a script a type; each `Counter { ... }` is an instance. `j3c run` runs
scripts on a VM, so editing counter.j3s re-runs without compiling main.j3; `j3c build`
compiles them in. Prints `a: 1 of 5` and `b: 4 of 5`.

- props are set where the instance is made and are read-only in the script; one with no
  default is required.
- shared is one value for every instance. A top-level `let` is each instance's own.
- Outside code only calls `pub fn`s; it can't read an instance's fields.
- To run a script on a scene object instead, attach it in the scene (`recipes scene3d`).
- `j3c doc scripts/counter.j3s` summarizes a script. Rules: `j3c doc lang`, Scripts.
