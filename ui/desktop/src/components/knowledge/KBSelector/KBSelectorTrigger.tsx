import { useState } from 'react';
import { ChevronDown } from '../../icons/app-icons';
import { Popover, PopoverContent, PopoverTrigger } from '../../ui/popover';
import { useKnowledge } from '../KnowledgeContext';
import { KbDot } from '../KbDot';
import { KBSelectorMenu } from './KBSelectorMenu';

interface Props {
  /** Controlled open state. When provided, the trigger acts as a controlled component. */
  open?: boolean;
  onOpenChange?: (open: boolean) => void;
  /** Open the manager dialog (the trigger owns neither the dialog nor its state). */
  onManage: () => void;
  /** Open the manager dialog with the create form unfolded. */
  onCreate: () => void;
}

/**
 * The knowledge-base selector — a Select-SHAPED control, so it takes the Select
 * trigger's chrome exactly (ui-spec §4.1).
 *
 * Three corrections over the bespoke field it replaces:
 *
 * - **`border-border-emphasized`, not `border-border-input`.** That token is
 *   darker than `--border-strong` in light and lighter in dark, so the old
 *   `hover:border-border-strong` measurably WEAKENED the edge on hover in both
 *   modes (light 1.66 → 1.52; dark 1.91 → 1.59). Hover is now the Input
 *   primitive's whisper — an inset ring — and nothing shifts hue. (This is a
 *   repair, not compliance: the resting edge still measures 1.62–1.66 light /
 *   2.04–2.10 dark against SC 1.4.11's 3:1, which is an app-wide `Input` token
 *   change tracked outside this section.)
 * - **It opens an anchored picker, not a 760px modal.** Clicking a selector
 *   should show you the list, not a management console.
 * - **The `KB` badge is dropped**, so the control needs an accessible name of
 *   its own — the surrounding `<h1>` is not one.
 *
 * `gap-2` and not `gap-2.5`: 10px is off the 4px grid. `text-label` and not
 * `text-label font-semibold`: 14/600 is not a step in the type scale.
 */
export function KBSelectorTrigger({ open: openProp, onOpenChange, onManage, onCreate }: Props) {
  const { primaryKb, visibleBases } = useKnowledge();
  const [openInternal, setOpenInternal] = useState(false);

  // How many bases this chat uses *besides* the one named in the trigger. With
  // no primary the name slot names nothing, so every visible base is "other" —
  // subtracting 1 unconditionally would undercount them.
  const otherBaseCount = visibleBases.length - (primaryKb ? 1 : 0);

  // Support both controlled (open+onOpenChange) and uncontrolled usage.
  const isControlled = openProp !== undefined;
  const open = isControlled ? openProp : openInternal;
  const setOpen = isControlled ? (v: boolean) => onOpenChange?.(v) : setOpenInternal;

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <button
          type="button"
          data-testid="knowledge-kb-selector-trigger"
          aria-label="Knowledge base"
          aria-expanded={open}
          className="flex h-control-md w-full items-center gap-2 rounded-element border border-border-emphasized bg-background-default px-3 text-label transition-[color,background-color,border-color,box-shadow] hover:inset-ring-2 hover:inset-ring-border-emphasized/30"
        >
          <KbDot color={primaryKb?.color} />
          <span className="min-w-0 flex-1 truncate text-left">
            {primaryKb?.name ?? 'No primary knowledge base'}
          </span>
          {otherBaseCount > 0 && (
            <span
              className="shrink-0 text-supporting text-text-muted"
              title={`${otherBaseCount} more knowledge base${otherBaseCount === 1 ? '' : 's'} in this chat`}
            >
              +{otherBaseCount}
            </span>
          )}
          <ChevronDown
            aria-hidden="true"
            className={`h-icon-row w-icon-row shrink-0 text-text-muted transition-transform ${open ? 'rotate-180' : ''}`}
          />
        </button>
      </PopoverTrigger>
      <PopoverContent
        align="start"
        sideOffset={6}
        className="w-64 p-0"
        // The picker owns a text field, so the popover must not steal its focus
        // back to the trigger on open.
        onOpenAutoFocus={(event) => event.preventDefault()}
      >
        <KBSelectorMenu
          onClose={() => setOpen(false)}
          onManage={() => {
            setOpen(false);
            onManage();
          }}
          onCreate={() => {
            setOpen(false);
            onCreate();
          }}
        />
      </PopoverContent>
    </Popover>
  );
}
