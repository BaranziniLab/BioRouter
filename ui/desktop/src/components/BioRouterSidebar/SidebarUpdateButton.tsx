import { useEffect, useState } from 'react';
import { Download } from '../icons/app-icons';
import { UPDATES_ENABLED } from '../../updates';
import {
  hasKnownUpdate,
  initialUpdaterState,
  reduceUpdaterEvent,
  stateFromSnapshot,
  type UpdaterState,
} from '../../utils/updaterState';
import { requestUpdateModal } from '../../utils/updateUiEvents';

const PHASE_ORDER: UpdaterState['phase'][] = [
  'idle',
  'up-to-date',
  'checking',
  'error',
  'available',
  'downloaded',
];

export default function SidebarUpdateButton() {
  const [state, setState] = useState<UpdaterState>(initialUpdaterState);

  useEffect(() => {
    if (!UPDATES_ENABLED || !window.electron?.onUpdaterEvent) return;

    let cancelled = false;
    const dispose = window.electron.onUpdaterEvent((payload) => {
      if (!cancelled) setState((previous) => reduceUpdaterEvent(previous, payload));
    });

    window.electron
      .getUpdateState?.()
      ?.then((snapshot) => {
        if (cancelled || !snapshot) return;
        const recovered = stateFromSnapshot(snapshot);
        setState((previous) =>
          PHASE_ORDER.indexOf(recovered.phase) > PHASE_ORDER.indexOf(previous.phase)
            ? recovered
            : previous
        );
      })
      .catch(() => {
        // Live updater events remain the source of truth when no snapshot is available.
      });

    return () => {
      cancelled = true;
      dispose?.();
    };
  }, []);

  if (!UPDATES_ENABLED || !hasKnownUpdate(state)) return null;

  const versionLabel = state.latestVersion ? ` to ${state.latestVersion}` : '';

  return (
    <button
      type="button"
      data-testid="sidebar-update-button"
      aria-label={`Update Biorouter${versionLabel}`}
      title={`Update Biorouter${versionLabel}`}
      onClick={() => requestUpdateModal(state)}
      className="flex h-8 w-full items-center justify-center gap-2 rounded-lg bg-background-accent px-3 text-[11px] font-semibold tracking-[0.12em] text-text-on-accent transition-colors duration-150 hover:bg-background-accent-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-sidebar-ring"
    >
      <Download className="size-3.5" />
      <span>UPDATE</span>
    </button>
  );
}
