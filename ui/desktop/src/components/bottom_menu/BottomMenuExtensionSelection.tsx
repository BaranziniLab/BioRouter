import { useCallback, useEffect, useMemo, useState, useRef } from 'react';
import { Puzzle } from '../icons/app-icons';
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuTrigger,
} from '../ui/dropdown-menu';
import { Input } from '../ui/input';
import { Switch } from '../ui/switch';
import BuiltInBadge from '../ui/BuiltInBadge';
import { FixedExtensionEntry, useConfig } from '../ConfigContext';
import { isCapabilityExtension } from '../settings/capabilities/capabilities';
import { toastService } from '../../toasts';
import {
  formatExtensionName,
  isBuiltInExtension,
} from '../settings/extensions/subcomponents/ExtensionList';
import { ExtensionConfig, getSessionExtensions } from '../../api';
import { addToAgent, removeFromAgent } from '../settings/extensions/agent-api';
import { setExtensionOverride, getExtensionOverrides } from '../../store/extensionOverrides';

interface BottomMenuExtensionSelectionProps {
  sessionId: string | null;
}

export const BottomMenuExtensionSelection = ({ sessionId }: BottomMenuExtensionSelectionProps) => {
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

    // Foundational capabilities (Developer, Extension Manager, Skills, Todo,
    // Memory, Knowledge) are managed in Settings → Capabilities, not toggled
    // per-conversation. Excluding them here keeps the chat extension list focused
    // on the optional built-ins and the user's own installed extensions, instead
    // of a long, confusing list that mixes in the always-on capabilities.
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

  const activeCount = useMemo(() => {
    return extensionsList.filter((ext) => ext.enabled).length;
  }, [extensionsList]);

  const visibleEnabledCount = useMemo(
    () => sortedExtensions.filter((ext) => ext.enabled).length,
    [sortedExtensions]
  );

  const handleBulkToggle = useCallback(async () => {
    if (bulkInFlight || pendingExtensionNames.size > 0 || sortedExtensions.length === 0) {
      return;
    }

    const targetEnabled = visibleEnabledCount === 0;
    const targets = sortedExtensions.filter((ext) => ext.enabled !== targetEnabled);
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
    sortedExtensions,
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
      <DropdownMenuTrigger asChild>
        <button
          type="button"
          className="flex h-7 items-center rounded-md px-0.5 cursor-pointer [&_svg]:size-4 text-text-default/70 hover:bg-background-medium hover:text-text-default text-xs"
          title="manage extensions"
          aria-label={`Manage extensions (${activeCount} enabled)`}
        >
          <Puzzle className="mr-0.5 h-4 w-4" />
          <span>{activeCount}</span>
        </button>
      </DropdownMenuTrigger>
      <DropdownMenuContent
        side="top"
        align="center"
        className="w-64"
        onCloseAutoFocus={(e) => {
          e.preventDefault();
        }}
      >
        <div className="p-2">
          <Input
            type="text"
            placeholder="search extensions..."
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            className="h-8 text-sm"
            autoFocus
          />
          {sortedExtensions.length > 0 && (
            <button
              type="button"
              onClick={handleBulkToggle}
              disabled={bulkInFlight || pendingExtensionNames.size > 0}
              className="mt-1.5 text-xs text-text-default/70 hover:text-text-default underline-offset-2 hover:underline disabled:opacity-50 disabled:cursor-not-allowed cursor-pointer"
            >
              {visibleEnabledCount === 0
                ? `Enable all (${sortedExtensions.length})`
                : `Disable all (${visibleEnabledCount})`}
            </button>
          )}
        </div>
        <div className="max-h-[400px] overflow-y-auto">
          {sortedExtensions.length === 0 ? (
            <div className="px-2 py-4 text-center text-sm text-text-default/70">
              {searchQuery ? 'no extensions found' : 'no extensions available'}
            </div>
          ) : (
            sortedExtensions.map((ext) => {
              const rowDisabled = bulkInFlight;
              return (
                <DropdownMenuCheckboxItem
                  key={ext.name}
                  checked={ext.enabled}
                  showIndicator={false}
                  disabled={rowDisabled}
                  onCheckedChange={(checked) => handleToggle(ext, checked)}
                  onSelect={(event) => event.preventDefault()}
                  className={`flex items-center justify-between px-2 py-2 transition-colors duration-[var(--motion-fast)] hover:bg-background-medium ${
                    rowDisabled ? 'cursor-wait opacity-70' : 'cursor-pointer'
                  }`}
                  title={ext.description || ext.name}
                >
                  <div className="flex items-center gap-1.5 min-w-0 pr-2">
                    <div className="text-sm font-medium text-text-default truncate">
                      {formatExtensionName(ext.name)}
                    </div>
                    {isBuiltInExtension(ext) && <BuiltInBadge />}
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
