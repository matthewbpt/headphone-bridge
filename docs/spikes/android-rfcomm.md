# Spike: Headphone Bridge on Android

**Date**: 2026-10-14 · **Branches**: `spike/android-rfcomm` (spike 1), `spike/android-widget` (spike 2, below) · **Phone**: Galaxy S25 Ultra (SM-S938B), Android 16 (API 36)

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

## Spike 2: a home screen widget and a Quick Settings tile

**Question**: following the recommendation above, can a home screen widget and a Quick Settings tile change the Noise Control Mode with the app closed, Attaching only for each tap?

**Answer**: **yes, both work, on both Devices.** That includes the tile from the lock screen, and a widget tap with the app's process stopped. The user liked both, and especially the tile working while locked. Holding the control link only while the app is in the foreground also works.

### What was built

- **One core per process, owned outside Tauri.**
  - A widget or tile tap may start the process with no Activity. Tauri only starts its Rust app from the Activity, so the core can't live inside the Tauri app.
  - A Kotlin `Hub` owns Bluetooth and talks to the core over plain JNI, exchanging JSON messages (`NativeCore.event`, `.command` and `.fromCore`).
  - The app's window, the widget and the tile all share that core. The shell takes it as `&'static Core`.
- **A gate.**
  - Devices reach the core only while something holds `Hub`'s gate. Releasing the gate reports them disconnected, which closes their links.
  - The app holds the gate while it's in the foreground, and keeps it for 3 s after it leaves. Each widget or tile action holds it until the Device confirms the new mode.
- **The widget** shows the Primary Device's name and battery, and a button per Noise Control Mode in its Capabilities. Tapping the name refreshes; "Open" opens the app. It keeps the last known state across process deaths.
- **The tile** steps to the next mode on each tap; quick repeated taps make one change.
- **Timing gotcha**: the Tauri plugin's Kotlin `load()` runs *after* the Rust setup, not before. So the core starts on first use from either side, and Kotlin attaches to it whenever it arrives.

### Measurements

Time from the tap to the Device confirming the mode, with the link closed beforehand:

| Device | Widget or tile tap |
|---|---|
| Buds4 Pro | 0.28–0.92 s (65 ms when the app already held the link) |
| WH-1000XM5 | 0.74–1.3 s; 6.9 s on the first tap after it reconnected |

The XM5's slow first tap is its secure connect failing after 6.3 s; the insecure retry then connects in 0.4 s. On the XM5 the first secure connect failed every time it had just connected (three times across the spikes), so connecting insecure first for it would save that time.

### Multipoint: the other hosts contend too

For several minutes the XM5 dropped every link within about 1 s of opening, sometimes before it answered anything. On the phone, nothing else touched its control channel. The stack logged a disconnect from the remote (`RFC_PORT_EVENT_DISC`), and that was all.

The cause was the XM5 also being connected to the user's laptop. It allows one control session across all its hosts, so a host connecting knocks off the one holding the link, as on a single phone. Disconnecting the laptop and power-cycling the XM5 fixed it: the next link stayed up until the app released it.

This matters beyond Android. The desktop app holds the link forever, so with multipoint it would always beat the phone (and the phone's vendor app). Every platform should yield the link, perhaps holding it only while that host is the Device's active audio source. Also, the UI's "control link dropped" gave no hint; something like "another device or app may be using it" would have.

### What it means

- **Keep both the widget and the tile.** They share all their logic: one `Controller` and the cached display state.
- **On Android, the core belongs in the app's process, outside the Tauri webview's lifetime.** The JNI host from spike 2 is the shape to keep, not spike 1's Tauri-plugin bridge.
- **Hold the link only while it's needed**, on every platform: the foreground app, or a single action. That's now proven on Android.
- **The widget can show a stale mode.** It shows the last mode seen, so a change made on the Device itself appears only after the next tap or refresh. That was acceptable in use, but worth saying in the UI ("Updated 5 min ago" does this).
- **Lock screen**: the tile acts without unlocking. Noise Control Mode seems harmless to change from there, but a shipped app should decide that deliberately, and Power Off probably shouldn't work while locked.

### Not tested in spike 2

- Using the widget or tile while a vendor app is open: it should hit the same contention as spike 1.
- Long-term behaviour under Doze and background restrictions, and whether Samsung's battery management kills the process during a tap.
- Battery use.

## Reproducing

```sh
export PATH=/opt/homebrew/opt/rustup/bin:$PATH NDK_HOME=$HOME/Library/Android/sdk/ndk/27.1.12297006
cd app && npx tauri android build --debug --target aarch64 --apk
adb install -r src-tauri/gen/android/app/build/outputs/apk/universal/debug/app-universal-debug.apk
adb logcat -s HeadphoneBridge     # the app's own lines; Rust lines are prefixed [rust]
adb logcat | grep -E 'connectSocket|cleanup_rfc_slot'   # every app's RFCOMM opens and closes
```

On `spike/android-widget`, add the widget from the launcher's widget list ("Noise Control"), and the tile from the Quick panel's edit screen. A cold widget tap can be simulated with:

```sh
adb shell am force-stop io.github.matthewbpt.headphone_bridge
adb shell am broadcast -n io.github.matthewbpt.headphone_bridge/io.github.matthewbpt.headphone_bridge.bluetooth.ModeWidget \
  -a io.github.matthewbpt.headphone_bridge.REFRESH
```
