import { useState } from "react";
import { setEqualizer, whyRejected } from "./core";
import type {
  Bands,
  Device,
  Equalizer as EqualizerValue,
  EqualizerCapability,
  Preset,
  StepRange,
} from "./deviceList";
import { sameValue, usePendingSetting } from "./usePendingSetting";

/** The reason the card gives while Equalizer Unavailable: the core's own. */
const UNAVAILABLE = (() => {
  const why = whyRejected("equalizerUnavailable");
  return `${why[0].toUpperCase()}${why.slice(1)}.`;
})();

/**
 * The Equalizer card, drawn from the Model's Capability alone: a Preset
 * dropdown listing the Model's Presets under its own names and, once the
 * Device reports them, the curve as a slider per Band and a Clear Bass
 * slider. A picked Preset or a released slider is Pending until the Device
 * reports it, and a Preset picked alone settles on that Preset whatever its
 * curve. While the Device is Equalizer Unavailable the card stays, disabled, and says why.
 */
export function Equalizer({
  device,
  capability,
  disabled,
}: {
  device: Device;
  capability: EqualizerCapability;
  /** While the Device is Powering Off. */
  disabled: boolean;
}) {
  // A Pending curve is the whole value the Device should report, so its
  // report settles it.
  const {
    shown: shownAsked,
    pending,
    error,
    request,
  } = usePendingSetting(
    device.equalizer,
    (asked: EqualizerValue) => setEqualizer(device.address, asked),
    (asked) => `${presetName(capability, asked.preset)} ${asked.bands ? "curve" : "Preset"}`,
    settles,
  );
  // A Preset picked alone keeps showing the reported curve until the
  // Device reports the Preset's own.
  const shown = shownAsked && {
    ...shownAsked,
    bands: shownAsked.bands ?? device.equalizer?.bands ?? null,
    clearBass: shownAsked.clearBass ?? device.equalizer?.clearBass ?? null,
  };
  // A Preset the Model doesn't list, e.g. one added by a firmware update,
  // selects nothing. Neither does one not reported yet.
  const listed = shown !== null && preset(capability, shown.preset) !== undefined;

  /**
   * Asks for the shown curve with `edit` applied: on the shown Preset if its
   * curve can be changed, else on the Model's first editable Preset (the
   * XM5's Manual), which starts from the shown curve so nothing jumps.
   */
  const requestCurve = (
    edit: Pick<EqualizerValue, "bands"> | Pick<EqualizerValue, "clearBass">,
  ) => {
    if (shown === null) return;
    const id = preset(capability, shown.preset)?.editable
      ? shown.preset
      : capability.presets.find((preset) => preset.editable)?.id;
    if (id === undefined) return;
    request({ ...shown, preset: id, ...edit });
  };

  return (
    <fieldset className="card equalizer" disabled={disabled || device.equalizerUnavailable}>
      <legend className="equalizer__title">Equalizer</legend>
      <select
        aria-label="Preset"
        aria-busy={pending !== null}
        value={listed && shown ? shown.preset : ""}
        disabled={shown === null}
        onChange={(e) => request({ preset: Number(e.target.value), bands: null, clearBass: null })}
      >
        {!listed && (
          <option value="" disabled>
            {shown === null ? "" : "Unrecognised Preset"}
          </option>
        )}
        {capability.presets.map((preset) => (
          <option key={preset.id} value={preset.id} disabled={!preset.selectable}>
            {preset.name}
          </option>
        ))}
      </select>
      {capability.bands && shown?.bands && (
        <Curve
          bands={capability.bands}
          values={shown.bands}
          pending={pending !== null}
          onRelease={(bands) => requestCurve({ bands })}
        />
      )}
      {capability.clearBass && shown?.clearBass != null && (
        <ClearBass
          range={capability.clearBass}
          value={shown.clearBass}
          pending={pending !== null}
          onRelease={(clearBass) => requestCurve({ clearBass })}
        />
      )}
      {device.equalizerUnavailable && <p className="hint">{UNAVAILABLE}</p>}
      {error && (
        <p className="error" role="alert">
          {error}
        </p>
      )}
    </fieldset>
  );
}

