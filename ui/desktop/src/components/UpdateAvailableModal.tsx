import { useEffect, useState } from 'react';
import { Button } from './ui/button';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from './ui/dialog';
import { Download, ExternalLink } from './icons/app-icons';
import { UPDATES_ENABLED } from '../updates';

const GITHUB_API_URL = 'https://api.github.com/repos/BaranziniLab/BioRouter/releases/latest';
const GITHUB_RELEASES_URL = 'https://github.com/BaranziniLab/BioRouter/releases/latest';
const BIOROUTER_DOWNLOAD_URL = 'https://baranzinilab.github.io/biorouter-landing/download.html';
const DISMISS_STORAGE_KEY = 'biorouter:update-modal-dismissed-version';
const STARTUP_DELAY_MS = 3000;
const RELEASE_NOTES_MAX_LEN = 600;

function normalizeVersion(v: string): string {
  return v.replace(/^v/, '').trim();
}

function isNewerVersion(latest: string, current: string): boolean {
  const parts = (v: string) =>
    normalizeVersion(v)
      .split('.')
      .map((p) => parseInt(p, 10))
      .map((n) => (Number.isFinite(n) ? n : 0));
  const lat = parts(latest);
  const cur = parts(current);
  const len = Math.max(lat.length, cur.length);
  for (let i = 0; i < len; i++) {
    const l = lat[i] ?? 0;
    const c = cur[i] ?? 0;
    if (l > c) return true;
    if (l < c) return false;
  }
  return false;
}

interface ReleaseInfo {
  currentVersion: string;
  latestVersion: string;
  releaseUrl: string;
  releaseNotes: string | null;
}

export default function UpdateAvailableModal() {
  const [open, setOpen] = useState(false);
  const [release, setRelease] = useState<ReleaseInfo | null>(null);

  useEffect(() => {
    if (!UPDATES_ENABLED) return;

    let cancelled = false;
    const timer = setTimeout(async () => {
      try {
        const currentVersion = normalizeVersion(window.electron?.getVersion?.() || '');
        // Skip when no version known (e.g. dev launch before env wires up)
        if (!currentVersion || currentVersion === '0.0.0') return;

        const response = await fetch(GITHUB_API_URL, {
          headers: { Accept: 'application/vnd.github+json' },
        });
        if (!response.ok) return;

        const data = (await response.json()) as {
          tag_name?: string;
          html_url?: string;
          body?: string;
        };
        const latestVersion = normalizeVersion(data.tag_name || '');
        if (!latestVersion) return;

        if (!isNewerVersion(latestVersion, currentVersion)) return;

        // Respect per-version dismissal
        const dismissed = localStorage.getItem(DISMISS_STORAGE_KEY);
        if (dismissed === latestVersion) return;

        if (cancelled) return;

        const releaseNotes = (data.body || '').slice(0, RELEASE_NOTES_MAX_LEN) || null;
        setRelease({
          currentVersion,
          latestVersion,
          releaseUrl: data.html_url || GITHUB_RELEASES_URL,
          releaseNotes,
        });
        setOpen(true);
      } catch {
        // Network/offline: silently skip — matches bioscratch behavior
      }
    }, STARTUP_DELAY_MS);

    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, []);

  const handleNotNow = () => {
    if (release) {
      // Remember dismissal so we don't nag for the same version
      localStorage.setItem(DISMISS_STORAGE_KEY, release.latestVersion);
    }
    setOpen(false);
  };

  const handleDownload = () => {
    if (!release) return;
    localStorage.setItem(DISMISS_STORAGE_KEY, release.latestVersion);
    if (window.electron?.openExternal) {
      window.electron.openExternal(BIOROUTER_DOWNLOAD_URL);
    } else {
      window.open(BIOROUTER_DOWNLOAD_URL, '_blank');
    }
    setOpen(false);
  };

  if (!release) return null;

  return (
    <Dialog open={open} onOpenChange={(o) => (!o ? handleNotNow() : setOpen(true))}>
      <DialogContent className="sm:max-w-[460px]">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Download className="w-5 h-5 text-background-accent" />
            New version available
          </DialogTitle>
        </DialogHeader>

        <div className="py-2 space-y-3">
          <p className="text-sm text-text-default">
            A newer version of BioRouter is available. Download and install the latest release to
            get the newest features and fixes.
          </p>

          <div className="bg-background-muted rounded-lg px-3 py-2 space-y-1">
            <div className="flex items-center justify-between">
              <span className="text-xs text-text-muted">Current</span>
              <span className="text-sm font-mono text-text-default">{release.currentVersion}</span>
            </div>
            <div className="flex items-center justify-between">
              <span className="text-xs text-text-muted">Latest</span>
              <span className="text-sm font-mono font-semibold text-text-default">
                {release.latestVersion}
              </span>
            </div>
          </div>

          {release.releaseNotes && (
            <div className="bg-background-muted border border-border-default rounded-md px-3 py-2 max-h-32 overflow-y-auto text-xs text-text-muted whitespace-pre-wrap leading-relaxed">
              {release.releaseNotes}
            </div>
          )}

          <p className="text-xs text-text-muted">
            Click <strong>Download Latest</strong> to open the BioRouter download page, grab the
            installer for your platform, and follow the instructions to update.
          </p>
        </div>

        <DialogFooter className="flex-wrap gap-2">
          <Button variant="outline" size="sm" onClick={handleNotNow}>
            Not Now
          </Button>
          <Button
            variant="secondary"
            size="sm"
            onClick={() => {
              if (window.electron?.openExternal) {
                window.electron.openExternal(release.releaseUrl);
              } else {
                window.open(release.releaseUrl, '_blank');
              }
            }}
            className="flex items-center gap-2"
          >
            <ExternalLink className="w-4 h-4" />
            View on GitHub
          </Button>
          <Button
            variant="default"
            size="sm"
            onClick={handleDownload}
            className="flex items-center gap-2"
          >
            <Download className="w-4 h-4" />
            Download Latest
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
