import { api, type Snapshot } from '../api';

interface StatusBannerProps {
  snap: Snapshot;
  onFixPermission: () => void;
  report: (e: unknown) => void;
}

/** Shows only when something needs the user's attention. */
export function StatusBanner({ snap, onFixPermission, report }: StatusBannerProps) {
  const { settings, keyboard, audio, platform } = snap;
  if (settings.enabled && !keyboard.active) {
    if (keyboard.permission === 'denied') {
      return (
        <div className="banner warn">
          <strong>Keyboard access required</strong>
          <span>
            {platform === 'macos'
              ? 'Allow KeyClick in System Settings → Privacy & Security → Input Monitoring.'
              : 'KeyClick could not start its keyboard listener.'}
          </span>
          <button type="button" className="btn small" onClick={onFixPermission}>
            Fix
          </button>
        </div>
      );
    }
    return (
      <div className="banner warn">
        <strong>Keyboard listener stopped</strong>
        <span>{keyboard.error ?? 'KeyClick is not detecting key presses.'}</span>
        <button type="button" className="btn small" onClick={() => api.retryKeyboard().catch(report)}>
          Retry
        </button>
      </div>
    );
  }
  if (audio.error) {
    return (
      <div className="banner">
        <strong>No sound output</strong>
        <span>{audio.error} KeyClick will reconnect automatically.</span>
      </div>
    );
  }
  return null;
}

export function PrivacyNote() {
  return (
    <div className="privacy">
      <h3>Privacy First</h3>
      <p>KeyClick does not record what you type.</p>
      <p>
        Keyboard events are processed locally only to trigger sounds and are immediately discarded.
      </p>
      <ul>
        <li>No typed text is stored.</li>
        <li>No keyboard data is uploaded.</li>
      </ul>
    </div>
  );
}
