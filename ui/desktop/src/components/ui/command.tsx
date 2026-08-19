import * as React from 'react';
import { cn } from '../../utils';
import { Search } from '../icons/app-icons';

/**
 * The searchable picker — a pinned text field over a filtered list.
 *
 * ⚠ **This is deliberately NOT a `DropdownMenu` with an `<Input>` in it.**
 * Radix's `DropdownMenu` implements the ARIA *menu* pattern, which includes
 * typeahead: printable characters are captured at the menu root and move focus
 * to the matching item. A nested text field then competes for every keystroke —
 * characters are swallowed, or focus jumps out of the field mid-word — and the
 * arrow keys belong to the menu's roving focus, so the field cannot be exited
 * predictably. Five surfaces in the Knowledge section need this shape
 * (knowledge-base/okf-migration/ui-spec.md §4.0), so specifying it as a menu
 * would have replicated the defect five times.
 *
 * What this is instead is the **combobox / listbox** pattern: DOM focus stays in
 * the field, the list is a `role="listbox"` of `role="option"` rows, and the
 * highlighted row is named by `aria-activedescendant` rather than focused. Arrow
 * keys, Home/End and Enter are handled on the field; Escape is left to bubble,
 * so the `Popover` this normally lives inside closes it.
 *
 * ⚠ **No `cmdk`.** The specification names cmdk as the implementation and cmdk
 * is not a dependency of this app (checked: absent from `package.json` and from
 * `node_modules`). Rather than add a runtime dependency the design explicitly
 * budgets elsewhere, the pattern is implemented here against the app's own
 * primitives. The exported shape is deliberately cmdk-compatible —
 * `CommandInput` / `CommandList` / `CommandGroup` / `CommandItem` /
 * `CommandEmpty` / `CommandSeparator` — so adopting cmdk later is a swap of this
 * file and not of its call sites.
 *
 * **Filtering is the caller's job**, but the QUERY lives here. The caller
 * already holds the collection and usually needs the filtered set for other
 * reasons (a count, an empty state, a "create ‹query›" affordance) — a second,
 * hidden filter inside the primitive is how a list ends up disagreeing with the
 * number printed above it. The query is owned here because the highlight has to
 * be repaired whenever it changes, and a repair keyed on state this component
 * cannot see is a repair that does not run.
 */

interface CommandContextValue {
  query: string;
  setQuery: (value: string) => void;
  activeId: string | null;
  setActiveId: (id: string | null) => void;
}

const CommandContext = React.createContext<CommandContextValue | null>(null);

function useCommandContext(component: string): CommandContextValue {
  const ctx = React.useContext(CommandContext);
  if (!ctx) {
    throw new Error(`<${component}> must be rendered inside <Command>`);
  }
  return ctx;
}

export interface CommandProps extends Omit<React.ComponentProps<'div'>, 'onSelect'> {
  /** Announced name for the listbox. */
  label: string;
  /** The search text. */
  query: string;
  onQueryChange: (value: string) => void;
}

export function Command({
  label,
  query,
  onQueryChange,
  className,
  children,
  ...props
}: CommandProps) {
  const [activeId, setActiveId] = React.useState<string | null>(null);
  const rootRef = React.useRef<HTMLDivElement>(null);

  const visibleIds = React.useCallback((): string[] => {
    const root = rootRef.current;
    if (!root) return [];
    return Array.from(root.querySelectorAll<HTMLElement>('[data-command-item]'))
      .filter((el) => el.getAttribute('aria-disabled') !== 'true')
      .map((el) => el.id);
  }, []);

  // Keep the highlight on a row that exists. It reads the DOM the render just
  // produced, because that is the only place that knows what the CALLER's
  // filter left behind. Keyed on the query — the one input that changes the set
  // — with `move()` and Enter resolving defensively for every other cause.
  React.useEffect(() => {
    const ids = visibleIds();
    if (ids.length === 0) {
      if (activeId !== null) setActiveId(null);
      return;
    }
    if (activeId === null || !ids.includes(activeId)) {
      setActiveId(ids[0]);
    }
  }, [query, activeId, visibleIds]);

  /** The highlighted row, resolved against what is actually on screen now. */
  function effectiveActive(ids: string[]): string | null {
    if (ids.length === 0) return null;
    return activeId && ids.includes(activeId) ? activeId : ids[0];
  }

  function move(delta: number | 'first' | 'last') {
    const ids = visibleIds();
    const current = effectiveActive(ids);
    if (current === null) return;
    const index = ids.indexOf(current);
    const next =
      delta === 'first'
        ? 0
        : delta === 'last'
          ? ids.length - 1
          : (index + delta + ids.length) % ids.length;
    const id = ids[next];
    setActiveId(id);
    document.getElementById(id)?.scrollIntoView({ block: 'nearest' });
  }

  function onKeyDown(event: React.KeyboardEvent<HTMLDivElement>) {
    switch (event.key) {
      case 'ArrowDown':
        event.preventDefault();
        move(1);
        break;
      case 'ArrowUp':
        event.preventDefault();
        move(-1);
        break;
      case 'Home':
        event.preventDefault();
        move('first');
        break;
      case 'End':
        event.preventDefault();
        move('last');
        break;
      case 'Enter': {
        const id = effectiveActive(visibleIds());
        if (!id) return;
        event.preventDefault();
        document.getElementById(id)?.click();
        break;
      }
      // Escape is NOT handled: it belongs to the Popover that owns this.
      default:
        break;
    }
  }

  return (
    <CommandContext.Provider value={{ query, setQuery: onQueryChange, activeId, setActiveId }}>
      <div
        ref={rootRef}
        data-command-root
        data-command-label={label}
        onKeyDown={onKeyDown}
        className={cn('flex min-h-0 flex-col', className)}
        {...props}
      >
        {children}
      </div>
    </CommandContext.Provider>
  );
}

