import { useCallback, useEffect, useMemo, useState, useRef } from 'react';
import { Puzzle } from '../icons/app-icons';
import { DropdownMenu, DropdownMenuContent, DropdownMenuTrigger } from '../ui/dropdown-menu';
import { Input } from '../ui/input';
import { Switch } from '../ui/switch';
import BuiltInBadge from '../ui/BuiltInBadge';
import { FixedExtensionEntry, useConfig } from '../ConfigContext';
import { toastService } from '../../toasts';
import {
  formatExtensionName,
  isBuiltInExtension,
} from '../settings/extensions/subcomponents/ExtensionList';
import { ExtensionConfig, getSessionExtensions } from '../../api';
import { addToAgent, removeFromAgent } from '../settings/extensions/agent-api';
import {
  setExtensionOverride,
  getExtensionOverride,
  getExtensionOverrides,
} from '../../store/extensionOverrides';

interface BottomMenuExtensionSelectionProps {
  sessionId: string | null;
}

export const BottomMenuExtensionSelection = ({ sessionId }: BottomMenuExtensionSelectionProps) => {
  const [searchQuery, setSearchQuery] = useState('');
  const [isOpen, setIsOpen] = useState(false);
  const [sessionExtensions, setSessionExtensions] = useState<ExtensionConfig[]>([]);
  const [hubUpdateTrigger, setHubUpdateTrigger] = useState(0);
  const [isTransitioning, setIsTransitioning] = useState(false);
  const [pendingSort, setPendingSort] = useState(false);
  const [togglingExtension, setTogglingExtension] = useState<string | null>(null);
  const [bulkInFlight, setBulkInFlight] = useState(false);
  const [refreshTrigger, setRefreshTrigger] = useState(0);
  const sortTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const { extensionsList: allExtensions } = useConfig();
  const isHubView = !sessionId;

  useEffect(() => {
    const handleSessionLoaded = () => {
      setTimeout(() => {
        setRefreshTrigger((prev) => prev + 1);
      }, 500);
    };

    window.addEventListener('session-created', handleSessionLoaded);
    window.addEventListener('message-stream-finished', handleSessionLoaded);

    return () => {
      window.removeEventListener('session-created', handleSessionLoaded);
      window.removeEventListener('message-stream-finished', handleSessionLoaded);
    };
  }, []);

  useEffect(() => {
    return () => {
      if (sortTimeoutRef.current) {
        clearTimeout(sortTimeoutRef.current);
      }
    };
  }, []);

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
    async (extensionConfig: FixedExtensionEntry) => {
      if (togglingExtension === extensionConfig.name) {
        return;
      }

      setIsTransitioning(true);
      setTogglingExtension(extensionConfig.name);

      if (isHubView) {
        const currentState = getExtensionOverride(extensionConfig.name) ?? extensionConfig.enabled;
        setExtensionOverride(extensionConfig.name, !currentState);
        setPendingSort(true);

        if (sortTimeoutRef.current) {
          clearTimeout(sortTimeoutRef.current);
        }

        // Delay the re-sort to allow animation
        sortTimeoutRef.current = setTimeout(() => {
          setHubUpdateTrigger((prev) => prev + 1);
          setPendingSort(false);
          setIsTransitioning(false);
          setTogglingExtension(null);
        }, 800);

        toastService.success({
          title: 'Extension Updated',
          msg: `${formatExtensionName(extensionConfig.name)} will be ${!currentState ? 'enabled' : 'disabled'} in new chats`,
        });
        return;
      }

      if (!sessionId) {
        setIsTransitioning(false);
        setTogglingExtension(null);
        toastService.error({
          title: 'Extension Toggle Error',
          msg: 'No active session found. Please start a chat session first.',
          traceback: 'No session ID available',
        });
        return;
      }

      try {
        if (extensionConfig.enabled) {
          await removeFromAgent(extensionConfig.name, sessionId, true);
        } else {
          await addToAgent(extensionConfig, sessionId, true);
        }

        setPendingSort(true);

        if (sortTimeoutRef.current) {
          clearTimeout(sortTimeoutRef.current);
        }

        sortTimeoutRef.current = setTimeout(async () => {
          const response = await getSessionExtensions({
            path: { session_id: sessionId },
          });

          if (response.data?.extensions) {
            setSessionExtensions(response.data.extensions);
          }
          setPendingSort(false);
          setIsTransitioning(false);
          setTogglingExtension(null);
        }, 800);
      } catch {
        setIsTransitioning(false);
        setPendingSort(false);
        setTogglingExtension(null);
      }
    },
    [sessionId, isHubView, togglingExtension]
  );

  // Merge all available extensions with session-specific or hub override state
  const extensionsList = useMemo(() => {
    const hubOverrides = getExtensionOverrides();

    if (isHubView) {
      return allExtensions.map(
        (ext) =>
          ({
            ...ext,
            enabled: hubOverrides.has(ext.name) ? hubOverrides.get(ext.name)! : ext.enabled,
          }) as FixedExtensionEntry
      );
    }

    const sessionExtensionNames = new Set(sessionExtensions.map((ext) => ext.name));

    return allExtensions.map(
      (ext) =>
        ({
          ...ext,
          enabled: sessionExtensionNames.has(ext.name),
        }) as FixedExtensionEntry
    );
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [allExtensions, sessionExtensions, isHubView, hubUpdateTrigger]);

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
    return [...filteredExtensions].sort((a, b) => {
      // Primary sort: enabled first
      if (a.enabled !== b.enabled) return a.enabled ? -1 : 1;

      // Secondary sort: alphabetically by name
      return a.name.localeCompare(b.name);
    });
  }, [filteredExtensions]);

  const activeCount = useMemo(() => {
    return extensionsList.filter((ext) => ext.enabled).length;
  }, [extensionsList]);

  const visibleEnabledCount = useMemo(
    () => sortedExtensions.filter((ext) => ext.enabled).length,
    [sortedExtensions]
  );

  const handleBulkToggle = useCallback(async () => {
    if (bulkInFlight || togglingExtension !== null || sortedExtensions.length === 0) {
      return;
    }

    const targetEnabled = visibleEnabledCount === 0;
    const targets = sortedExtensions.filter((ext) => ext.enabled !== targetEnabled);
    if (targets.length === 0) {
      return;
    }

    setBulkInFlight(true);
    setIsTransitioning(true);

    if (isHubView) {
      targets.forEach((ext) => setExtensionOverride(ext.name, targetEnabled));
      setPendingSort(true);

      if (sortTimeoutRef.current) {
        clearTimeout(sortTimeoutRef.current);
      }
      sortTimeoutRef.current = setTimeout(() => {
        setHubUpdateTrigger((prev) => prev + 1);
        setPendingSort(false);
        setIsTransitioning(false);
        setBulkInFlight(false);
      }, 800);

      toastService.success({
        title: 'Extensions Updated',
        msg: `${targets.length} extension${targets.length === 1 ? '' : 's'} ${targetEnabled ? 'enabled' : 'disabled'} in new chats`,
      });
      return;
    }

    if (!sessionId) {
      setIsTransitioning(false);
      setBulkInFlight(false);
      toastService.error({
        title: 'Extension Toggle Error',
        msg: 'No active session found. Please start a chat session first.',
        traceback: 'No session ID available',
      });
      return;
    }

    try {
      await Promise.all(
        targets.map((ext) =>
          targetEnabled
            ? addToAgent(ext, sessionId, true)
            : removeFromAgent(ext.name, sessionId, true)
        )
      );

      setPendingSort(true);
      if (sortTimeoutRef.current) {
        clearTimeout(sortTimeoutRef.current);
      }
      sortTimeoutRef.current = setTimeout(async () => {
        const response = await getSessionExtensions({
          path: { session_id: sessionId },
        });
        if (response.data?.extensions) {
          setSessionExtensions(response.data.extensions);
        }
        setPendingSort(false);
        setIsTransitioning(false);
        setBulkInFlight(false);
      }, 800);

      toastService.success({
        title: 'Extensions Updated',
        msg: `${targets.length} extension${targets.length === 1 ? '' : 's'} ${targetEnabled ? 'enabled' : 'disabled'} for this chat session`,
      });
    } catch {
      setIsTransitioning(false);
      setPendingSort(false);
      setBulkInFlight(false);
    }
  }, [
    bulkInFlight,
    togglingExtension,
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
          if (sortTimeoutRef.current) {
            clearTimeout(sortTimeoutRef.current);
          }
          setIsTransitioning(false);
          setPendingSort(false);
          setTogglingExtension(null);
          setBulkInFlight(false);
        }
      }}
    >
      <DropdownMenuTrigger asChild>
        <button
          className="flex items-center cursor-pointer [&_svg]:size-4 text-text-default/70 hover:text-text-default hover:scale-100 hover:bg-transparent text-xs"
          title="manage extensions"
        >
          <Puzzle className="mr-1 h-4 w-4" />
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
              disabled={bulkInFlight || togglingExtension !== null}
              className="mt-1.5 text-xs text-text-default/70 hover:text-text-default underline-offset-2 hover:underline disabled:opacity-50 disabled:cursor-not-allowed cursor-pointer"
            >
              {visibleEnabledCount === 0
                ? `Enable all (${sortedExtensions.length})`
                : `Disable all (${visibleEnabledCount})`}
            </button>
          )}
        </div>
        <div
          className={`max-h-[400px] overflow-y-auto transition-opacity duration-300 ${
            isTransitioning && pendingSort ? 'opacity-50' : 'opacity-100'
          }`}
        >
          {sortedExtensions.length === 0 ? (
            <div className="px-2 py-4 text-center text-sm text-text-default/70">
              {searchQuery ? 'no extensions found' : 'no extensions available'}
            </div>
          ) : (
            sortedExtensions.map((ext) => {
              const isToggling = togglingExtension === ext.name;
              const rowDisabled = isToggling || bulkInFlight;
              return (
                <div
                  key={ext.name}
                  className={`flex items-center justify-between px-2 py-2 hover:bg-background-medium transition-all duration-300 ${
                    rowDisabled ? 'cursor-wait opacity-70' : 'cursor-pointer'
                  }`}
                  onClick={() => !rowDisabled && handleToggle(ext)}
                  title={ext.description || ext.name}
                >
                  <div className="flex items-center gap-1.5 min-w-0 pr-2">
                    <div className="text-sm font-medium text-text-default truncate">
                      {formatExtensionName(ext.name)}
                    </div>
                    {isBuiltInExtension(ext) && <BuiltInBadge />}
                  </div>
                  <div onClick={(e) => e.stopPropagation()}>
                    <Switch
                      checked={ext.enabled}
                      onCheckedChange={() => handleToggle(ext)}
                      variant="mono"
                      disabled={rowDisabled}
                    />
                  </div>
                </div>
              );
            })
          )}
        </div>
      </DropdownMenuContent>
    </DropdownMenu>
  );
};