/**
 * The Equalizer curve: a vertical slider per Band, lowest first, with a line
 * joining their knobs. A Band being dragged moves only here; letting it go,
 * by pointer or key, hands the whole curve to `onRelease`.
 */
function Curve({
  bands,
  values,
  pending,
  onRelease,
}: {
  bands: Bands;
  /** The shown curve: Pending, else reported. */
  values: number[];
  pending: boolean;
  onRelease: (values: number[]) => void;
}) {
  const { dragged, drag, release } = useDrag(onRelease);
  const curve = dragged ?? values;

  const { min, max } = bands.range;
  const points = curve.map((value, i) => `${i + 0.5},${(max - value) / (max - min)}`).join(" ");
  return (
    <div className="equalizer__bands" data-pending={pending}>
      <svg
        className="equalizer__line"
        viewBox={`0 0 ${curve.length} 1`}
        preserveAspectRatio="none"
        aria-hidden
      >
        <polyline points={points} vectorEffect="non-scaling-stroke" />
      </svg>
      {bands.frequencies.map((frequency, i) => (
        <label key={frequency} className="equalizer__band">
          {/* A horizontal slider turned upright: WebKitGTK lays out a range
              input in a vertical writing-mode with its knob stuck mid-track. */}
          <span className="equalizer__slider">
            <input
              type="range"
              min={min}
              max={max}
              step={1}
              value={curve[i]}
              aria-label={`${frequency} Band`}
              aria-orientation="vertical"
              aria-busy={pending}
              onChange={(e) =>
                drag(curve.map((value, j) => (j === i ? Number(e.target.value) : value)))
              }
              onPointerUp={release}
              onKeyUp={release}
            />
          </span>
          <span aria-hidden>{frequency}</span>
        </label>
      ))}
    </div>
  );
}

/**
 * Clear Bass as a horizontal slider. Like a Band, a drag moves only here
 * until it's let go, by pointer or key, and then goes to `onRelease`.
 */
function ClearBass({
  range,
  value,
  pending,
  onRelease,
}: {
  range: StepRange;
  /** The shown Clear Bass: Pending, else reported. */
  value: number;
  pending: boolean;
  onRelease: (value: number) => void;
}) {
  const { dragged, drag, release } = useDrag(onRelease);

  return (
    <label className="equalizer__clear-bass" data-pending={pending}>
      <span>Clear Bass</span>
      <input
        type="range"
        min={range.min}
        max={range.max}
        step={1}
        value={dragged ?? value}
        aria-label="Clear Bass"
        aria-orientation="horizontal"
        aria-busy={pending}
        onChange={(e) => drag(Number(e.target.value))}
        onPointerUp={release}
        onKeyUp={release}
      />
    </label>
  );
}

/**
 * A slider's drag: the value being dragged, shown only here until `release`,
 * on pointer or key up, hands it to `onRelease`. Null while not dragged.
 */
function useDrag<T>(onRelease: (value: T) => void) {
  const [dragged, setDragged] = useState<T | null>(null);
  const release = () => {
    if (dragged === null) return;
    setDragged(null);
    onRelease(dragged);
  };
  return { dragged, drag: setDragged, release };
}

/**
 * Whether the Device reporting `reported` settles `asked`: on its Preset,
 * with the asked Bands and Clear Bass where `asked` gives them.
 */
function settles(asked: EqualizerValue, reported: EqualizerValue): boolean {
  return (
    asked.preset === reported.preset &&
    (asked.bands === null || sameValue(asked.bands, reported.bands)) &&
    (asked.clearBass === null || asked.clearBass === reported.clearBass)
  );
}

/** The Model's Preset with wire id `id`, if it lists one. */
function preset(capability: EqualizerCapability, id: number): Preset | undefined {
  return capability.presets.find((preset) => preset.id === id);
}

/** The Model's name for the Preset with wire id `id`. */
function presetName(capability: EqualizerCapability, id: number): string {
  return preset(capability, id)?.name ?? String(id);
}
