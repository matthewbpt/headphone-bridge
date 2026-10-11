# PROTOTYPE brief: window redesign (throwaway, branch `prototype/ui`)

Settled in a grilling session (design-session worktree). Run `/prototype` (UI branch, sub-shape A) against this.

## Question
What should the Headphone Bridge window look like, redesigned after the Samsung Galaxy Buds app?
Reference screenshots (read them):
- /Users/matthew/.herdr/worktrees/headphone-bridge/design-session/Screenshot_20261011_211222_Galaxy Buds.jpg
- /Users/matthew/.herdr/worktrees/headphone-bridge/design-session/Screenshot_20261011_211249_Galaxy Buds.jpg

## How to run it
The frontend (`app/src`) must run in a plain browser via `npm run dev` (Vite) with **no Bluetooth or Tauri backend**:
stub `deviceList.ts`'s `invoke`/`listen` with made-up `DeviceList` snapshots and `Labels` (copy label text from
`app/src-tauri/src/labels.rs`). Setters (`setNoiseControlMode`, `setAmbientLevel`, `retryAttach`) mutate the
in-memory fake snapshot after a ~400 ms delay so pending/optimistic states are visible. Add one npm script, e.g.
`npm run prototype`. A state picker (separate from the variant switcher) chooses which scenario is loaded.

## Decided layout (top to bottom)
1. Header: OS Device name as title; Model name as muted subtitle only when it differs from the name.
   Device switcher only when >1 Device Connected.
2. Device image: per-Model SVG drawn by you (Galaxy Buds4 Pro pair; WH-1000XM5). Generic headphone glyph for
   unidentified/unsupported Model. Always shown.
3. Under image: battery when Attached; otherwise "Attaching…" or the not-Attached reason + Retry.
   Battery: buds -> "L·R n%" when equal, "L n% · R n%" otherwise; "Case n%" only when case is non-null;
   headphones -> single "n%". Small battery glyph per value. No arc gauge. No charging state.
4. Noise Control Mode selector, Samsung-style: round icon buttons joined by a track, selected one filled accent,
   label under each. Only the Model's modes (4 on Buds4 Pro, 3 on XM5). Glossary labels (Off, Noise Cancelling,
   Ambient, Adaptive), NOT Samsung's wording. Draw icons: Off = plain person; Ambient = person with radiating dots;
   Adaptive = person with waveform; Noise Cancelling = person in closed ring.
5. Ambient Level slider ("Ambient Level", notched dots, -/+ buttons) shown ONLY when the Chosen Mode is Ambient,
   rendered optimistically: a pending mode counts as Chosen for visibility; reverts if rejected/timed out.
6. Effective Mode hint (e.g. bud out of ear) under the selector. Drop the old "Applying: X" line.
7. App settings (Launch at Login, frame log) in a collapsed section at the bottom.

Out of scope: tray menu, Noise Cancelling Level slider (separate ticket), media/pinch controls, charging state.
Styling: one plain CSS file with custom properties; follows OS light/dark (`prefers-color-scheme`);
dark palette from the screenshots (near-black cards, blue accent). No new dependencies. Window 420x560.
Use GLOSSARY.md terms throughout.

## What the variants must answer
- SVG style: (a) flat filled two/three-tone silhouettes vs (b) single-stroke line art. Show both for each Model.
- Device switcher: (a) pills row above the image vs (b) dropdown on the header title. Show both.
Combine these into 2-4 structurally distinct variants via `?variant=`, plus a light/dark toggle in the bar.

## Scenarios (state picker), each viewable light and dark
Buds4 Pro Attached, Ambient chosen (slider visible); Buds4 Pro Noise Cancelling (no slider); mode pending;
Effective Mode hint shown (bud out of ear); XM5 Attached (3 modes, headphones battery); Attaching…;
not Attached with reason + Retry; unsupported Model (generic glyph); two Devices Connected (switcher).

## Handoff
Commit everything on `prototype/ui` (never main). Don't push or open a PR. When done, report the run command,
URLs/variant keys, and a summary; the design-session agent will collect the user's verdict and write the spec.
