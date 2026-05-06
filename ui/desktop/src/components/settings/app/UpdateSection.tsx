import { useState, useEffect } from 'react';
import { Button } from '../../ui/button';
import { ExternalLink, CheckCircle, Download, AlertCircle, Loader2 } from '../../icons/app-icons';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from '../../ui/dialog';

const GITHUB_API_URL = 'https://api.github.com/repos/BaranziniLab/BioRouter/releases/latest';
const GITHUB_RELEASES_URL = 'https://github.com/BaranziniLab/BioRouter/releases/latest';

type UpdateStatus = 'idle' | 'checking' | 'up-to-date' | 'update-available' | 'error';

interface ReleaseInfo {
  version: string;
  url: string;
  name: string;
}

function normalizeVersion(v: string): string {
  return v.replace(/^v/, '').trim();
}

function isNewerVersion(latest: string, current: string): boolean {
  const latestParts = normalizeVersion(latest).split('.').map(Number);
  const currentParts = normalizeVersion(current).split('.').map(Number);
  const len = Math.max(latestParts.length, currentParts.length);
  for (let i = 0; i < len; i++) {
    const l = latestParts[i] ?? 0;
    const c = currentParts[i] ?? 0;
    if (l > c) return true;
    if (l < c) return false;
  }
  return false;
}

export default function UpdateSection() {
  const [currentVersion, setCurrentVersion] = useState('');
  const [status, setStatus] = useState<UpdateStatus>('idle');
  const [latestRelease, setLatestRelease] = useState<ReleaseInfo | null>(null);
  const [dialogOpen, setDialogOpen] = useState(false);
  const [errorMessage, setErrorMessage] = useState('');

  useEffect(() => {
    setCurrentVersion(window.electron.getVersion());
  }, []);

  const checkForUpdates = async () => {
    setStatus('checking');
    setDialogOpen(true);

    try {
      const response = await fetch(GITHUB_API_URL, {
        headers: { Accept: 'application/vnd.github+json' },
      });

      if (!response.ok) {
        throw new Error(`GitHub API returned ${response.status}`);
      }

      const data = await response.json();
      const latestVersion = data.tag_name as string;
      const releaseName = (data.name as string) || latestVersion;
      const releaseUrl = (data.html_url as string) || GITHUB_RELEASES_URL;

      setLatestRelease({ version: latestVersion, url: releaseUrl, name: releaseName });

      if (isNewerVersion(latestVersion, currentVersion)) {
        setStatus('update-available');
      } else {
        setStatus('up-to-date');
      }
    } catch (err) {
      console.error('Update check failed:', err);
      setErrorMessage(err instanceof Error ? err.message : 'Unknown error');
      setStatus('error');
    }
  };

  const handleClose = () => {
    setDialogOpen(false);
    setStatus('idle');
  };

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

      <div className="flex items-center gap-3">
        <Button
          onClick={checkForUpdates}
          disabled={status === 'checking'}
          variant="secondary"
          size="sm"
          className="flex items-center gap-2"
        >
          {status === 'checking' ? (
            <Loader2 className="w-4 h-4 animate-spin" />
          ) : (
            <ExternalLink className="w-4 h-4" />
          )}
          Check for Updates
        </Button>
        <p className="text-xs text-text-muted">Compare with the latest release on GitHub.</p>
      </div>

      <Dialog open={dialogOpen} onOpenChange={(open) => !open && handleClose()}>
        <DialogContent className="sm:max-w-[420px]">
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              {status === 'checking' && (
                <>
                  <Loader2 className="w-5 h-5 animate-spin text-text-muted" />
                  Checking for updates…
                </>
              )}
              {status === 'up-to-date' && (
                <>
                  <CheckCircle className="w-5 h-5 text-green-600 dark:text-green-400" />
                  You&apos;re up to date
                </>
              )}
              {status === 'update-available' && (
                <>
                  <Download className="w-5 h-5 text-background-accent" />
                  New version available
                </>
              )}
              {status === 'error' && (
                <>
                  <AlertCircle className="w-5 h-5 text-text-danger" />
                  Update check failed
                </>
              )}
            </DialogTitle>
          </DialogHeader>

          <div className="py-2 space-y-3">
            {status === 'checking' && (
              <p className="text-sm text-text-muted">
                Fetching latest release information from GitHub…
              </p>
            )}

            {status === 'up-to-date' && (
              <div className="space-y-2">
                <p className="text-sm text-text-default">
                  BioRouter is running the latest version.
                </p>
                <div className="flex items-center gap-2 bg-background-muted rounded-lg px-3 py-2">
                  <span className="text-xs text-text-muted">Version</span>
                  <span className="text-sm font-mono text-text-default ml-auto">
                    {normalizeVersion(currentVersion)}
                  </span>
                </div>
              </div>
            )}

            {status === 'update-available' && latestRelease && (
              <div className="space-y-3">
                <p className="text-sm text-text-default">
                  A newer version of BioRouter is available. Download and install the latest release
                  to get the newest features and fixes.
                </p>
                <div className="bg-background-muted rounded-lg px-3 py-2 space-y-1">
                  <div className="flex items-center justify-between">
                    <span className="text-xs text-text-muted">Current</span>
                    <span className="text-sm font-mono text-text-default">
                      {normalizeVersion(currentVersion)}
                    </span>
                  </div>
                  <div className="flex items-center justify-between">
                    <span className="text-xs text-text-muted">Latest</span>
                    <span className="text-sm font-mono font-semibold text-text-default">
                      {normalizeVersion(latestRelease.version)}
                    </span>
                  </div>
                </div>
                <p className="text-xs text-text-muted">
                  Download the <code className="font-mono">.dmg</code> file, open it, and drag
                  BioRouter to your Applications folder to update.
                </p>
              </div>
            )}

            {status === 'error' && (
              <div className="space-y-2">
                <p className="text-sm text-text-default">
                  Could not reach GitHub to check for updates. Please verify your internet
                  connection and try again.
                </p>
                {errorMessage && (
                  <p className="text-xs font-mono text-text-muted bg-background-muted rounded px-2 py-1">
                    {errorMessage}
                  </p>
                )}
              </div>
            )}
          </div>

          <DialogFooter>
            {status === 'update-available' && latestRelease && (
              <Button
                variant="default"
                size="sm"
                onClick={() => window.open(latestRelease.url, '_blank')}
                className="flex items-center gap-2"
              >
                <Download className="w-4 h-4" />
                Download Latest
              </Button>
            )}
            {status === 'error' && (
              <Button
                variant="secondary"
                size="sm"
                onClick={() => window.open(GITHUB_RELEASES_URL, '_blank')}
                className="flex items-center gap-2"
              >
                <ExternalLink className="w-4 h-4" />
                View on GitHub
              </Button>
            )}
            <Button variant="outline" size="sm" onClick={handleClose}>
              {status === 'up-to-date' ? 'Great!' : 'Close'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
