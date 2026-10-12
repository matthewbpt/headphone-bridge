import { useState } from "react";
import { setEqualizer } from "./core";
import type { Bands, Device, Equalizer as EqualizerValue, EqualizerCapability } from "./deviceList";
import { usePendingSetting } from "./usePendingSetting";

/**
 * The Equalizer card, drawn from the Model's Capability alone: a Preset
 * dropdown listing the Model's Presets under its own names and, once the
 * Device reports them, the curve as a slider per Band. A picked Preset or a
 * released Band is Pending until the Device reports it. While the Device is
 * Equalizer Unavailable the card stays, disabled, and says why.
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
  // report settles it. Only what the Model lets the app set is sent.
  const { shown, pending, error, request } = usePendingSetting(
    device.equalizer,
    (asked: EqualizerValue) => setEqualizer(device.address, settable(capability, asked)),
    (asked) => `${presetName(capability, asked.preset)} ${asked.bands ? "curve" : "Preset"}`,
  );
  // A Preset the Model doesn't list, e.g. one added by a firmware update,
  // selects nothing. Neither does one not reported yet.
  const listed = shown !== null && capability.presets.some((preset) => preset.id === shown.preset);

  /**
   * Asks for `bands`: on the shown Preset if its Bands can be changed, else
   * on the Model's first editable Preset (the XM5's Manual), which starts
   * from the shown curve so nothing jumps.
   */
  const requestBands = (bands: number[]) => {
    if (shown === null) return;
    const editable = capability.presets.find((preset) => preset.id === shown.preset)?.editable;
    const preset = editable ? shown.preset : capability.presets.find((preset) => preset.editable)?.id;
    if (preset === undefined) return;
    request({ ...shown, preset, bands });
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
        <Curve bands={capability.bands} values={shown.bands} pending={pending !== null} onRelease={requestBands} />
      )}
      {device.equalizerUnavailable && (
        <p className="hint">The Device isn't taking Equalizer changes right now.</p>
      )}
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
  const [dragged, setDragged] = useState<number[] | null>(null);
  const curve = dragged ?? values;
  const release = () => {
    if (dragged === null) return;
    setDragged(null);
    onRelease(dragged);
  };

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
          <input
            type="range"
            min={min}
            max={max}
            step={1}
            value={curve[i]}
            aria-label={`${frequency} Band`}
            aria-busy={pending}
            onChange={(e) => setDragged(curve.map((value, j) => (j === i ? Number(e.target.value) : value)))}
            onPointerUp={release}
            onKeyUp={release}
          />
          <span aria-hidden>{frequency}</span>
        </label>
      ))}
    </div>
  );
}

/** `asked`, with what the Model doesn't let the app set left unchanged. */
function settable(capability: EqualizerCapability, asked: EqualizerValue): EqualizerValue {
  return { ...asked, clearBass: capability.clearBass ? asked.clearBass : null };
}

/** The Model's name for the Preset with wire id `id`. */
function presetName(capability: EqualizerCapability, id: number): string {
  return capability.presets.find((preset) => preset.id === id)?.name ?? String(id);
}
