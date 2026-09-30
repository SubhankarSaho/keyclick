import { useEffect, useRef, useState, type CSSProperties } from 'react';
import { api } from '../api';

interface VolumeSliderProps {
  volume: number;
  onChange: (volume: number) => Promise<void>;
}

const SEND_INTERVAL_MS = 60;

/** Smooth 0–100 % slider. Sends throttled updates while dragging, then a click preview. */
export function VolumeSlider({ volume, onChange }: VolumeSliderProps) {
  const [value, setValue] = useState(Math.round(volume * 100));
  const dragging = useRef(false);
  const lastSent = useRef(0);
  const pending = useRef<number | null>(null);

  useEffect(() => {
    if (!dragging.current) setValue(Math.round(volume * 100));
  }, [volume]);

  useEffect(() => () => {
    if (pending.current !== null) window.clearTimeout(pending.current);
  }, []);

  const send = (v: number) => {
    lastSent.current = Date.now();
    void onChange(v / 100);
  };

  const onInput = (v: number) => {
    setValue(v);
    if (pending.current !== null) window.clearTimeout(pending.current);
    const wait = SEND_INTERVAL_MS - (Date.now() - lastSent.current);
    if (wait <= 0) send(v);
    else pending.current = window.setTimeout(() => send(v), wait);
  };

  const commit = () => {
    if (!dragging.current) return;
    dragging.current = false;
    if (pending.current !== null) window.clearTimeout(pending.current);
    pending.current = null;
    send(value);
    if (value > 0) void api.preview('normal');
  };

  return (
    <div className="volume">
      <input
        type="range"
        min={0}
        max={100}
        step={1}
        value={value}
        aria-label="Volume"
        style={{ '--fill': `${value}%` } as CSSProperties}
        onPointerDown={() => (dragging.current = true)}
        onKeyDown={() => (dragging.current = true)}
        onChange={(e) => onInput(Number(e.target.value))}
        onPointerUp={commit}
        onKeyUp={commit}
        onBlur={commit}
      />
      <span className="volume-value">{value}%</span>
    </div>
  );
}