export type CommandInputProps = Omit<React.ComponentProps<'input'>, 'value' | 'onChange'>;

export function CommandInput({ className, placeholder, ...props }: CommandInputProps) {
  const { query, setQuery, activeId } = useCommandContext('CommandInput');

  return (
    // The field's own 32px, from the Input primitive's geometry. Sub-32px fields
    // do not exist in this app and adding a `size` variant to `Input` is a
    // design-system change this surface does not own (ui-spec §4.0).
    <div className="flex h-control-md flex-none items-center gap-2 border-b border-border-subtle px-2">
      <Search className="h-icon-row w-icon-row shrink-0 text-text-muted" aria-hidden="true" />
      <input
        type="text"
        role="combobox"
        aria-expanded
        aria-autocomplete="list"
        aria-activedescendant={activeId ?? undefined}
        value={query}
        placeholder={placeholder}
        onChange={(event) => setQuery(event.target.value)}
        // Focus is global (D-15) and this field's ground IS the popover, so a
        // bordered box would be a second field edge inside a surface that
        // already has one.
        className={cn(
          'min-w-0 flex-1 bg-transparent text-label text-text-default outline-none placeholder:text-text-muted',
          className
        )}
        {...props}
      />
    </div>
  );
}

export function CommandList({ className, children, ...props }: React.ComponentProps<'div'>) {
  return (
    <div role="listbox" className={cn('min-h-0 flex-1 overflow-y-auto p-1', className)} {...props}>
      {children}
    </div>
  );
}

export function CommandGroup({
  heading,
  className,
  children,
  ...props
}: React.ComponentProps<'div'> & { heading?: React.ReactNode }) {
  const headingId = React.useId();
  return (
    <div role="group" aria-labelledby={heading ? headingId : undefined} {...props}>
      {heading ? (
        <div id={headingId} className="px-2 pb-1 pt-2 text-caps text-text-muted">
          {heading}
        </div>
      ) : null}
      <div className={cn('flex flex-col', className)}>{children}</div>
    </div>
  );
}

export function CommandSeparator({ className, ...props }: React.ComponentProps<'div'>) {
  return (
    <div role="separator" className={cn('my-1 h-px bg-border-subtle', className)} {...props} />
  );
}

export interface CommandItemProps extends Omit<React.ComponentProps<'div'>, 'onSelect'> {
  onSelect: () => void;
  /** Marks the row as the current VALUE (a check), not as the highlight. */
  selected?: boolean;
  disabled?: boolean;
}

export function CommandItem({
  onSelect,
  selected = false,
  disabled = false,
  className,
  children,
  ...props
}: CommandItemProps) {
  const id = React.useId();
  const { activeId, setActiveId } = useCommandContext('CommandItem');
  const active = activeId === id;

  return (
    <div
      id={id}
      role="option"
      data-command-item=""
      aria-selected={selected}
      aria-disabled={disabled || undefined}
      // The pointer moves the highlight rather than painting a separate hover
      // state, so the keyboard's notion of "current row" and the pointer's are
      // one thing.
      onMouseMove={() => {
        if (!disabled && !active) setActiveId(id);
      }}
      onClick={() => {
        if (!disabled) onSelect();
      }}
      className={cn(
        // A row inside a 12px popover takes the next step down: `rounded-inner`
        // is the nested-in-a-control rung.
        'flex h-row-rail cursor-pointer select-none items-center gap-2 rounded-inner px-2 text-label',
        disabled && 'cursor-not-allowed opacity-50',
        active && !disabled && 'tint-selected tint-interactive',
        className
      )}
      {...props}
    >
      {children}
    </div>
  );
}

export function CommandEmpty({ className, children, ...props }: React.ComponentProps<'div'>) {
  return (
    <div className={cn('px-2 py-6 text-center', className)} {...props}>
      {children}
    </div>
  );
}
