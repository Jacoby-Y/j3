Threads in J3 (Linux) — move heavy work off the frame thread
============================================================

The one idea: HAND WORK OVER, DO NOT SHARE IT. Give a worker a struct only it writes, wait
for its result, then read it. There is no memory model beyond that and the compiler does not
check races, so the rules below are the whole safety net.

Start from a program: `j3c doc recipes threads` (a pool builds chunks, a frame-style loop feeds
it and collects results without blocking). Every primitive is in `speculative/13_threads.j3`.
Names and signatures: `j3c doc j3.thread`.


Pick a tool
-----------
    run one thing in the background   spawn(&t, name, f, arg), later join(&t)
    many jobs on a few workers        Pool.create(&arena, workers, queueCap), then submit / trySubmit
    the same work over 0..n           pool.parallelFor(n, f, ctx)   (f writes out[i]; it waits)
    results back to a frame loop      a second Queue<T>: workers push, main tryPops
    a counter or cancel flag          @atomicAdd / @atomicLoad / @atomicStore / @atomicCas on an int
    anything bigger that is shared    a Mutex around it (better: hand it over instead)
    one value per thread              #[singleton] #[thread] let X: T;   (memory.scratch is one)


The frame thread must never block
---------------------------------
Use these in a 60 Hz loop:   pool.trySubmit (false: queue full, keep the job, retry next frame),
                             queue.tryPop (a result or nothing), queue.tryPush, queue.count,
                             pool.pendingJobs, @atomicLoad.
These can wait on other threads, so keep them to loaders, tools, and shutdown:
                             submit, wait, parallelFor, push, pop, join, lock.
Size every Queue for the worst case. A full results queue blocks the worker in push until main
drains it; a full job queue is what trySubmit reports.


The rules
---------
1. Share by handing off. A worker owns the memory it was given until it reports back. The owner
   touches that memory again only after the signal (wait, pop, join).
2. Ordering: everything a thread wrote before unlock, signal, push, or an atomic store is visible
   to another thread after the matching lock, wait, pop, or atomic load. Nothing else is ordered.
   Two cases callers rely on: everything written before spawn or submit is visible to that job
   when it starts, and everything a job wrote is visible once join, pool.wait, or parallelFor
   returns.
   Never spin on a plain flag: the compiler may hoist the load. Use @atomicLoad or a Queue.
3. A job is a plain fn(ptr) plus ONE ptr to a struct the spawner owns and keeps alive until the
   job has finished. There are no capturing closures. Put what the job needs in that struct: its
   input, the slice it alone writes, an index, a &Queue to send the result to.
4. One Arena per thread. Two threads allocating from one Arena race. A worker allocates from its
   own arena or from memory.scratch, which is per thread and which a pool worker rewinds after
   every job, so scratch memory never outlives the job: copy results into the struct main gave
   you before the job returns.
5. Window, GL, SDL, audio, and file dialogs are main-thread only, and so is all state in j3.game,
   j3.gfx (including j3.gfx.paint), j3.scene3d, j3.ui, and j3.app. Workers compute; main uploads and draws.
6. Races are yours to avoid. Default for data the owner keeps editing: main COPIES the region into a
   worker-owned buffer when it dispatches, and a version stamp only decides if the result is stale
   (see "Shared data that keeps changing" below). Reading it live is allowed only when the worker
   survives torn data.
7. Scheduling is nondeterministic: jobs start in order and finish in any order. Anything that
   feeds saves, replays, or netcode reduces results in a fixed order. parallelFor writing out[i]
   and then summing in index order is deterministic.
8. A Thread, Mutex, Cond, and Queue must not move or be copied once in use. Keep one in a let (or
   an arena slot, as Pool does) and pass &.
9. A panic or crash on any thread ends the whole process. A panic prints [thread name] first, and
   every thread has its own crash stack, so a stack overflow in a worker still prints a trace
   (without the thread name). Scripts (.j3s) never run on a worker.


