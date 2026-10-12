# Fixtures

Recorded control-link traffic that drives the seam tests. The frame log writes
the same format, so a log captured on real hardware can be dropped in here and
replayed as a regression test.

## Format (`.frames`)

A UTF-8 text file with one control-link session per file. Each line is a frame
record, a comment or blank:

```
# Galaxy Buds4 Pro, firmware rev 4.
2026-09-13T13:26:47.128Z galaxy-buds4-pro >> fd 06 00 88 01 01 22 da 58 dd
2026-09-13T13:26:47.207Z galaxy-buds4-pro << fd 41 00 61 04 0d 64 61 …
```

The fields of a record are separated by single spaces:

1. **Timestamp**: RFC 3339, UTC, millisecond precision (`YYYY-MM-DDTHH:MM:SS.mmmZ`).
2. **Device Model**: its slug (`galaxy-buds4-pro`, `wh-1000xm5`). If the Model
   isn't identified yet, the vendor's slug (`samsung`, `sony`).
3. **Direction**: `>>` for a frame the app sent to the Device, `<<` for a frame
   received from it.
4. **Raw bytes**: the whole frame as it appears on the wire, start and end
   bytes and checksum included. Lowercase hex, one space between bytes.

Lines starting with `#` are comments. The parser is
`headphone_bridge_core::frame_log::FrameRecord` (`FromStr` / `Display`).

## From a frame log

Switch on **Developer › Frame log** in the window. Frames are appended to
`frame-log.frames` in the app's log directory (on macOS
`~/Library/Logs/dev.matthewbpt.headphonebridge/`); the window shows the full path. Each
time the log is switched on it adds a `# Headphone Bridge frame log` comment,
and one file can hold several link sessions and Devices. Cut out the session
you want, add a comment saying what happened, and save it here as a `.frames`
file.

## Contents

- `galaxy-buds4-pro/`: converted from the omarchy-buds captures (MIT, see
  `galaxy-buds4-pro/omarchy/LICENSE`). The originals and the converter
  (`omarchy/convert.py`) sit alongside. A Galaxy Buds4 Pro on firmware revision
  4, captured on 2026-09-13.
  - `capture-1a`…`1c`, `capture-2`…`4`: re-encoded from decoded logs. capture-1
    holds three link sessions, so it is split into three files.
  - `raw-1`: 27 frames with their original wire bytes, including the header's
    sequence and unknown flag bits.
  - `real-noise-control`: not from omarchy-buds but from the frame log, on
    2026-10-11. Attach, then two mode sets, each confirmed only by an ACK
    carrying the applied mode.
  - `real-nc-level-0`…`4`: from the frame log, on 2026-10-12. One Attach
    each, with Noise Cancelling chosen and the Noise Cancelling Level set to
    0…4 on the phone beforehand. Extended status byte 24 reports the level.
  - `real-nc-level-set`: from the frame log, on 2026-10-12. Attach, then
    Noise Cancelling Level and mode sets from the app, each level set
    confirmed only by an ACK carrying the applied level.
- `wh-1000xm5/`: synthesised from libmdr's protocol definitions (MIT,
  mos9527/SonyHeadphonesClient) by `wh-1000xm5/gen.py`, until real captures
  exist from the frame log. No WH-1000XM5 capture exists yet.
  - `attach`: identification by model name, support functions, battery,
    noise control and the Equalizer.
  - `real-attach`: a real Attach from the frame log, WH-1000XM5 firmware 2.5.1.
  - `real-noise-control`: real mode and Ambient Level sets, each confirmed by
    a notification, firmware 2.5.1.
  - `real-noise-control` and `real-power-off` were recorded before the app
    read the Equalizer on Attach. Their replays re-base them with
    `support::with_equalizer_read`, which inserts synthetic reads of the
    Equalizer's Status and params. The two reads leave the sequence numbers
    of every later frame as recorded.
