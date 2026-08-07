import { useCallback, useEffect, useMemo, useState, useRef } from 'react';
import { Puzzle } from '../icons/app-icons';
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuLabel,
  DropdownMenuTrigger,
} from '../ui/dropdown-menu';
import { Input } from '../ui/input';
import { Switch } from '../ui/switch';
import { Tooltip, TooltipContent, TooltipTrigger } from '../ui/Tooltip';
import BuiltInBadge from '../ui/BuiltInBadge';
import { FixedExtensionEntry, useConfig } from '../ConfigContext';
import { isCapabilityExtension } from '../settings/capabilities/capabilities';
import { toastService } from '../../toasts';
import {
  formatExtensionName,
  isBuiltInExtension,
} from '../settings/extensions/subcomponents/ExtensionList';
import { ExtensionConfig, getSessionExtensions } from '../../api';
import type { SessionClassification } from '../../api/types.gen';
import { addToAgent, removeFromAgent } from '../settings/extensions/agent-api';
import { extensionPairingRefused } from '../settings/extensions/extensionPrivacy';
import { setExtensionOverride, getExtensionOverrides } from '../../store/extensionOverrides';

/** §14.5's reason, in the composer's own words. Public model → private tool. */
const PAIRING_REFUSED_REASON =
  'Unavailable in this chat — a private extension needs a private model';

interface BottomMenuExtensionSelectionProps {
  sessionId: string | null;
  /**
   * The focused chat's privacy tier (issue #56, §14.5).
   *
   * This component is where the *true* per-session pairing state belongs,
   * because it is the only extension surface that already knows which chat it
   * is looking at. Settings has no session awareness at all, and with tabs and
   * splits "the focused session" is undefined once the user navigates away from
   * chat — so Settings judges against the global default provider instead and
   * says so, and only this surface answers "will it work *here*".
   *
   * `undefined` means unresolved, not public: see `extensionPairingRefused`.
   */
  privacyTier?: SessionClassification;
}

