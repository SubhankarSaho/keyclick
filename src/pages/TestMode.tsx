import { useRef } from 'react';
import type { PageProps } from '../App';
import { PowerToggle } from '../components/Toggle';

/**
 * Local scratch area for trying KeyClick. The textarea is uncontrolled: its
 * text is never read, stored or sent, and disappears when the page closes.
 * Sounds come from the same global listener used everywhere else.
 */
export default function TestMode({ snap, update, navigate }: PageProps) {
  const area = useRef<HTMLTextAreaElement>(null);
  const { settings, keyboard } = snap;

  return (
    <main className="page">
      <header className="header">
        <button type="button" className="link back" onClick={() => navigate('home')}>
          ← Back
        </button>
        <span className="title">Test</span>
        <PowerToggle on={settings.enabled} onChange={(enabled) => update({ enabled })} />
      </header>

      <label className="test-label" htmlFor="test-area">
        Type here to test KeyClick:
      </label>
      <textarea
        id="test-area"
        ref={area}
        className="test-area"
        placeholder="Start typing..."
        autoFocus
        spellCheck={false}
        autoComplete="off"
        autoCorrect="off"
        autoCapitalize="off"
      />
      <div className="test-footer">
        <span className="muted">
          {!settings.enabled
            ? 'KeyClick is off — turn it on to hear sounds.'
            : keyboard.active
              ? 'Nothing typed here is saved.'
              : 'Keyboard listener is not running.'}
        </span>
        <button
          type="button"
          className="btn small ghost"
          onClick={() => {
            if (area.current) area.current.value = '';
            area.current?.focus();
          }}
        >
          Clear
        </button>
      </div>
    </main>
  );
}
