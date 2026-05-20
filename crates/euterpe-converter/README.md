# euterpe-converter

Lossless audio conversion to FLAC for Euterpe.

## Supported inputs (phase 1)

- WAV
- ALAC (`.m4a`, `.mp4`, `.caf` — AAC is rejected)
- APE (Monkey's Audio ≥ 3.95)

## Backlog

- **WavPack** (`.wv`) — enable via feature flag `wavpack` when a decoder is integrated.

## PCM guarantee

Encoding presets affect compression ratio and CPU time only. Roundtrip tests assert **identical PCM samples** after FLAC decode.
