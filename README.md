# Rust Impulse Response

Windows CLI and reusable Rust library for capturing a linear software filter from a WASAPI
playback endpoint and exporting it as a full channel-to-channel convolution matrix for
Equalizer APO.

The project measures software processing only. It does not use a microphone and is not a room
or loudspeaker measurement tool.

## How the measurement works

The program opens one continuous shared-mode render stream and a loopback capture stream on the
same endpoint. It renders a known silent lead-in followed by one discrete Dirac impulse per input
channel, with a fixed response window between impulses. Every output channel is recorded for
every input impulse, producing an `outputs × inputs` matrix.

The default impulse is -24 dBFS. Its known amplitude is divided out during extraction, so this
does not attenuate or normalize the exported filter. The lower probe level provides headroom for
filters with positive gain.

WASAPI packet timestamps establish one common relationship between the capture and render clocks.
Integer capture-stream frame indices place all samples after that. Responses are never aligned or
trimmed independently, preserving cross-channel delays and leading zeros at exact sample offsets.

## CLI

List playback endpoints:

```powershell
cargo run --release -- devices
```

Capture the default endpoint with the default two-second response window:

```powershell
cargo run --release -- capture --output captured-filter
```

Useful capture options:

```text
--device-id <ID>       Select a playback endpoint instead of the default
--probe-dbfs <DBFS>    Probe level, no greater than 0 dBFS (default: -24)
--lead-ms <MS>         Silence before the first impulse (default: 500)
--response-ms <MS>     Samples retained for every matrix path (default: 2000)
--tail-ms <MS>         Silence after the final window (default: 250)
```

Keep other applications silent during capture because endpoint loopback includes other audio sent
to the selected endpoint. A WASAPI discontinuity or invalid timestamp aborts the measurement.

## Equalizer APO output

The output directory contains:

- One 32-bit floating-point multichannel WAV per input channel. Its WAV channels contain the
  responses from that input to every output.
- `equalizer-apo.txt`, which creates virtual channels, convolves every input-to-output path, and
  mixes the matrix back into the physical channels.

Place the entire output directory under Equalizer APO's configuration directory and include the
generated file from the active configuration. Keep the WAV files next to the generated config so
its relative paths remain valid. The endpoint sample rate must equal the WAV sample rate.

Disable the original captured filter before enabling its convolution replacement; otherwise both
filters will be applied.

## Limits

- Only linear, time-invariant processing can be represented by an impulse response. Compressors,
  limiters, gates, and time-varying effects cannot be reproduced exactly.
- A delay shared identically by every matrix path cannot be distinguished conceptually from common
  pipeline latency. Channel-relative and path-relative delays remain intact.
- Stereo is the first validated layout. The core and capture schedule support arbitrary endpoint
  channel counts, using Equalizer APO channel indices when a standard Windows speaker mask is not
  available.

## Architecture

The library owns device discovery, capture scheduling, clock mapping, matrix extraction, and
export. The binary contains only CLI concerns. This boundary is intended to support a later WinUI
3 frontend without routing UI state through command-line output.
