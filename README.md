# Rust Impulse Response

Windows CLI and reusable Rust library for capturing a linear software filter from a WASAPI
playback endpoint and exporting it as a full channel-to-channel convolution matrix for
Equalizer APO on Windows and CamillaDSP on Linux or SteamOS.

The project measures software processing only. It does not use a microphone and is not a room
or loudspeaker measurement tool.

## How the measurement works

The program opens one continuous shared-mode render stream on the selected source playback
endpoint and a loopback capture stream on the selected destination playback endpoint. They may be
the same endpoint, or separate endpoints in a routed DSP chain. It renders a known silent lead-in
followed by one discrete Dirac impulse per input channel, with a fixed response window between
impulses. Every destination output channel is recorded for every source input impulse, producing
an `outputs × inputs` matrix.

The default impulse is -24 dBFS. Its known amplitude is divided out during extraction, so this
does not attenuate or normalize the exported filter. The lower probe level provides headroom for
filters with positive gain.

WASAPI packet timestamps and the shared Windows QPC timebase establish one common relationship
between the capture and render clocks. Integer capture-stream frame indices place all samples
after that. By default, frames that are exactly silent across the entire matrix are removed from
the beginning. The same frame count is removed from every path, preserving all cross-channel and
path-relative delays. No path is ever aligned or trimmed independently. The two endpoints must
use the same sample rate.

## CLI

### Download a Windows binary

Every push to `main` and every pull request targeting `main` is built and tested on Windows by
GitHub Actions. Open the repository's **Actions** tab, select a successful **Windows build** run,
and download the `rust-impulse-response-windows-x86_64-...` artifact. Extract the ZIP before
running the executable.

You can also build it locally with `cargo build --release`; the executable is written to
`target\\release\\rust-impulse-response.exe`.

### Capture

List playback endpoints:

```powershell
cargo run --release -- devices
# Or, when using the downloaded binary:
.\rust-impulse-response.exe devices
```

Capture the default endpoint with the default two-second response window:

```powershell
cargo run --release -- capture --output captured-filter
# Or, when using the downloaded binary:
.\rust-impulse-response.exe capture --output captured-filter
```

Capture a routed correction application that receives audio on one playback endpoint and sends
processed audio to another:

```powershell
.\rust-impulse-response.exe capture `
  --render-device-id '<Virtual Audio Device ID>' `
  --capture-device-id '<LCD TV ID>' `
  --output captured-filter
```

Useful capture options:

```text
--render-device-id <ID>   Endpoint that receives the impulses (alias: --device-id)
--capture-device-id <ID>  Endpoint captured via loopback (default: render endpoint)
--probe-dbfs <DBFS>    Probe level, no greater than 0 dBFS (default: -24)
--lead-ms <MS>         Silence before the first impulse (default: 500)
--response-ms <MS>     Samples retained for every matrix path (default: 2000)
--tail-ms <MS>         Silence after the final window (default: 250)
--no-trim-leading-silence
                       Preserve common leading digital silence (trimming is on by default)
```

Keep other applications silent during capture because endpoint loopback includes other audio sent
to the selected endpoint. A WASAPI discontinuity or invalid timestamp aborts the measurement.

### Generate a manual Dirac impulse

Generate a one-second mono integer-PCM WAV whose first sample is positive full scale and whose
remaining samples are all exactly zero:

```powershell
.\rust-impulse-response.exe impulse `
  --output dirac-48k-24bit.wav `
  --sample-rate 48000 `
  --bit-depth 24
```

Supported bit depths are 16, 24, and 32-bit integer PCM. The generator applies no dithering,
noise, gain, filtering, fades, or normalization.

### Field checklist for a stereo (2×2) capture

1. In Windows Sound settings, select the playback endpoint and sample rate you intend to use.
2. Keep the software filter and its routing enabled, close other audio applications, and run
   `devices`. Confirm that both source and destination endpoints report `2 channels` and the same
   sample rate.
3. Send impulses to the virtual source with `--render-device-id` and select the final physical
   playback endpoint with `--capture-device-id`. Do not play other audio until capture finishes.
4. Check that the command reports the intended source and destination, a nonzero peak below
   `1.0`, the common trim applied, and writes two WAV files plus
   `captured-filter-equalizer-apo.txt` and `captured-filter-camilladsp.yml`.
