import { useState, useEffect } from 'react';
import { Button } from '../../ui/button';
import { ExternalLink, CheckCircle, Download, AlertCircle, Loader2, Rocket } from '../../icons/app-icons';
import {
  initialUpdaterState,
  reduceUpdaterEvent,
  type UpdaterState,
} from '../../../utils/updaterState';

// Always point users at the official Biorouter download website (the same place
// the startup update flow directs to) rather than the raw GitHub releases page.
const DOWNLOAD_WEBSITE_URL = 'https://biorouter.ucsf.edu/download';

/**
 * Settings → "Check for Updates".
 *
 * Drives the same `electron-updater` pipeline as the startup modal: pressing
 * "Check for Updates" asks the main process to check GitHub; if a newer release
 * exists it downloads in the background and this panel shows progress, then a
 * one-click **Restart & Update** button. No manual DMG/drag-and-drop.
 */
export default function UpdateSection() {
  const [currentVersion, setCurrentVersion] = useState('');
  const [state, setState] = useState<UpdaterState>(initialUpdaterState);
  const [checkRequested, setCheckRequested] = useState(false);

  useEffect(() => {
    setCurrentVersion(window.electron.getVersion());

    if (!window.electron?.onUpdaterEvent) return;
    const dispose = window.electron.onUpdaterEvent((payload) => {
      setState((prev) => reduceUpdaterEvent(prev, payload));
    });

    // Recover any in-flight/ready update established before this panel opened.
    window.electron
      .getUpdateState?.()
      ?.then((snapshot) => {
        if (!snapshot) return;
        if (snapshot.status === 'downloaded' || snapshot.updateAvailable) {
          setState((prev) =>
            reduceUpdaterEvent(prev, {
              event: snapshot.status === 'downloaded' ? 'update-downloaded' : 'update-available',
              data: { version: snapshot.latestVersion, percent: snapshot.percent },
            })
          );
        }
      })
      .catch(() => {});

    return () => dispose?.();
  }, []);

  const checkForUpdates = async () => {
    setCheckRequested(true);
    setState((prev) => reduceUpdaterEvent(prev, { event: 'checking-for-update' }));
    try {
      const res = await window.electron.checkForUpdates();
      if (res?.error) {
        setState((prev) => reduceUpdaterEvent(prev, { event: 'error', data: res.error }));
      }
      // Success path: the main process emits update-available / -not-available
      // (and download-progress / -downloaded) through onUpdaterEvent.
    } catch (err) {
      setState((prev) =>
        reduceUpdaterEvent(prev, {
          event: 'error',
          data: err instanceof Error ? err.message : 'Unknown error',
        })
      );
    }
  };

  const handleRestartAndUpdate = () => window.electron?.installUpdate?.();

  const { phase } = state;
  const busy = phase === 'checking' || phase === 'available';

  return (
    <div>
      <div className="text-sm text-text-muted mb-4">
        <div className="flex flex-col">
          <div className="text-text-default text-2xl font-mono">
            {currentVersion || 'Loading...'}
          </div>
          <div className="text-xs text-text-muted">Current version</div>
        </div>
      </div>

      <div className="flex items-center gap-3 flex-wrap">
        {phase === 'downloaded' ? (
          <Button
            onClick={handleRestartAndUpdate}
            variant="default"
            size="sm"
            className="flex items-center gap-2"
          >
            <Rocket className="w-4 h-4" />
            Restart &amp; Update{state.latestVersion ? ` to ${state.latestVersion}` : ''}
          </Button>
        ) : (
          <Button
            onClick={checkForUpdates}
            disabled={busy}
            variant="secondary"
            size="sm"
            className="flex items-center gap-2"
          >
            {busy ? (
              <Loader2 className="w-4 h-4 animate-spin" />
            ) : (
              <ExternalLink className="w-4 h-4" />
            )}
            Check for Updates
          </Button>
        )}
        <p className="text-xs text-text-muted">
          Updates install automatically — one click restarts BioRouter on the new version.
        </p>
      </div>

      {/* Status line */}
      {checkRequested && (
        <div className="mt-3 text-sm">
          {phase === 'checking' && (
            <div className="flex items-center gap-2 text-text-muted">
              <Loader2 className="w-4 h-4 animate-spin" />
              Checking for updates…
            </div>
          )}

          {phase === 'up-to-date' && (
            <div className="flex items-center gap-2 text-text-success">
              <CheckCircle className="w-4 h-4" />
              BioRouter is up to date.
            </div>
          )}

          {phase === 'available' && (
            <div className="space-y-2 max-w-sm">
              <div className="flex items-center gap-2 text-text-default">
                <Download className="w-4 h-4 text-background-accent" />
                Downloading {state.latestVersion ?? 'update'}…
              </div>
              <div className="h-2 w-full overflow-hidden rounded-full bg-background-muted">
                <div
                  className="h-full rounded-full bg-background-accent transition-all duration-300"
                  style={{ width: `${Math.max(4, state.percent)}%` }}
                />
              </div>
              <p className="text-xs text-text-muted text-right font-mono">{state.percent}%</p>
            </div>
          )}

          {phase === 'downloaded' && (
            <div className="flex items-center gap-2 text-text-default">
              <CheckCircle className="w-4 h-4 text-text-success" />
              Version {state.latestVersion} is ready — click Restart &amp; Update.
            </div>
          )}

          {phase === 'error' && (
            <div className="space-y-2">
              <div className="flex items-center gap-2 text-text-danger">
                <AlertCircle className="w-4 h-4" />
                Could not complete the update.
              </div>
              {state.error && (
                <p className="text-xs font-mono text-text-muted bg-background-muted rounded px-2 py-1">
                  {state.error}
                </p>
              )}
              <Button
                variant="secondary"
                size="sm"
                onClick={() => window.open(DOWNLOAD_WEBSITE_URL, '_blank')}
                className="flex items-center gap-2"
              >
                <ExternalLink className="w-4 h-4" />
                Download from Biorouter
              </Button>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
