import { useEffect, useState } from 'react';
import type { PageProps } from '../App';
import { api } from '../api';
import { Logo } from '../components/Logo';

type Step = 'welcome' | 'permission' | 'ready';
const PERMISSION_POLL_MS = 1500;

export default function Onboarding({ snap, update, setSnap, navigate, report }: PageProps) {
  const [step, setStep] = useState<Step>('welcome');
  const mac = snap.platform === 'macos';
  const needsPermission = mac && !snap.keyboard.active;

  // While the permission step is visible, check whether access was granted.
  useEffect(() => {
    if (step !== 'permission') return;
    const t = window.setInterval(() => {
      api
        .retryKeyboard()
        .then((next) => {
          setSnap(next);
          if (next.keyboard.active) setStep('ready');
        })
        .catch(() => {});
    }, PERMISSION_POLL_MS);
    return () => window.clearInterval(t);
  }, [step, setSnap]);

  const openSettings = async () => {
    try {
      const next = await api.requestPermission();
      setSnap(next);
      if (next.keyboard.active) return setStep('ready');
      await api.openPermissionSettings();
    } catch (e) {
      report(e);
    }
  };

  const finish = async (then: 'home' | 'test') => {
    await update({ onboarding_complete: true });
    navigate(then);
  };

  if (step === 'welcome') {
    return (
      <main className="page onboarding">
        <Logo size={56} />
        <h1>Welcome to KeyClick</h1>
        <p className="tagline">Make typing sound satisfying.</p>
        <p>KeyClick works alongside your existing keyboard.</p>
        <p>
          It does not replace your keyboard
          <br />
          and does not record what you type.
        </p>
        <button type="button" className="btn primary" onClick={() => setStep(needsPermission ? 'permission' : 'ready')}>
          Continue
        </button>
      </main>
    );
  }

  if (step === 'permission') {
    const grantedButInactive = snap.keyboard.permission === 'granted' && !snap.keyboard.active;
    return (
      <main className="page onboarding">
        <div className="badge">🔒</div>
        <h1>{grantedButInactive ? 'Almost there.' : 'One permission is required.'}</h1>
        {grantedButInactive ? (
          <>
            <p>Access was granted. macOS needs KeyClick to restart to apply it.</p>
            <button type="button" className="btn primary" onClick={() => api.restartApp().catch(report)}>
              Restart KeyClick
            </button>
          </>
        ) : (
          <>
            <p>macOS requires permission for KeyClick to detect keyboard events.</p>
            <p>This is used only to trigger local sounds.</p>
            <p>
              <strong>Your typing is never recorded.</strong>
            </p>
            <ol className="steps">
              <li>Click “Open System Settings”.</li>
              <li>
                In <em>Privacy &amp; Security → Input Monitoring</em>, turn on <strong>KeyClick</strong>.
              </li>
              <li>Come back here — this screen updates automatically.</li>
            </ol>
            <button type="button" className="btn primary" onClick={openSettings}>
              Open System Settings
            </button>
          </>
        )}
        <button type="button" className="link" onClick={() => finish('home')}>
          Skip for now
        </button>
      </main>
    );
  }

  return (
    <main className="page onboarding">
      <div className="badge ok">✓</div>
      <h1>You're ready.</h1>
      {mac ? (
        <p>Start typing anywhere.</p>
      ) : (
        <>
          <p>KeyClick is running in the system tray.</p>
          <p>Open Notepad or your browser and start typing.</p>
        </>
      )}
      {!snap.keyboard.active && snap.settings.enabled && (
        <p className="warn-text">{snap.keyboard.error ?? 'The keyboard listener is not running yet.'}</p>
      )}
      <div className="actions">
        <button type="button" className="btn primary" onClick={() => finish('test')}>
          Test Keyboard
        </button>
        <button type="button" className="btn ghost" onClick={() => finish('home')}>
          Continue
        </button>
      </div>
    </main>
  );
}
