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

## I/O design (streaming by default)

Conversion uses a **bounded-memory pipeline**:

1. **Decode** — `WavSource`, `AlacSource`, or `ApeSource` implement `flacenc::Source` and yield PCM in FLAC block-sized chunks (Symphonia packets for ALAC, `FrameIterator` for APE, incremental `hound` reads for WAV).
2. **Encode** — PCM is accumulated to full FLAC block sizes before each frame is encoded (Symphonia ALAC packets can be smaller than a block). Frames use **fixed blocking** (frame numbers in headers, matching STREAMINFO `min/max blocksize`). Do not use sample-number headers on a fixed-block stream (breaks Safari). After encode, an empty Vorbis Comment block is appended as the metadata tail (like libflac/xrecode) so a manually added SEEKTABLE is not the last block. Compressed frames for the whole album are held in RAM during encode (~output FLAC size, not full PCM). `flacenc` multithread is ignored (parallelism is per-file in the server worker).

Peak decode RAM is **O(FLAC block × channels + one APE frame)**. Encode RAM scales with compressed FLAC size.

**Format preservation:** FLAC uses the source sample rate and bit depth (e.g. 48 kHz / 24-bit ALAC from the MP4 magic cookie). No forced downmix to 44.1 kHz / 16-bit.

`decode_to_pcm` / `encode_flac` + `MemSource` remain for tests and small fixtures.

Progress: `ConvertOptions::on_progress` fires every 32 FLAC frames during encode (used by the convert worker for SSE).

See project rule `.cursor/rules/rust-streaming-io.mdc`.
