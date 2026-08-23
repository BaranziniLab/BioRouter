import { isBrowserSurface } from '../../utils/surface';
import { HOST_MANAGED_MODEL_REASON, HOST_MANAGED_MODEL_SHORT } from './hostManagedModelCopy';

/**
 * The inline note that sits beside a provider/model control a browser session
 * cannot use (SD-1). See `hostManagedModelCopy.ts` for the ruling and the words.
 *
 * ⚠ **Renders nothing on the desktop**, so a call site can mount it
 * unconditionally. Every surface carrying it would otherwise repeat the same
 * `isBrowserSurface() && …` guard, and the one that forgot would ship a note
 * telling desktop users their model is fixed.
 */
export function HostManagedModelNote({
  className,
  short = false,
}: {
  className?: string;
  /** Use the one-line form, for a chip or a settings row. */
  short?: boolean;
}) {
  if (!isBrowserSurface()) return null;
  return (
    <p
      data-testid="host-managed-model-note"
      className={className ?? 'text-xs leading-relaxed text-text-muted'}
    >
      {short ? HOST_MANAGED_MODEL_SHORT : HOST_MANAGED_MODEL_REASON}
    </p>
  );
}
