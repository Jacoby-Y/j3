A 440 Hz sine at 48 kHz stereo through `game.openAudioStream`: each frame tops the queue up to
50 ms ahead and prints the playhead. Headless (no window); in a game put the top-up in the
frame callback.

The pieces:
- `game.openAudioStream(rate, channels)` opens its own SDL device for f32 interleaved samples
  (512-frame device buffer). `raw == 0` means no device: every call on it is then a no-op.
- `s.queue(samples)` appends `frames * channels` floats and never blocks. Queue ~30-50 ms ahead
  each frame; more is latency, less risks gaps on a slow frame.
- Playhead in frames = frames sent - `s.queued()`. The speakers trail it by up to one device
  buffer (~11 ms at 48 kHz).
- Seek or stop: `s.clear()` drops the queue at once, then queue from the new spot. Scrubbing is
  the same: `clear()` and queue a short grain (20-40 ms) at the cursor each drag step.
- `s.pause(true/false)` holds the device; queued samples wait. `s.close()` at the end.
- SDL_mixer (`Sound`, `Music`) is a separate device; both play at the same time.
- `SDL_AUDIODRIVER=dummy` drains in real time, so `queued()` counts down in headless tests.
  The `game_headless` backend (`j3c check`) gives an empty stream.