export const BottomMenuExtensionSelection = ({
  sessionId,
  privacyTier,
}: BottomMenuExtensionSelectionProps) => {
  const [searchQuery, setSearchQuery] = useState('');
  const [isOpen, setIsOpen] = useState(false);
  const [sessionExtensions, setSessionExtensions] = useState<ExtensionConfig[]>([]);
  const [hubUpdateTrigger, setHubUpdateTrigger] = useState(0);
  const [sessionOverrides, setSessionOverrides] = useState<Map<string, boolean>>(new Map());
  const [pendingExtensionNames, setPendingExtensionNames] = useState<Set<string>>(new Set());
  const [bulkInFlight, setBulkInFlight] = useState(false);
  const [refreshTrigger, setRefreshTrigger] = useState(0);
  const refreshTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const sessionToggleChainsRef = useRef<Map<string, Promise<boolean>>>(new Map());
  const { extensionsList: allExtensions } = useConfig();
  const isHubView = !sessionId;

  useEffect(() => {
    const handleSessionLoaded = () => {
      if (refreshTimerRef.current) clearTimeout(refreshTimerRef.current);
      refreshTimerRef.current = setTimeout(() => {
        refreshTimerRef.current = null;
        setRefreshTrigger((prev) => prev + 1);
      }, 500);
    };

    window.addEventListener('session-created', handleSessionLoaded);
    window.addEventListener('message-stream-finished', handleSessionLoaded);

    return () => {
      window.removeEventListener('session-created', handleSessionLoaded);
      window.removeEventListener('message-stream-finished', handleSessionLoaded);
      if (refreshTimerRef.current) clearTimeout(refreshTimerRef.current);
    };
  }, []);

  useEffect(() => {
    sessionToggleChainsRef.current.clear();
    setSessionOverrides(new Map());
    setPendingExtensionNames(new Set());
  }, [sessionId]);

  // Fetch session-specific extensions or use global defaults
  useEffect(() => {
    const fetchExtensions = async () => {
      if (!sessionId) {
        return;
      }

      try {
        const response = await getSessionExtensions({
          path: { session_id: sessionId },
        });

        if (response.data?.extensions) {
          setSessionExtensions(response.data.extensions);
        }
      } catch (error) {
        console.error('Failed to fetch session extensions:', error);
      }
    };

    fetchExtensions();
  }, [sessionId, isOpen, refreshTrigger]);

  const handleToggle = useCallback(
    (extensionConfig: FixedExtensionEntry, requestedState?: boolean) => {
      const nextEnabled = requestedState ?? !extensionConfig.enabled;
      if (isHubView) {
        setExtensionOverride(extensionConfig.name, nextEnabled);
        setHubUpdateTrigger((prev) => prev + 1);

        toastService.success({
          title: 'Extension Updated',
          msg: `${formatExtensionName(extensionConfig.name)} will be ${nextEnabled ? 'enabled' : 'disabled'} in new chats`,
        });
        return;
      }

      if (!sessionId) {
        toastService.error({
          title: 'Extension Toggle Error',
          msg: 'No active session found. Please start a chat session first.',
          traceback: 'No session ID available',
        });
        return;
      }

      const name = extensionConfig.name;
      setSessionOverrides((prev) => new Map(prev).set(name, nextEnabled));
      setPendingExtensionNames((prev) => new Set(prev).add(name));

      const previous =
        sessionToggleChainsRef.current.get(name) ?? Promise.resolve(extensionConfig.enabled);
      const operation = previous.then(async (appliedState) => {
        if (appliedState === nextEnabled) return appliedState;
        try {
          if (nextEnabled) {
            await addToAgent(extensionConfig, sessionId, true);
          } else {
            await removeFromAgent(name, sessionId, true);
          }
          return nextEnabled;
        } catch {
          toastService.error({
            title: 'Extension Toggle Error',
            msg: `${formatExtensionName(name)} could not be ${nextEnabled ? 'enabled' : 'disabled'}.`,
          });
          return appliedState;
        }
      });

      sessionToggleChainsRef.current.set(name, operation);
      void operation.then(async () => {
        if (sessionToggleChainsRef.current.get(name) !== operation) return;

        try {
          const response = await getSessionExtensions({ path: { session_id: sessionId } });
          if (sessionToggleChainsRef.current.get(name) !== operation) return;
          if (response.data?.extensions) setSessionExtensions(response.data.extensions);
        } catch {
          toastService.error({
            title: 'Extension Refresh Error',
            msg: 'The latest extension state could not be refreshed.',
          });
        } finally {
          if (sessionToggleChainsRef.current.get(name) === operation) {
            sessionToggleChainsRef.current.delete(name);
            setSessionOverrides((prev) => {
              const next = new Map(prev);
              next.delete(name);
              return next;
            });
            setPendingExtensionNames((prev) => {
              const next = new Set(prev);
              next.delete(name);
              return next;
            });
          }
        }
      });
    },
    [isHubView, sessionId]
  );

  // Merge all available extensions with session-specific or hub override state
  const extensionsList = useMemo(() => {
    const hubOverrides = getExtensionOverrides();

    // Shipped capabilities are managed in Settings → Chat → Capabilities, not
    // overridden per conversation. The chat menu contains only user-installed
    // extensions.
    const togglable = allExtensions.filter((ext) => !isCapabilityExtension(ext));

    if (isHubView) {
      return togglable.map(
        (ext) =>
          ({
            ...ext,
            enabled: hubOverrides.has(ext.name) ? hubOverrides.get(ext.name)! : ext.enabled,
          }) as FixedExtensionEntry
      );
    }

    const sessionExtensionNames = new Set(sessionExtensions.map((ext) => ext.name));

    return togglable.map(
      (ext) =>
        ({
          ...ext,
          enabled: sessionOverrides.has(ext.name)
            ? sessionOverrides.get(ext.name)!
            : sessionExtensionNames.has(ext.name),
        }) as FixedExtensionEntry
    );
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [allExtensions, sessionExtensions, sessionOverrides, isHubView, hubUpdateTrigger]);

  const filteredExtensions = useMemo(() => {
    return extensionsList.filter((ext) => {
      const query = searchQuery.toLowerCase();
      return (
        ext.name.toLowerCase().includes(query) ||
        (ext.description && ext.description.toLowerCase().includes(query))
      );
    });
  }, [extensionsList, searchQuery]);

  const sortedExtensions = useMemo(() => {
    return [...filteredExtensions].sort((a, b) => a.name.localeCompare(b.name));
  }, [filteredExtensions]);

  /**
   * The rows "Enable all" may actually act on.
   *
   * A refused pairing is rendered but not toggleable, so it must not be counted
   * in the bulk label either — otherwise "Enable all (4)" enables three and
   * leaves the fourth looking broken, which is the failure this state exists to
   * remove.
   */
  const toggleableExtensions = useMemo(
    () => sortedExtensions.filter((ext) => !extensionPairingRefused(ext.name, privacyTier)),
    [sortedExtensions, privacyTier]
  );

  const activeCount = useMemo(() => {
    return extensionsList.filter((ext) => ext.enabled).length;
  }, [extensionsList]);

  const visibleEnabledCount = useMemo(
    () => toggleableExtensions.filter((ext) => ext.enabled).length,
    [toggleableExtensions]
  );

  const handleBulkToggle = useCallback(async () => {
    if (bulkInFlight || pendingExtensionNames.size > 0 || toggleableExtensions.length === 0) {
      return;
    }

    const targetEnabled = visibleEnabledCount === 0;
    const targets = toggleableExtensions.filter((ext) => ext.enabled !== targetEnabled);
    if (targets.length === 0) {
      return;
    }

    setBulkInFlight(true);
    if (isHubView) {
      targets.forEach((ext) => setExtensionOverride(ext.name, targetEnabled));
      setHubUpdateTrigger((prev) => prev + 1);
      setBulkInFlight(false);

      toastService.success({
        title: 'Extensions Updated',
        msg: `${targets.length} extension${targets.length === 1 ? '' : 's'} ${targetEnabled ? 'enabled' : 'disabled'} in new chats`,
      });
      return;
    }

    if (!sessionId) {
      setBulkInFlight(false);
      toastService.error({
        title: 'Extension Toggle Error',
        msg: 'No active session found. Please start a chat session first.',
        traceback: 'No session ID available',
      });
      return;
    }

    try {
      setSessionOverrides((prev) => {
        const next = new Map(prev);
        targets.forEach((ext) => next.set(ext.name, targetEnabled));
        return next;
      });
      await Promise.all(
        targets.map((ext) =>
          targetEnabled
            ? addToAgent(ext, sessionId, true)
            : removeFromAgent(ext.name, sessionId, true)
        )
      );
      const response = await getSessionExtensions({ path: { session_id: sessionId } });
      if (response.data?.extensions) setSessionExtensions(response.data.extensions);

      toastService.success({
        title: 'Extensions Updated',
        msg: `${targets.length} extension${targets.length === 1 ? '' : 's'} ${targetEnabled ? 'enabled' : 'disabled'} for this chat session`,
      });
    } catch {
      toastService.error({
        title: 'Extension Toggle Error',
        msg: 'The extension selection could not be updated.',
      });
    } finally {
      setSessionOverrides((prev) => {
        const next = new Map(prev);
        targets.forEach((ext) => next.delete(ext.name));
        return next;
      });
      setBulkInFlight(false);
    }
  }, [
    bulkInFlight,
    pendingExtensionNames.size,
    toggleableExtensions,
    visibleEnabledCount,
    isHubView,
    sessionId,
  ]);

  return (
    <DropdownMenu
      open={isOpen}
      onOpenChange={(open) => {
        setIsOpen(open);
        if (!open) {
          setSearchQuery('');
        }
      }}
    >
      <Tooltip>
        <TooltipTrigger asChild>
          <DropdownMenuTrigger asChild>
            <button
              type="button"
              className="flex h-7 items-center rounded-md px-0.5 cursor-pointer [&_svg]:size-4 text-text-default/70 hover:bg-background-medium hover:text-text-default text-xs"
              aria-label={`Manage extensions (${activeCount} enabled)`}
            >
              <Puzzle className="mr-0.5 h-4 w-4" />
              <span>{activeCount}</span>
            </button>
          </DropdownMenuTrigger>
        </TooltipTrigger>
        <TooltipContent side="top">Manage extensions</TooltipContent>
      </Tooltip>
      <DropdownMenuContent
        side="top"
        align="center"
        className="w-64"
        onCloseAutoFocus={(e) => {
          e.preventDefault();
        }}
      >
        <div className="p-1">
          <Input
            type="text"
            placeholder="Search extensions..."
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            className="h-8 text-[13px]"
            autoFocus
          />
        </div>
        {/* §4.5 section label names the group; the bulk action shares its row as a
            real ghost affordance (radius + hover fill + hit area) rather than the
            bare underlined text link it used to be. */}
        <div className="flex items-center justify-between gap-2">
          <DropdownMenuLabel>Extensions</DropdownMenuLabel>
          {toggleableExtensions.length > 0 && (
            <button
              type="button"
              onClick={handleBulkToggle}
              disabled={bulkInFlight || pendingExtensionNames.size > 0}
              className="mr-2 shrink-0 cursor-pointer rounded-sm px-1.5 py-0.5 text-[11px] font-medium text-text-muted transition-colors duration-[var(--motion-fast)] hover:bg-background-medium hover:text-text-default disabled:cursor-not-allowed disabled:opacity-50"
            >
              {visibleEnabledCount === 0
                ? `Enable all (${toggleableExtensions.length})`
                : `Disable all (${visibleEnabledCount})`}
            </button>
          )}
        </div>
        <div className="max-h-[400px] overflow-y-auto">
          {sortedExtensions.length === 0 ? (
            <div className="px-3 py-4 text-center text-[13px] leading-[18px] text-text-muted">
              {searchQuery ? 'No extensions found' : 'No extensions available'}
            </div>
          ) : (
            sortedExtensions.map((ext) => {
              // §14.5, and the reason it is a *state* rather than an omission:
              // dropping the row is what produces "the OMOP tool is broken".
              // Gate C is invisible in the GUI by construction — it returns
              // `ErrorData` from inside `dispatch_tool_call`, so it never
              // enters `PermissionCheckResult`, produces no approval card and
              // records no denial. Nothing else in this app will ever tell the
              // user why the tool did nothing.
              //
              // The row stays in the list and carries `aria-disabled="true"` —
              // Radix derives that attribute from `disabled` below, which is
              // why the string appears nowhere else in this file. Filtering the
              // row out instead would satisfy every other assertion here and
              // reintroduce the exact silence this state removes.
              const pairingRefused = extensionPairingRefused(ext.name, privacyTier);
              const rowDisabled = bulkInFlight || pairingRefused;
              return (
                <DropdownMenuCheckboxItem
                  key={ext.name}
                  checked={ext.enabled}
                  showIndicator={false}
                  disabled={rowDisabled}
                  onCheckedChange={(checked) => handleToggle(ext, checked)}
                  onSelect={(event) => event.preventDefault()}
                  // Geometry comes from the shared §4.5 row (32px, 12px padding,
                  // --radius-md, 13/18, --background-medium hover) — this call site
                  // adds only the toggle layout and the in-flight cursor.
                  className={`justify-between hover:bg-background-medium ${
                    pairingRefused
                      ? 'h-auto cursor-not-allowed items-start py-1.5 opacity-70'
                      : rowDisabled
                        ? 'cursor-wait opacity-70'
                        : 'cursor-pointer'
                  }`}
                  title={pairingRefused ? PAIRING_REFUSED_REASON : ext.description || ext.name}
                >
                  <div className="flex min-w-0 flex-col gap-0.5 pr-2">
                    <div className="flex items-center gap-1.5 min-w-0">
                      <div className="font-medium text-text-default truncate">
                        {formatExtensionName(ext.name)}
                      </div>
                      {isBuiltInExtension(ext) && <BuiltInBadge />}
                    </div>
                    {pairingRefused && (
                      <div className="text-[11px] leading-4 text-text-muted">
                        Unavailable in this chat (public model)
                      </div>
                    )}
                  </div>
                  <div className="pointer-events-none" aria-hidden="true">
                    <Switch
                      checked={ext.enabled}
                      variant="mono"
                      disabled={rowDisabled}
                      tabIndex={-1}
                      aria-hidden="true"
                    />
                  </div>
                </DropdownMenuCheckboxItem>
              );
            })
          )}
        </div>
      </DropdownMenuContent>
    </DropdownMenu>
  );
};
