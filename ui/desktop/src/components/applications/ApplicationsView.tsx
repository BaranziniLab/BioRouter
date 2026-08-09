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
import { SearchView } from '../conversation/SearchView';
import { getSearchShortcutText } from '../../utils/keyboardShortcuts';
import { toastSuccess, toastError } from '../../toasts';
import { ReadableContent } from '../Layout/ReadableContent';
import {
  appUrl,
  buildExportUrl,
  configuredBaseUrl,
  deleteAgentDrafterApp,
  requireOk,
  secretHeader,
} from './appManagement';
import type { AppManifest, ExportOptions } from './appManagement';
import ExportAppDialog from './ExportAppDialog';

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
  const [launchingAppId, setLaunchingAppId] = useState<string | null>(null);
  const [exportingAppId, setExportingAppId] = useState<string | null>(null);
  const [appToExport, setAppToExport] = useState<AppManifest | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const res = await fetch(`${configuredBaseUrl()}/apps`, { headers: await secretHeader() });
      await requireOk(res);
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

  // Launch opens the app's own served URL in the real browser — archetype-
  // agnostic by construction (nothing here assumes a chat-shaped app), so the
  // v2 non-chat starters need no special affordance (plan Phase 5 item 5).
  const launch = async (app: AppManifest) => {
    if (launchingAppId === app.id) return;
    const baseUrl = configuredBaseUrl();
    if (!baseUrl) {
      setError('Backend URL unavailable. Is biorouterd running?');
      return;
    }
    setLaunchingAppId(app.id);
    try {
      await window.electron.openExternal(appUrl(app.id, baseUrl));
    } catch (err) {
      console.error('Failed to open app:', err);
      toastError({ title: app.title, msg: 'Could not open the app in your browser.' });
    } finally {
      setLaunchingAppId((current) => (current === app.id ? null : current));
    }
  };

  const openConversation = (app: AppManifest) => {
    if (!app.session_id) return;
    // Reopen the chat this app was built in so the user can keep iterating.
    navigate(`/pair?resumeSessionId=${encodeURIComponent(app.session_id)}`, {
      state: { resumeSessionId: app.session_id },
    });
  };

  const exportApp = async (app: AppManifest, options: ExportOptions) => {
    if (exportingAppId === app.id) return;
    setExportingAppId(app.id);
    try {
      // Older daemons ignore the query params and return the same scaffold
      // map, so this degrades gracefully to a launcher export.
      const res = await fetch(buildExportUrl(configuredBaseUrl(), app.id, options), {
        headers: await secretHeader(),
      });
      await requireOk(res);
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
    } finally {
      setExportingAppId((current) => (current === app.id ? null : current));
    }
  };

  const confirmDelete = async () => {
    if (!appToDelete) return;
    const app = appToDelete;
    setIsDeleting(true);
    try {
      await deleteAgentDrafterApp(app.id);
      setApps((prev) => prev.filter((a) => a.id !== app.id));
      toastSuccess({ title: app.title, msg: 'Application deleted' });
    } catch (err) {
      console.error('Failed to delete app:', err);
      toastError({
        title: app.title,
        msg:
          err instanceof Error
            ? `Could not delete the app: ${err.message}`
            : 'Could not delete the app.',
      });
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
        <div className="flex-shrink-0 border-b border-border-subtle">
          <ReadableContent className="px-8 pt-12 pb-6">
            <div className="flex flex-col page-transition">
              <h1 className="text-title mb-1">Applications</h1>
              <p className="text-body text-text-muted mb-0">
                Apps you built with Agent Drafter. Each one runs a full Biorouter agent with its own
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
          </ReadableContent>
        </div>

        {/* List */}
        <SearchView
          onSearch={(term, _caseSensitive) => setSearchTerm(term)}
          placeholder="Search applications..."
        >
          <ReadableContent className="px-8 py-4">
            {loading ? (
              <p className="text-body text-text-muted mt-10 text-center">Loading applications…</p>
            ) : error && apps.length === 0 ? (
              <div className="flex flex-col items-center justify-center mt-16 text-center">
                <p className="text-text-danger mb-4">Could not load applications: {error}</p>
                <Button onClick={load}>Retry</Button>
              </div>
            ) : filtered.length === 0 ? (
              <div className="text-center mt-16 max-w-md mx-auto">
                <h3 className="text-subheading text-text-default mb-1">
                  {searchTerm ? 'No matching applications' : 'No applications yet'}
                </h3>
                <p className="text-body text-text-muted">
                  {searchTerm
                    ? 'No applications match your search.'
                    : 'Ask Biorouter to build one, for example "use Agent Drafter to build a dashboard app". It will appear here.'}
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
                    onExport={() => setAppToExport(app)}
                    onDelete={() => setAppToDelete(app)}
                    isLaunching={launchingAppId === app.id}
                    isExporting={exportingAppId === app.id}
                  />
                ))}
              </div>
            )}
          </ReadableContent>
        </SearchView>
      </div>

      {appToExport && (
        <ExportAppDialog
          key={appToExport.id}
          app={appToExport}
          onCancel={() => setAppToExport(null)}
          onConfirm={(options) => {
            const app = appToExport;
            setAppToExport(null);
            void exportApp(app, options);
          }}
        />
      )}

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
  isLaunching?: boolean;
  isExporting?: boolean;
}