5. Copy the whole `captured-filter` directory under Equalizer APO's `config` directory. Disable
   the filter that was captured, then add
   `Include: captured-filter\\captured-filter-equalizer-apo.txt` to the active Equalizer APO
   configuration.
6. Open Equalizer APO's Configuration Editor and confirm that both convolution entries load
   without errors. Test at low volume first with distinct left-only and right-only audio.

## Equalizer APO output

The output directory contains:

- One 32-bit floating-point multichannel WAV per input channel. Its WAV channels contain the
  responses from that input to every output.
- `<filter-name>-equalizer-apo.txt`, which creates virtual channels, convolves every
  input-to-output path, and mixes the matrix back into the physical channels.

Place the entire output directory under Equalizer APO's configuration directory and include the
generated file from the active configuration. Keep the WAV files next to the generated config so
its relative paths remain valid. The endpoint sample rate must equal the WAV sample rate.

Disable the original captured filter before enabling its convolution replacement; otherwise both
filters will be applied.

## CamillaDSP output for Linux and SteamOS

The output directory also contains `<filter-name>-camilladsp.yml`. Both generated config filenames
use the output directory name as `<filter-name>`; for example, `--output my-speakers` writes
`my-speakers-equalizer-apo.txt` and `my-speakers-camilladsp.yml`. Characters that are not portable
in filenames are replaced with hyphens. The CamillaDSP file is a complete native PipeWire
configuration with:

- A `camilladsp-capture` input node with the captured input channel count.
- The full input-to-output convolution matrix, using the same floating-point WAV files as the
  Equalizer APO export.
- A `camilladsp-playback` output node with the captured output channel count.

Copy the entire directory to the Linux or SteamOS machine so the relative WAV paths remain valid.
Use a [CamillaDSP build with the native PipeWire backend][camilladsp-releases], then run this from
that directory:

```bash
camilladsp my-speakers-camilladsp.yml
```

Route application audio into `camilladsp-capture` and route `camilladsp-playback` to the physical
audio output. This can be done with [PipeWire routing rules][camilladsp-pipewire] or a patchbay
such as Helvum or qpwgraph. The generated sample rate must match the PipeWire graph and physical
output. CamillaDSP will validate the configuration without starting audio when run with:

```bash
camilladsp --check my-speakers-camilladsp.yml
```

The generated `chunksize` follows CamillaDSP's starting-point recommendation for the captured
sample rate. It can be tuned later to trade latency against CPU use.

[camilladsp-releases]: https://github.com/HEnquist/camilladsp/releases
[camilladsp-pipewire]: https://github.com/HEnquist/camilladsp/blob/master/backend_pipewire.md

## Experimental Windows GUI

The `gui` directory contains a WinUI 3 frontend built with Windows Reactor. It uses the same
capture and multi-format export library as the CLI. Reactor is currently pinned as a Git
submodule so GUI builds remain reproducible while that project is experimental.

Clone and build it with:

```powershell
git clone --recurse-submodules https://github.com/nukmuk/rust-impulse-response.git
cd rust-impulse-response
cargo build --manifest-path gui/Cargo.toml --release
./gui/package.ps1 -Profile release
```

Run `gui\target\package-release\rust-impulse-response-gui.exe` and keep everything in the
`package-release` directory together. Pull requests also build and test this frontend, then
publish the complete self-contained GUI directory as a GitHub Actions artifact.

## Limits

- Only linear, time-invariant processing can be represented by an impulse response. Compressors,
  limiters, gates, and time-varying effects cannot be reproduced exactly.
- A delay shared identically by every matrix path cannot be distinguished conceptually from common
  pipeline latency. Default leading-silence trimming removes that common latency while keeping all
  channel-relative and path-relative delays intact. Use `--no-trim-leading-silence` to preserve it.
- The source and destination playback endpoints must have the same Windows mix sample rate.
- Stereo is the first validated layout. The core and capture schedule support arbitrary endpoint
  channel counts, using Equalizer APO channel indices when a standard Windows speaker mask is not
  available.

## Architecture

The library owns device discovery, capture scheduling, clock mapping, matrix extraction, and
export. The binary contains only CLI concerns. This boundary is intended to support a later WinUI
3 frontend without routing UI state through command-line output.
