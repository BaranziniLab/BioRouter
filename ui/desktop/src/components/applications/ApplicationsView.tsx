import { useCallback, useEffect, useState } from 'react';
import { MainPanelLayout } from '../Layout/MainPanelLayout';
import { Button } from '../ui/button';
import { Play, Trash2, RefreshCw } from 'lucide-react';
import { client } from '../../api/client.gen';

/** An Agent-Drafter-built app, as returned by biorouterd `GET /apps`. */
interface AppManifest {
  id: string;
  title: string;
  description?: string;
  kind: 'static' | 'agentic';
  created_at?: number;
  updated_at?: number;
  built_at?: number | null;
  agent?: {
    model?: { provider?: string; model?: string };
    extensions?: string[];
    skills?: string[];
    knowledge_base?: string;
  } | null;
}

function baseUrl(): string {
  // The generated client is configured with the biorouterd origin.
  const cfg = client.getConfig();
  return ((cfg.baseUrl as string) || '').replace(/\/+$/, '');
}

/** App URL on the local daemon, with the id safely encoded. */
function appUrl(id: string): string {
  return `${baseUrl()}/apps/${encodeURIComponent(id)}/`;
}

function secretHeader(): Record<string, string> {
  const cfg = client.getConfig() as { headers?: Record<string, string> };
  const key = cfg.headers?.['X-Secret-Key'];
  return key ? { 'X-Secret-Key': key } : {};
}

const GridLayout = ({ children }: { children: React.ReactNode }) => (
  <div
    className="grid gap-4 p-1"
    style={{ gridTemplateColumns: 'repeat(auto-fill, minmax(300px, 1fr))' }}
  >
    {children}
  </div>
);

export default function ApplicationsView() {
  const [apps, setApps] = useState<AppManifest[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const res = await fetch(`${baseUrl()}/apps`, { headers: secretHeader() });
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      const data: AppManifest[] = await res.json();
      setApps(data);
      setError(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load applications');
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    load();
  }, [load]);

  const launch = async (app: AppManifest) => {
    if (!baseUrl()) {
      setError('Backend URL unavailable — is biorouterd running?');
      return;
    }
    try {
      await window.electron.openExternal(appUrl(app.id));
    } catch (err) {
      console.error('Failed to open app:', err);
      setError('Could not open the app in your browser.');
    }
  };

  const remove = async (app: AppManifest) => {
    try {
      const res = await fetch(`${baseUrl()}/apps/${encodeURIComponent(app.id)}`, {
        method: 'DELETE',
        headers: secretHeader(),
      });
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      setApps((prev) => prev.filter((a) => a.id !== app.id));
    } catch (err) {
      console.error('Failed to delete app:', err);
      setError('Could not delete the app.');
    }
  };

  return (
    <MainPanelLayout>
      <div className="flex-1 flex flex-col min-h-0">
        <div className="bg-background-default px-8 pb-8 pt-16">
          <div className="flex flex-col page-transition">
            <div className="flex justify-between items-center mb-1">
              <h1 className="text-2xl font-semibold tracking-tight">Applications</h1>
              <Button variant="ghost" size="sm" onClick={load} className="flex items-center gap-2">
                <RefreshCw className="h-4 w-4" />
                Refresh
              </Button>
            </div>
            <p className="text-sm text-text-muted mb-4">
              BioRouter apps you built with Agent Drafter. Each runs the full BioRouter agent —
              its own model, extensions, skills and knowledge — and opens in your browser.
            </p>
          </div>
        </div>

        <div className="flex-1 overflow-y-auto bg-background-muted px-8 pb-8">
          {loading ? (
            <div className="flex items-center justify-center h-64">
              <p className="text-text-muted">Loading applications…</p>
            </div>
          ) : error && apps.length === 0 ? (
            <div className="flex flex-col items-center justify-center h-64 text-center">
              <p className="text-text-danger mb-4">Error loading applications: {error}</p>
              <Button onClick={load}>Retry</Button>
            </div>
          ) : apps.length === 0 ? (
            <div className="flex items-center justify-center h-64">
              <div className="text-center max-w-md">
                <h3 className="text-lg font-medium mb-2">No applications yet</h3>
                <p className="text-sm text-text-muted">
                  Ask BioRouter to build one — e.g. “Use Agent Drafter to build a SPOKE dashboard
                  app.” It will appear here, ready to launch.
                </p>
              </div>
            </div>
          ) : (
            <GridLayout>
              {apps.map((app) => {
                const model = app.agent?.model?.model;
                return (
                  <div
                    key={app.id}
                    className="flex flex-col p-4 border border-border-subtle rounded-xl bg-background-default hover:border-border-strong hover:shadow-default transition-all"
                  >
                    <div className="flex-1 mb-4">
                      <h3 className="font-medium text-text-default mb-2">{app.title}</h3>
                      {app.description && (
                        <p className="text-sm text-text-muted mb-3 line-clamp-3">
                          {app.description}
                        </p>
                      )}
                      <div className="flex flex-wrap gap-1.5">
                        <span className="inline-block px-2 py-0.5 text-xs bg-background-medium text-text-muted rounded-md">
                          {app.kind}
                        </span>
                        {model && (
                          <span className="inline-block px-2 py-0.5 text-xs bg-background-medium text-text-muted rounded-md">
                            {model}
                          </span>
                        )}
                        {app.agent?.knowledge_base && (
                          <span className="inline-block px-2 py-0.5 text-xs bg-background-medium text-text-muted rounded-md">
                            KB: {app.agent.knowledge_base}
                          </span>
                        )}
                      </div>
                    </div>
                    <div className="flex gap-2">
                      <Button
                        variant="default"
                        size="sm"
                        onClick={() => launch(app)}
                        className="flex items-center gap-2 flex-1"
                      >
                        <Play className="h-4 w-4" />
                        Launch
                      </Button>
                      <Button
                        variant="ghost"
                        size="sm"
                        onClick={() => remove(app)}
                        className="flex items-center gap-2"
                        aria-label={`Delete ${app.title}`}
                      >
                        <Trash2 className="h-4 w-4" />
                      </Button>
                    </div>
                  </div>
                );
              })}
            </GridLayout>
          )}
        </div>
      </div>
    </MainPanelLayout>
  );
}