export function ApplicationItem({
  app,
  onLaunch,
  onOpenConversation,
  onExport,
  onDelete,
  isLaunching = false,
  isExporting = false,
}: ApplicationItemProps) {
  const model = app.agent?.model?.model;
  const kb = app.agent?.knowledge_base;
  const actionCount = app.surface?.actions?.length ?? 0;
  const signalCount = app.surface?.signals?.length ?? 0;
  const plural = (n: number, noun: string) => `${n} ${noun}${n === 1 ? '' : 's'}`;
  const surfaceSummary = [
    ...(actionCount > 0 ? [plural(actionCount, 'action')] : []),
    ...(signalCount > 0 ? [plural(signalCount, 'signal')] : []),
  ].join(' · ');
  return (
    <div
      className="biorouter-list-row flex items-start py-3 px-3 group gap-3"
      aria-busy={isLaunching || isExporting}
    >
      <div className="flex-1 min-w-0">
        <div className="flex items-center gap-1.5">
          <p className="text-label text-text-default truncate">{app.title}</p>
          <span className="text-supporting px-1.5 py-0.5 rounded-inner bg-background-medium text-text-muted flex-shrink-0">
            {app.kind}
          </span>
          {surfaceSummary && (
            <span
              className="text-supporting px-1.5 py-0.5 rounded-inner bg-background-medium text-text-muted flex-shrink-0"
              title="Declared app surface: verbs the agent can call and signals it can subscribe to"
            >
              {surfaceSummary}
            </span>
          )}
        </div>
        {app.description && (
          <p className="text-supporting text-text-muted mt-0.5 line-clamp-1">{app.description}</p>
        )}
        <div className="flex items-center gap-3 mt-1 text-supporting text-text-subtle flex-wrap">
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
      <div className="flex items-center gap-1 flex-shrink-0 opacity-100 transition-opacity sm:opacity-0 sm:group-hover:opacity-100 sm:group-focus-within:opacity-100">
        <Button
          onClick={onLaunch}
          size="sm"
          className="h-7 w-7 p-0"
          title="Launch in browser"
          aria-label={`Launch ${app.title} in browser`}
          disabled={isLaunching}
        >
          <Play className="w-4 h-4" />
        </Button>
        {app.session_id && (
          <Button
            onClick={onOpenConversation}
            variant="outline"
            size="sm"
            className="h-7 w-7 p-0"
            title="Open the conversation where this app was built"
            aria-label={`Open the conversation where ${app.title} was built`}
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
          aria-label={`Export ${app.title} to a folder`}
          disabled={isExporting}
        >
          <Download className="w-4 h-4" />
        </Button>
        <Button
          onClick={onDelete}
          variant="ghost"
          size="sm"
          className="h-7 w-7 p-0 text-text-danger hover:bg-background-danger/10"
          title="Delete this app"
          aria-label={`Delete ${app.title}`}
        >
          <Trash2 className="w-4 h-4" />
        </Button>
      </div>
    </div>
  );
}
