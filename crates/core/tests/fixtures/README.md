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
2. **Device Model**: its slug (`galaxy-buds4-pro`, `galaxy-buds2`, `wh-1000xm5`). If the Model
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
  - `real-equalizer-custom`: from the frame log on Linux, 2026-10-18,
    firmware `R640XXU0AZD2`. Samsung's app can't connect while this app is
    Attached, so the Device was disconnected from the laptop, Samsung's app set
    a custom curve, and it was connected again. This Attach reports Preset 6:
    the Buds4 Pro's Custom is 6 on the wire, not the 7 the upstream-derived
    protocol gave.
- `galaxy-buds2/`: a Galaxy Buds2 on firmware revision 11, device id 314,
  from the frame log on macOS, on 2026-10-17. Two link sessions, cut into the
  `real-*` files.
  - `real-attach`: the Attach, answered with an extended status, then the
    Device's own traffic.
  - `real-noise-control`: Attach, then four mode sets, each confirmed only
    by an ACK carrying the applied mode: the set's `MANAGER_INFO` gets only
    a bare `88 00`, never an extended status. Then a bud leaves the ear and goes
    back in: a 5-byte `NOISE_CONTROLS_UPDATE` with cause 2 (placement
    change) at byte 4 pauses Ambient to Off, then resumes it.
  - `real-gesture`: the second session, with long-press set
    to Noise Controls. Attach, then four long-presses between Noise
    Cancelling and Ambient, each a 5-byte `NOISE_CONTROLS_UPDATE` with cause
    1 (gesture) at byte 4, and nothing sent to the Device.
  - `real-ambient-level`: Attach, then six Ambient Level sets down to 0,
    each confirmed only by an ACK carrying the applied level.
  - `attach`: synthesised. The app's `MANAGER_INFO`, answered by the real
    revision-10 extended status in GalaxyBudsClient's tests
    (`TestData/ExtendedStatusUpdate/Buds2_rev10.bin`, GPL-3.0,
    timschneeb/GalaxyBudsClient at 754b6fd), wire bytes verbatim. Device id
    316 (Green), Noise Cancelling, Ambient Level 1.
- `wh-1000xm5/`: `attach` is synthesised from libmdr's protocol definitions
  (MIT, mos9527/SonyHeadphonesClient) by `wh-1000xm5/gen.py`. The `real-`
  fixtures are from the frame log.
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
  - `real-equalizer-presets`: from the frame log on Linux, 2026-10-12, with
    the same protocol info as `real-attach`. An Attach that reads the
    Equalizer, then twelve Presets picked alone, each notified with its
    curve. It replays exactly.
  - `real-equalizer-curve`: the same link session continued: Band and Clear
    Bass edits, and two Noise Control Mode sets. The Device notifies an edit
    only when it moves onto Manual from another Preset. An edit already on
    Manual is only ACKed. It was recorded before the app read a curve back
    after its ACK, so its replay plays the Device's side from it rather than
    matching every frame the app sends.
  - `real-equalizer-kept`: the next Attach, two minutes later. The Device
    reports the last curve sent, which it had only ACKed.
  - `real-equalizer-more-presets`: from the frame log on Linux, 2026-10-18,
    firmware 2.5.1. An Attach, then Mellow, Vocal, Custom 1 and Custom 2
    picked alone, each notified with its curve. A development build sent
    `EQEBB_GET_CAPABILITY` for `PRESET_EQ` once after Attach; its
    `EQEBB_RET_CAPABILITY` advertises Clear Bass and five Bands of 21 steps
    and the twelve Presets the Model lists, with empty names. The app as it
    ships never sends it, so replays drop the probe with
    `support::without_capability_probe`, which re-sequences the frames after
    it.
  - `real-equalizer-from-sonys-app`: the next session, the same day. Sony's
    app can't Attach while this app is, so the app was closed, the phone
    moved the Device to Custom 1 with Clear Bass +5, and the app was opened
    again. This Attach reads that change. It carries the same probe.
