# euterpe-converter

Lossless audio conversion to FLAC for Euterpe.

## Supported inputs (phase 1)

- WAV
- ALAC (`.m4a`, `.mp4`, `.caf` — AAC is rejected)
- APE (Monkey's Audio ≥ 3.95)

## Backlog

- **WavPack** (`.wv`) — behind the `wavpack` feature.

## PCM guarantee

Encoding presets affect compression ratio and CPU time only. Roundtrip tests assert **identical PCM samples** after FLAC decode.

## I/O design

Production conversion uses `flac-bound` over vendored static `libFLAC`. The previous pure-Rust
encoder path produced formally valid files, but 24-bit transient material could stall
browser/Howler playback even though `flac -t` and Audacity accepted it. `libFLAC` produces
browser-compatible bitstreams for the same PCM.

The Rust streaming decoder modules feed PCM directly into `libFLAC`. After encode, an empty
Vorbis Comment block is appended as the metadata tail when needed, then source tags are transferred.

Docker builder images install `cmake` to build the vendored encoder. Runtime images do not need
a separate `libFLAC` package.

**Format preservation:** FLAC uses the source sample rate and bit depth (e.g. 48 kHz / 24-bit ALAC from the MP4 magic cookie). No forced downmix to 44.1 kHz / 16-bit.

`decode_to_pcm` remains for tests and small fixtures.

See project rule `.cursor/rules/rust-streaming-io.mdc`.
