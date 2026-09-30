import { useCallback, useEffect, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import { api, errorText, NAVIGATE_EVENT, STATE_EVENT, type Settings, type Snapshot } from './api';
import Onboarding from './pages/Onboarding';
import Home from './pages/Home';
import SettingsPage from './pages/Settings';
import TestMode from './pages/TestMode';

export type Page = 'home' | 'settings' | 'test';

export interface PageProps {
  snap: Snapshot;
  update: (patch: Partial<Settings>) => Promise<void>;
  setSnap: (snap: Snapshot) => void;
  navigate: (page: Page) => void;
  report: (e: unknown) => void;
}

const PAGES: Page[] = ['home', 'settings', 'test'];

export default function App() {
  const [snap, setSnap] = useState<Snapshot | null>(null);
  const [page, setPage] = useState<Page>('home');
  const [error, setError] = useState<string | null>(null);

  const report = useCallback((e: unknown) => setError(errorText(e)), []);
  const refresh = useCallback(() => {
    api.getState().then(setSnap).catch(report);
  }, [report]);

  useEffect(() => {
    refresh();
    const subscriptions = [
      listen(STATE_EVENT, refresh),
      listen<string>(NAVIGATE_EVENT, (e) => {
        if (PAGES.includes(e.payload as Page)) setPage(e.payload as Page);
      }),
    ];
    return () => subscriptions.forEach((s) => s.then((unlisten) => unlisten()));
  }, [refresh]);

  useEffect(() => {
    if (!error) return;
    const t = window.setTimeout(() => setError(null), 5000);
    return () => window.clearTimeout(t);
  }, [error]);

  const update = useCallback(
    async (patch: Partial<Settings>) => {
      try {
        setSnap(await api.update(patch));
      } catch (e) {
        report(e);
        refresh();
      }
    },
    [refresh, report],
  );

  if (!snap) return <div className="app loading" />;

  const props: PageProps = { snap, update, setSnap, navigate: setPage, report };
  let content;
  if (!snap.settings.onboarding_complete) content = <Onboarding {...props} />;
  else if (page === 'settings') content = <SettingsPage {...props} />;
  else if (page === 'test') content = <TestMode {...props} />;
  else content = <Home {...props} />;

  return (
    <div className="app">
      {content}
      {error && (
        <div className="toast" role="alert" onClick={() => setError(null)}>
          {error}
        </div>
      )}
    </div>
  );
}
