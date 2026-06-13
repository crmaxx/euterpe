# SMB share enumeration (classic NDR) — design

## Problem

`POST /api/v1/settings/storage/smb-shares` fails on Samba/DietPi with `RPC_BAD_STUB_DATA` (`0x000006f7`). macOS Finder and direct `smb://` mount work on the same host and credentials.

## Root cause (pcap)

| Capture | Enum transport | Stub | Result |
|---------|----------------|------|--------|
| `app_smb.txt` | SMB2 **Write/Read** | 76 B, inline InfoCtr | Success (`dietpi`, `IPC$`) |
| `app5_smb.txt` | **FSCTL_PIPE_TRANSCEIVE** | 76 B / 80 B, retries | All `0x6f7` faults |

The 80 B variant (extra `info_ctr` ref before `level`) is invalid — Wireshark shows `Level: 131072` (`0x20000`).

## Decision

1. **Enum RPC** uses `PipeRpcConnection::dcerpc_pipe_exchange` (Write/Read), same as bind and successful `app_smb`.
2. **Single request stub**: 76 bytes for `\\host` UNC — server ref `0x20000`, UTF-16 with NUL, level 1, Ctr1 ref `0x20000`, count 0, null array, `max_buffer = 0xffffffff`, null resume.
3. **No** 80 B container-ref variant, **no** opnum `0x24` retry loop.
4. **Response** decoder unchanged (union tag, two-pass share array).
5. **Errors**: `0x6f7` → `SMB_SHARE_ENUM_UNSUPPORTED` (501); fault PDU parsing stays in vendored `smb`.

## Success criteria

`POST /api/v1/settings/storage/smb-shares` with `host=192.168.0.124`, user `dietpi` returns share names including `dietpi` (hidden `$` shares filtered).

## Tests

- Golden hex unit test for 76 B request (`\\192.168.0.124`).
- Existing decode tests for Samba response layout.
