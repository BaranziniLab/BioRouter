import { useCallback, useEffect, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { MainPanelLayout } from '../Layout/MainPanelLayout';
import { Button } from '../ui/button';
import {
  Play,
  Trash2,
  RefreshCw,
  Download,
  MessageSquare,
  Calendar,
  Clock,
} from '../icons/app-icons';
import { ConfirmationModal } from '../ui/ConfirmationModal';
import { client } from '../../api/client.gen';
import { SearchView } from '../conversation/SearchView';
import { getSearchShortcutText } from '../../utils/keyboardShortcuts';
import { toastSuccess, toastError } from '../../toasts';

/** An Agent-Drafter-built app, as returned by biorouterd `GET /apps`. */
interface AppManifest {
  id: string;
  title: string;
  description?: string;
  kind: 'static' | 'agentic';
  created_at?: number;
  updated_at?: number;
  built_at?: number | null;
  /** Chat session this app was built in, when known (lets us reopen it). */
  session_id?: string;
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

/** Format a Unix-seconds timestamp as a short, readable date (e.g. "Jun 24, 2026"). */
function formatDate(secs?: number | null): string {
  if (!secs) return 'unknown';
  return new Date(secs * 1000).toLocaleDateString(undefined, {
    year: 'numeric',
    month: 'short',
    day: 'numeric',
  });
}

export default function ApplicationsView() {
  const navigate = useNavigate();
  const [apps, setApps] = useState<AppManifest[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [searchTerm, setSearchTerm] = useState('');
  const [appToDelete, setAppToDelete] = useState<AppManifest | null>(null);
  const [isDeleting, setIsDeleting] = useState(false);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const res = await fetch(`${baseUrl()}/apps`, { headers: secretHeader() });
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      const data: AppManifest[] = await res.json();
      // Most recently updated first.
      data.sort((a, b) => (b.updated_at ?? 0) - (a.updated_at ?? 0));
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
      setError('Backend URL unavailable. Is biorouterd running?');
      return;
    }
    try {
      await window.electron.openExternal(appUrl(app.id));
    } catch (err) {
      console.error('Failed to open app:', err);
      toastError({ title: app.title, msg: 'Could not open the app in your browser.' });
    }
  };

  const openConversation = (app: AppManifest) => {
    if (!app.session_id) return;
    // Reopen the chat this app was built in so the user can keep iterating.
    navigate(`/pair?resumeSessionId=${encodeURIComponent(app.session_id)}`, {
      state: { resumeSessionId: app.session_id },
    });
  };

  const exportApp = async (app: AppManifest) => {
    try {
      const res = await fetch(`${baseUrl()}/apps/${encodeURIComponent(app.id)}/export`, {
        headers: secretHeader(),
      });
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      const payload: { files?: Record<string, string> } = await res.json();
      const files = payload.files ?? {};
      // A real directory picker (openDirectory) so Export works the same on
      // macOS, Windows, and Linux — unlike selectFileOrDirectory, which only
      // offers directory selection on macOS.
      const picked = await window.electron.directoryChooser();
      if (picked.canceled || picked.filePaths.length === 0) return;
      const targetDir = `${picked.filePaths[0]}/${app.id}`;
      let written = 0;
      for (const [rel, content] of Object.entries(files)) {
        const ok = await window.electron.writeFile(`${targetDir}/${rel}`, content);
        if (ok) written += 1;
      }
      if (written > 0) {
        toastSuccess({ title: app.title, msg: `Exported ${written} files to ${targetDir}` });
      } else {
        toastError({ title: app.title, msg: 'Nothing was exported.' });
      }
    } catch (err) {
      console.error('Failed to export app:', err);
      toastError({ title: app.title, msg: 'Could not export the app.' });
    }
  };

  const confirmDelete = async () => {
    if (!appToDelete) return;
    const app = appToDelete;
    setIsDeleting(true);
    try {
      const res = await fetch(`${baseUrl()}/apps/${encodeURIComponent(app.id)}`, {
        method: 'DELETE',
        headers: secretHeader(),
      });
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      setApps((prev) => prev.filter((a) => a.id !== app.id));
      toastSuccess({ title: app.title, msg: 'Application deleted' });
    } catch (err) {
      console.error('Failed to delete app:', err);
      toastError({ title: app.title, msg: 'Could not delete the app.' });
    } finally {
      setIsDeleting(false);
      setAppToDelete(null);
    }
  };

  const filtered = apps.filter((app) => {
    if (!searchTerm) return true;
    const q = searchTerm.toLowerCase();
    return app.title.toLowerCase().includes(q) || (app.description ?? '').toLowerCase().includes(q);
  });

  return (
    <MainPanelLayout>
      <div
        className="flex flex-col min-w-0 flex-1 overflow-y-auto relative"
        data-search-scroll-area
      >
        {/* Header */}
        <div className="biorouter-page-header px-8 pt-12 pb-6 flex-shrink-0 border-b border-border-subtle">
          <div className="flex flex-col page-transition">
            <h1 className="text-2xl font-semibold tracking-tight mb-1">Applications</h1>
            <p className="text-sm text-text-muted mb-0">
              Apps you built with Agent Drafter. Each one runs a full BioRouter agent with its own
              model, extensions, skills, and knowledge, and opens in your browser.{' '}
              {getSearchShortcutText()} to search.
            </p>
          </div>
          <div className="flex gap-3 mt-5">
            <Button variant="outline" className="flex items-center gap-2" onClick={load}>
              <RefreshCw className="h-4 w-4" />
              Refresh
            </Button>
          </div>
        </div>

        {/* List */}
        <SearchView
          onSearch={(term, _caseSensitive) => setSearchTerm(term)}
          placeholder="Search applications..."
        >
          <div className="px-6 py-4">
            {loading ? (
              <p className="text-sm text-text-muted mt-10 text-center">Loading applications…</p>
            ) : error && apps.length === 0 ? (
              <div className="flex flex-col items-center justify-center mt-16 text-center">
                <p className="text-text-danger mb-4">Could not load applications: {error}</p>
                <Button onClick={load}>Retry</Button>
              </div>
            ) : filtered.length === 0 ? (
              <div className="text-center mt-16 max-w-md mx-auto">
                <h3 className="text-base font-medium text-text-default mb-1">
                  {searchTerm ? 'No matching applications' : 'No applications yet'}
                </h3>
                <p className="text-sm text-text-muted">
                  {searchTerm
                    ? 'No applications match your search.'
                    : 'Ask BioRouter to build one. For example: "Use Agent Drafter to build a SPOKE dashboard app." It will show up here, ready to launch.'}
                </p>
              </div>
            ) : (
              <div className="biorouter-list-shell">
                {filtered.map((app) => (
                  <ApplicationItem
                    key={app.id}
                    app={app}
                    onLaunch={() => launch(app)}
                    onOpenConversation={() => openConversation(app)}
                    onExport={() => exportApp(app)}
                    onDelete={() => setAppToDelete(app)}
                  />
                ))}
              </div>
            )}
          </div>
        </SearchView>
      </div>

      <ConfirmationModal
        isOpen={appToDelete !== null}
        title={`Delete "${appToDelete?.title}"?`}
        message="This permanently removes the application and its files from disk. This action cannot be undone."
        confirmLabel="Delete"
        cancelLabel="Cancel"
        confirmVariant="destructive"
        isSubmitting={isDeleting}
        onConfirm={confirmDelete}
        onCancel={() => setAppToDelete(null)}
      />
    </MainPanelLayout>
  );
}

// ---------------------------------------------------------------------------
// Inline application row component
// ---------------------------------------------------------------------------
interface ApplicationItemProps {
  app: AppManifest;
  onLaunch: () => void;
  onOpenConversation: () => void;
  onExport: () => void;
  onDelete: () => void;
}

function ApplicationItem({
  app,
  onLaunch,
  onOpenConversation,
  onExport,
  onDelete,
}: ApplicationItemProps) {
  const model = app.agent?.model?.model;
  const kb = app.agent?.knowledge_base;
  return (
    <div className="biorouter-list-row flex items-start py-3 px-3 group gap-3">
      <div className="flex-1 min-w-0">
        <div className="flex items-center gap-1.5">
          <p className="text-sm text-text-default truncate">{app.title}</p>
          <span className="text-[11px] px-1.5 py-0.5 rounded bg-background-medium text-text-muted flex-shrink-0">
            {app.kind}
          </span>
        </div>
        {app.description && (
          <p className="text-xs text-text-muted mt-0.5 line-clamp-1">{app.description}</p>
        )}
        <div className="flex items-center gap-3 mt-1 text-[11px] text-text-subtle flex-wrap">
          <span className="flex items-center">
            <Calendar className="w-3 h-3 mr-1" />
            Created {formatDate(app.created_at)}
          </span>
          <span className="flex items-center">
            <Clock className="w-3 h-3 mr-1" />
            Updated {formatDate(app.updated_at)}
          </span>
          {model && <span className="font-mono">{model}</span>}
          {kb && <span className="font-mono">KB: {kb}</span>}
        </div>
      </div>
      <div className="flex items-center gap-1 flex-shrink-0 opacity-0 group-hover:opacity-100 transition-opacity">
        <Button onClick={onLaunch} size="sm" className="h-7 w-7 p-0" title="Launch in browser">
          <Play className="w-4 h-4" />
        </Button>
        {app.session_id && (
          <Button
            onClick={onOpenConversation}
            variant="outline"
            size="sm"
            className="h-7 w-7 p-0"
            title="Open the conversation where this app was built"
          >
            <MessageSquare className="w-4 h-4" />
          </Button>
        )}
        <Button
          onClick={onExport}
          variant="outline"
          size="sm"
          className="h-7 w-7 p-0"
          title="Export to a folder"
        >
          <Download className="w-4 h-4" />
        </Button>
        <Button
          onClick={onDelete}
          variant="ghost"
          size="sm"
          className="h-7 w-7 p-0 text-text-danger hover:bg-background-danger/10"
          title="Delete"
        >
          <Trash2 className="w-4 h-4" />
        </Button>
      </div>
    </div>
  );
}