Which modules are safe on a worker
----------------------------------
No module state, and they call nothing main-thread only (checked by reading each module's globals
and imports). They keep state in the values you pass in, so two threads can use two different
values at once, never one value at once:
    j3.math  j3.rand  j3.text (its cstr scratch is per thread)  j3.io (lines from two threads can
    interleave)  j3.option  j3.result  j3.gfx.voxel  j3.gfx.noise  j3.gfx.mesh  j3.gfx.bvh
    j3.gfx.canvas (on an arena you own)  j3.game.path  .space  .collide  .tile  .anim  .ecs
    .physics  j3.data.save  j3.data.res  j3.data.json
Made for workers, since every call blocks: j3.net.tcp  .dns  .tls (loads libssl once under a lock;
its lastError is per thread)  .http  j3.proc (`recipes http`).
Inside j3.gfx only these touch no state: emitFace, emitFaceAo (mesh building), boxInClip, rayAabb
(geometry), plainSurface, fxWidth. Everything else in j3.gfx (draw, upload, lights, shadows,
effects) needs the GL context.
Main thread only: j3.game, j3.gfx (except the list above), j3.scene3d, j3.ui, j3.app,
j3.data.assets (it loads through j3.game).


Common mistakes
---------------
- Passing &local to a job when the function returns before the job runs. The struct must outlive
  the job: put it in an arena, an array, or a let that outlives the wait.
- Reading a buffer while a worker still writes it ("it is mostly done"). Wait for the signal.
- Calling wait, submit, or parallelFor from inside a job. A worker waiting on its own pool, or
  filling its queue, can deadlock it.
- A results Queue smaller than the jobs in flight, so workers block in push.
- Destroying the arena that holds a Pool before pool.shutdown(): the workers are still running.
- Two threads calling parallelFor on one Pool at once. One call at a time.
- Assuming jobs finish in the order they were submitted.
- Calling gfx or game from a job "just to upload". Hand the data back and upload on main.
- Using one Arena (or one Canvas, one voxel grid) from two threads.


Shared data that keeps changing
-------------------------------
Workers read a big grid while the frame thread edits it. Snapshot, stamp, compare, re-dispatch:

    // main, when it dispatches chunk c
    job.version = version[c];                 // what the grid looked like for this job
    copy(job.cells, grid, region(c));         // worker-owned copy: the edit cannot tear it
    pool.trySubmit(build, job as ptr);        // false: keep it, retry next frame
    // main, on every edit that touches chunk c
    version[c] += 1;
    dirty[c] = true;                          // dispatch c again when a job slot is free
    // main, when a result arrives
    if r.version != version[c] { dirty[c] = true; }   // stale: drop it, do not upload
    else { upload(r); }

Only main writes version[] and dirty[], so they need no atomics. The recipe does exactly this
with current[chunk]. To skip the copy and read the live grid, the worker must clamp every index,
never panic on an odd value, and read only ints and bytes; a bounds-check abort in a debug build
kills the process, and discarding the result afterwards does not help.


Program globals
---------------
- A #[singleton] written once at startup, before the first spawn or submit, and never again is
  safe to read from any worker (spawn and submit order the write before the job).
- Anything written after workers exist needs @atomic* (an int) or a Mutex. @atomicAdd(&COUNT, 1)
  works on a global.
- Do not initialise a global lazily ("create on first use") from workers without a lock: two
  threads both see it empty. Create it before the pool starts. memory.scratch gets away with it
  only because each thread has its own copy.
- A #[thread] global is one copy per thread, and a new thread's copy starts at the initializer,
  not at main's current value. Set it inside the job if the job needs it.


Cancel and priority
-------------------
There is no job priority and no cancel. Keep few jobs in flight and submit the urgent ones first.
To cancel, put an int flag in the job's struct, set it with @atomicStore(&flag, 1), and have the
job check @atomicLoad(&flag) between steps and return early. For results that went stale, stamp
each job with a version and have main drop results whose version is no longer current; the recipe
does this.


Language side
-------------
#[thread] on a module let gives every thread its own copy (a worker's copy starts at the
initializer). @atomicLoad(p), @atomicStore(p, v), @atomicAdd(p, v) (returns the old value), and
@atomicCas(p, old, new) (returns whether it swapped) work on an int through a pointer and are
sequentially consistent. Atomics are not supported in .j3s scripts.
