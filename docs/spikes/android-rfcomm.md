# Spike: Headphone Bridge on Android

**Date**: 2026-10-14 · **Branch**: `spike/android-rfcomm` · **Phone**: Galaxy S25 Ultra (SM-S938B), Android 16 (API 36)

## Question

Can the real app (the React UI plus `crates/core`, unchanged) open the vendor control link from Android and control a WH-1000XM5 and a Galaxy Buds4 Pro, given that Samsung's Buds plugin and Sony Sound Connect are installed and want the same link?

## Answer

**Yes, but it can't share the link with the vendor apps.** Both Devices Attach, and every setting works: Noise Control Mode, Ambient Level, Noise Cancelling Level. This needs no root and nothing uninstalled. On an RFCOMM channel only one app's link can be open at a time, though. Each new connection attempt knocks off whoever holds the link. The core retries forever, so while Headphone Bridge runs it always ends up holding the link and the vendor apps are locked out. Closing Headphone Bridge lets them connect again.

| Situation | Result |
|---|---|
| Vendor apps running when the Device connects | Attaches after 3 drops, 7–9 s in all, then keeps the link; the vendor app gives up |
| Vendor apps closed | Attaches on the first attempt, with no drops |
| Vendor app opened while we're Attached | The vendor app can't connect, until Headphone Bridge is swiped away |
| Headphone Bridge swiped away | The Buds plugin connects on its next attempt, 3 s later |

## What was built

- `crates/platform-android`: a Tauri v2 plugin crate.
  - Its Kotlin side (`BluetoothPlugin`) watches Bluetooth connections (`ACL_CONNECTED`/`ACL_DISCONNECTED`, plus the A2DP and headset profiles for Devices already Connected) and the media output (`getAudioDevicesForAttributes`).
  - It opens a socket with `createRfcommSocketToServiceRecord` (falling back to the insecure variant), then reads and writes on background threads.
  - Kotlin streams to Rust through `tauri::ipc::Channel`s passed in each command's payload.
  - Its Rust side, `AndroidBluetooth`, implements the core's `Platform`.
- The shell gates the tray, autostart, single instance and hide-on-close to `desktop`. The UI hides Launch at login on Android.
- `crates/core` is unchanged.

## How the vendor apps and Headphone Bridge contend

The Bluetooth stack's own log (`bt_btif_sock_rfcomm`, `BluetoothSocketManagerBinder`) records every app's connections by uid. In the excerpts below, 10585 is Headphone Bridge, 10345 is the Buds plugin (`com.samsung.accessory.budsunitemgr`) and 10509 is Sony Sound Connect (`com.sony.songpal.mdr`).

The Buds4 Pro connecting, with its plugin running (the vendor service is RFCOMM channel 27):

```
20:57:33.83  us      connect → link 1
20:57:35.23  plugin  connect ch 27 → fails; our link 1 closes 15 ms later
20:57:36.32  us      reconnect → link 2
20:57:38.31  plugin  connect ch 27 → fails; link 2 closes
20:57:39.46  us      reconnect → link 3
20:57:41.42  plugin  connect ch 27 → fails; link 3 closes
20:57:42.63  us      reconnect → link 4, held until the Buds disconnected 31 s later
```

The XM5 behaved the same with Sound Connect on channel 9: its attempts came at 23.7, 25.98 and 28.09 s, and our fourth link held.

It works both ways. At 21:01:24 the plugin held the Buds' link, and our connection closed it within 33 ms. So there's no first-come ownership: the latest connection attempt breaks the existing link, and usually fails itself. The vendor apps retry 3 times about 3 s apart, then give up. The core retries about 1 s after every drop and never gives up, so it always wins.

## Other findings

- **The first connection attempt right after a Device connects fails.** Android logs `PORT_StartCnf: Unable start configuration dlci:54 result:61440`: the phone and the Device are still setting up their RFCOMM connection. The insecure retry 1 ms later succeeded only because it came later; every secure attempt after that succeeded. The insecure fallback isn't needed, but a short wait after the Device connects (as on macOS) is.
- **Primary Device works.** The media output reports the Device's Classic Bluetooth address (`…5E:FF`, `…17:11`), the same address its control link uses.
- **The Buds4 Pro also connect on a separate LE-only address** (`…55:CB`, "Galaxy Buds4 Pro") with only LE Audio services. The core ignores it, because it has no vendor UUID.
- **Devices that are already Connected are found at launch** (the XM5 at 21:02:51) and Attached.
- **The service UUIDs were always cached**, so the wait for Android to look them up never ran.
- **Sound Connect restarts itself** when the XM5 connects, so force-stopping it lasts only until the next connection.
- **The Bluetooth permission prompt** ("Nearby devices") appears on first launch and works through Tauri's `@Permission`.

## What it means for an Android adapter

The adapter works; the open question is how to share the link with the vendor apps. Options, cheapest first:

1. **Hold the link only while the app is in the foreground**: close it on `onPause` and reopen it on `onResume`. The vendor app then works whenever the user opens it, because ours is in the background by then. The cost is that Status doesn't update in the background, which matters less on Android, which has no tray. This needs no core change, only a way to pause the core's reconnecting.
2. **Back off after being knocked off.** If a link drops within a few seconds of opening, assume another app wants it, and stop retrying until the user taps Retry. That's a change to the core's retry policy.
3. Both.

The recommendation is option 1, plus a short wait after the Device connects before the first open.

## Not tested

- Opening the vendor app *while* we're Attached and in the background. Option 1 would make this the normal case.
- Running with the vendor apps uninstalled. The contention above makes the result predictable: we'd Attach first time, with no rival.
- Power Off on Android. A 12-byte write was followed 150 ms later by the XM5 disconnecting, but that wasn't confirmed as Power Off.
- Battery and memory use. Each link leaks one `Channel` entry, because Tauri never releases them.

## Reproducing

```sh
export PATH=/opt/homebrew/opt/rustup/bin:$PATH NDK_HOME=$HOME/Library/Android/sdk/ndk/27.1.12297006
cd app && npx tauri android build --debug --target aarch64 --apk
adb install -r src-tauri/gen/android/app/build/outputs/apk/universal/debug/app-universal-debug.apk
adb logcat -s HeadphoneBridge     # the app's own lines; Rust lines are prefixed [rust]
adb logcat | grep -E 'connectSocket|cleanup_rfc_slot'   # every app's RFCOMM opens and closes
```
