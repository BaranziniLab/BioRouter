import { useEffect, useMemo, useState } from 'react';
import { Button } from '../ui/button';
import { Package } from '../icons/app-icons';
import { toastError } from '../../toasts';
import { BrxtInstallModal } from '../BrxtInstallModal';
import {
  loadRegistry,
  extensionMatches,
  effectivePrivacy,
  catalogFreshnessLine,
  type BaamRegistry,
  type RegistryExtension,
} from './registry';
import { Dialog, DialogContent, DialogTitle } from '../ui/dialog';
import { PrivacyBadge } from '../ui/PrivacyBadge';

interface Props {
  onClose: () => void;
  onInstalled: () => void;
  /** Lowercased names of extensions already configured. */
  installedNames: Set<string>;
}

export default function BrowseExtensionsModal({ onClose, onInstalled, installedNames }: Props) {
  const [registry, setRegistry] = useState<BaamRegistry | null>(null);
  const [live, setLive] = useState(false);
  const [fetchedAt, setFetchedAt] = useState<string | undefined>(undefined);
  const [loadError, setLoadError] = useState(false);
  const [search, setSearch] = useState('');
  const [downloadingId, setDownloadingId] = useState<string | null>(null);
  const [brxtPath, setBrxtPath] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    loadRegistry()
      .then(({ registry, live, fetchedAt }) => {
        if (cancelled) return;
        setRegistry(registry);
        setLive(live);
        setFetchedAt(fetchedAt);
      })
      .catch(() => !cancelled && setLoadError(true));
    return () => {
      cancelled = true;
    };
  }, []);

  const isInstalled = (e: RegistryExtension) =>
    installedNames.has(e.name.toLowerCase()) || installedNames.has(e.id.toLowerCase());

  const filtered = useMemo(() => {
    if (!registry) return [];
    return registry.extensions.filter((e) => extensionMatches(e, search));
  }, [registry, search]);

  const handleAdd = async (ext: RegistryExtension) => {
    if (downloadingId !== null) return;
    setDownloadingId(ext.id);
    try {
      const dl = await window.electron.downloadRegistryAsset(ext.download);
      if ('error' in dl) {
        toastError({ title: ext.name, msg: dl.error });
        return;
      }
      setBrxtPath(dl.path);
    } catch (error) {
      toastError({
        title: ext.name,
        msg: error instanceof Error ? error.message : 'Download failed',
      });
    } finally {
      setDownloadingId(null);
    }
  };

  // While the .brxt install (env-var configuration) flow is open, show it on
  // top of the browse list. One extension is installed at a time.
  if (brxtPath) {
    return (
      <BrxtInstallModal
        preloadedFilePath={brxtPath}
        onClose={() => setBrxtPath(null)}
        onInstalled={() => {
          setBrxtPath(null);
          onInstalled();
          onClose();
        }}
      />
    );
  }

  return (
    <Dialog open onOpenChange={(open) => !open && downloadingId === null && onClose()}>
      <DialogContent
        dismissible={downloadingId === null}
        className="flex max-h-[86vh] w-[720px] max-w-[92vw] flex-col gap-0 overflow-hidden p-0 sm:max-w-[92vw] lg:max-w-[720px]"
      >
        {/* Header */}
        <div className="px-6 pt-5 pb-4 pr-14 border-b border-border-subtle">
          <div>
            <DialogTitle>Browse Extensions</DialogTitle>
            <p className="text-xs text-text-muted mt-0.5">
              Install MCP extensions from the Biorouter marketplace. Add one at a time. Most need
              credentials configured during install.
              {/* §10.2: a last-good catalogue is dated rather than dismissed as
                  "offline" — the entries are real, they are just not current. */}
              {catalogFreshnessLine({ live, fetchedAt }) && (
                <span className="text-text-subtle">
                  {' '}
                  · {catalogFreshnessLine({ live, fetchedAt })}
                </span>
              )}
            </p>
          </div>
        </div>

        {/* Search */}
        <div className="px-6 pt-4 pb-3 border-b border-border-subtle">
          <input
            type="text"
            autoFocus
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder="Search extensions by name, description, or tag…"
            className="biorouter-modal-panel w-full rounded-lg px-3 py-2 text-sm"
          />
        </div>

        {/* List */}
        <div className="flex-1 overflow-y-auto px-6 py-3">
          {loadError && (
            <p className="text-sm text-text-danger text-center mt-10">
              Could not load the marketplace catalog.
            </p>
          )}
          {!registry && !loadError && (
            <p className="text-sm text-text-muted text-center mt-10 animate-pulse">
              Loading catalog…
            </p>
          )}
          {registry && filtered.length === 0 && (
            <p className="text-sm text-text-muted text-center mt-10">
              No extensions match your search.
            </p>
          )}
          <div className="flex flex-col gap-2">
            {filtered.map((ext) => {
              const installed = isInstalled(ext);
              const downloading = downloadingId === ext.id;
              return (
                <div
                  key={ext.id}
                  className="biorouter-modal-row flex items-start gap-3 rounded-lg px-3 py-3 hover:bg-background-default transition-colors"
                >
                  <div className="flex-shrink-0 mt-0.5 w-9 h-9 rounded-lg bg-background-default/80 flex items-center justify-center">
                    <Package className="w-5 h-5 text-text-muted" />
                  </div>
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-2 flex-wrap">
                      <span className="text-sm font-semibold text-text-default">{ext.name}</span>
                      {/* The union rule, not `ext.privacy`: a downgraded document
                          must not be able to un-badge an entry here either. */}
                      {registry && (
                        <PrivacyBadge
                          tier={effectivePrivacy(registry, ext.extension_name ?? ext.name)}
                        />
                      )}
                      <span className="text-[11px] text-text-subtle">
                        {[ext.organization, ext.version].filter(Boolean).join(' · ')}
                      </span>
                    </div>
                    <p className="text-xs text-text-muted mt-0.5 leading-relaxed">
                      {ext.description}
                    </p>
                    {ext.tags.length > 0 && (
                      <div className="flex gap-1 flex-wrap mt-1.5">
                        {ext.tags.map((t) => (
                          <span
                            key={t}
                            className="text-[10px] text-text-subtle bg-background-medium rounded px-1.5 py-0.5"
                          >
                            {t}
                          </span>
                        ))}
                      </div>
                    )}
                  </div>
                  <div className="flex-shrink-0 self-center">
                    {installed ? (
                      <span className="text-[10px] uppercase tracking-wide text-background-info bg-background-info/10 rounded px-2 py-1">
                        Installed
                      </span>
                    ) : (
                      <Button
                        size="sm"
                        variant="outline"
                        disabled={downloading || downloadingId !== null}
                        onClick={() => handleAdd(ext)}
                      >
                        {downloading ? 'Downloading…' : 'Add'}
                      </Button>
                    )}
                  </div>
                </div>
              );
            })}
          </div>
        </div>

        {/* Footer */}
        <div className="px-6 py-4 border-t border-border-subtle flex items-center justify-end">
          <Button variant="outline" onClick={onClose} disabled={downloadingId !== null}>
            Close
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
