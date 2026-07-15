import React, { useEffect, useState, useRef, useCallback, useMemo, startTransition } from 'react';
import {
  MessageSquareText,
  Target,
  AlertCircle,
  Calendar,
  Folder,
  Edit2,
  Trash2,
  Download,
  Upload,
  ExternalLink,
  Puzzle,
  LayoutDashboard,
} from '../icons/app-icons';
import { GitBranch } from 'lucide-react';
import { useNavigate } from 'react-router-dom';
import { useDashboard } from '../../contexts/DashboardContext';
import { toastError } from '../../toasts';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '../ui/dropdown-menu';
import { Button } from '../ui/button';
import { ScrollArea } from '../ui/scroll-area';
import { formatMessageTimestamp } from '../../utils/timeUtils';
import { billedSessionTokenEstimate, formatBilledTokenEstimate } from '../../utils/billedTokens';
import { SearchView } from '../conversation/SearchView';
import { SearchHighlighter } from '../../utils/searchHighlighter';
import { MainPanelLayout } from '../Layout/MainPanelLayout';
import { groupSessionsByDate, type DateGroup } from '../../utils/dateUtils';
import { Skeleton } from '../ui/skeleton';
import { toast } from 'react-toastify';
import { ConfirmationModal } from '../ui/ConfirmationModal';
import { ImportSessionModal } from './ImportSessionModal';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '../ui/Tooltip';
import {
  deleteSession,
  exportSession,
  importSession,
  Session,
  updateSessionName,
  ExtensionConfig,
  ExtensionData,
} from '../../api';
import { formatExtensionName } from '../settings/extensions/subcomponents/ExtensionList';
import { getSearchShortcutText } from '../../utils/keyboardShortcuts';
import { ReadableContent } from '../Layout/ReadableContent';
import { Dialog, DialogContent, DialogTitle } from '../ui/dialog';
import { EmptyState } from '../ui/empty-state';
import {
  getCachedSessionList,
  refreshSessionList,
  updateCachedSessionList,
} from '../../utils/sessionListCache';

function getSessionExtensionNames(extensionData: ExtensionData): string[] {
  try {
    const enabledExtensionData = extensionData?.['enabled_extensions.v0'] as
      | { extensions?: ExtensionConfig[] }
      | undefined;
    if (!enabledExtensionData?.extensions) return [];

    return enabledExtensionData.extensions.map((ext) => formatExtensionName(ext.name));
  } catch {
    return [];
  }
}

interface EditSessionModalProps {
  session: Session | null;
  isOpen: boolean;
  onClose: () => void;
  onSave: (sessionId: string, newDescription: string) => Promise<void>;
  disabled?: boolean;
}

const EditSessionModal = React.memo<EditSessionModalProps>(
  ({ session, isOpen, onClose, onSave, disabled = false }) => {
    const [description, setDescription] = useState('');
    const [isUpdating, setIsUpdating] = useState(false);

    useEffect(() => {
      if (session && isOpen) {
        setDescription(session.name);
      } else if (!isOpen) {
        setDescription('');
        setIsUpdating(false);
      }
    }, [session, isOpen]);

    const handleSave = useCallback(async () => {
      if (!session || disabled || isUpdating) return;

      const trimmedDescription = description.trim();
      if (trimmedDescription === session.name) {
        onClose();
        return;
      }

      setIsUpdating(true);
      try {
        await updateSessionName({
          path: { session_id: session.id },
          body: { name: trimmedDescription },
          throwOnError: true,
        });
        await onSave(session.id, trimmedDescription);
        onClose();
        setTimeout(() => {
          toast.success('Session description updated successfully');
        }, 300);
      } catch (error) {
        const errorMessage = error instanceof Error ? error.message : 'Unknown error occurred';
        console.error('Failed to update session description:', errorMessage);
        toast.error(`Failed to update session description: ${errorMessage}`);
        setDescription(session.name);
      } finally {
        setIsUpdating(false);
      }
    }, [session, description, onSave, onClose, disabled, isUpdating]);

    const handleCancel = useCallback(() => {
      if (!isUpdating) {
        onClose();
      }
    }, [onClose, isUpdating]);

    const handleKeyDown = useCallback(
      (e: React.KeyboardEvent<HTMLInputElement>) => {
        if (e.key === 'Enter' && !isUpdating) {
          handleSave();
        }
      },
      [handleSave, isUpdating]
    );

    const handleInputChange = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
      setDescription(e.target.value);
    }, []);

    if (!isOpen || !session) return null;

    return (
      <Dialog
        open={isOpen}
        onOpenChange={(open) => !open && !isUpdating && !disabled && handleCancel()}
      >
        <DialogContent
          dismissible={!isUpdating && !disabled}
          className="w-[500px] max-w-[90vw] sm:max-w-[500px]"
        >
          <DialogTitle>Edit Session Description</DialogTitle>

          <div className="space-y-4">
            <div>
              <input
                id="session-description"
                type="text"
                value={description}
                onChange={handleInputChange}
                className="biorouter-modal-panel w-full p-3 rounded-lg text-text-default "
                placeholder="Enter session description"
                autoFocus
                maxLength={200}
                onKeyDown={handleKeyDown}
                disabled={isUpdating || disabled}
              />
            </div>
          </div>

          <div className="flex justify-end space-x-3 mt-6">
            <Button onClick={handleCancel} variant="ghost" disabled={isUpdating || disabled}>
              Cancel
            </Button>
            <Button
              onClick={handleSave}
              disabled={!description.trim() || isUpdating || disabled}
              variant="default"
            >
              {isUpdating ? 'Saving...' : 'Save'}
            </Button>
          </div>
        </DialogContent>
      </Dialog>
    );
  }
);

EditSessionModal.displayName = 'EditSessionModal';

// Debounce hook for search
function useDebounce<T>(value: T, delay: number): T {
  const [debouncedValue, setDebouncedValue] = useState<T>(value);

  useEffect(() => {
    const handler = setTimeout(() => {
      setDebouncedValue(value);
    }, delay);

    return () => {
      window.clearTimeout(handler);
    };
  }, [value, delay]);

  return debouncedValue;
}

interface SearchContainerElement extends HTMLDivElement {
  _searchHighlighter: SearchHighlighter | null;
}

interface SessionListViewProps {
  onSelectSession: (sessionId: string) => void;
  selectedSessionId?: string | null;
}

const HISTORY_LOADING_GROUPS = [5, 4];
const HISTORY_LOADING_TITLE_WIDTHS = ['w-3/4', 'w-2/3', 'w-4/5', 'w-1/2'];
const INITIAL_VISIBLE_SESSIONS = 16;
const VISIBLE_SESSION_BATCH = 20;

function HistoryLoading() {
  let rowIndex = 0;

  return (
    <div role="status" aria-label="Loading chat history" className="space-y-8">
      <span className="sr-only">Loading chat history</span>
      {HISTORY_LOADING_GROUPS.map((rowCount, groupIndex) => (
        <div key={groupIndex} className="space-y-4" aria-hidden="true">
          <Skeleton
            className="biorouter-history-loading-cell h-3 w-16 rounded-sm bg-background-medium"
            style={{ animationDelay: `${-groupIndex * 180}ms` }}
          />
          <div className="session-grid biorouter-list-shell overflow-hidden">
            {Array.from({ length: rowCount }, (_, index) => {
              const currentRow = rowIndex++;
              const delay = -((currentRow * 95) % 1100);

              return (
                <div
                  key={index}
                  data-testid="history-loading-row"
                  className="flex min-h-10 items-center gap-3 border-b border-border-subtle px-4 py-2 last:border-b-0"
                >
                  <div className="min-w-0 flex-1">
                    <Skeleton
                      className={`biorouter-history-loading-cell mb-1.5 h-4 ${HISTORY_LOADING_TITLE_WIDTHS[currentRow % HISTORY_LOADING_TITLE_WIDTHS.length]} rounded-sm bg-background-medium`}
                      style={{ animationDelay: `${delay}ms` }}
                    />
                    <div className="flex items-center gap-3">
                      <Skeleton
                        className="biorouter-history-loading-cell h-3 w-20 rounded-sm bg-background-medium"
                        style={{ animationDelay: `${delay - 70}ms` }}
                      />
                      <Skeleton
                        className="biorouter-history-loading-cell h-3 w-32 rounded-sm bg-background-medium"
                        style={{ animationDelay: `${delay - 140}ms` }}
                      />
                    </div>
                  </div>
                  <Skeleton
                    className="biorouter-history-loading-cell h-3 w-14 rounded-sm bg-background-medium"
                    style={{ animationDelay: `${delay - 210}ms` }}
                  />
                </div>
              );
            })}
          </div>
        </div>
      ))}
    </div>
  );
}

const SessionListView: React.FC<SessionListViewProps> = React.memo(
  ({ onSelectSession, selectedSessionId }) => {
    const initialSessions = useRef(getCachedSessionList()).current;
    const navigate = useNavigate();
    const dashboard = useDashboard();
    const [sessions, setSessions] = useState<Session[]>(initialSessions ?? []);
    const [filteredSessions, setFilteredSessions] = useState<Session[]>(initialSessions ?? []);
    const [dateGroups, setDateGroups] = useState<DateGroup[]>(() =>
      groupSessionsByDate(initialSessions ?? [])
    );
    const [isLoading, setIsLoading] = useState(initialSessions === null);
    const [showSkeleton, setShowSkeleton] = useState(initialSessions === null);
    const [showContent, setShowContent] = useState(initialSessions !== null);
    const [isInitialLoad, setIsInitialLoad] = useState(initialSessions === null);
    const [error, setError] = useState<string | null>(null);
    const [searchResults, setSearchResults] = useState<{
      count: number;
      currentIndex: number;
    } | null>(null);

    const [visibleSessionCount, setVisibleSessionCount] = useState(INITIAL_VISIBLE_SESSIONS);

    // Edit modal state
    const [showEditModal, setShowEditModal] = useState(false);
    const [editingSession, setEditingSession] = useState<Session | null>(null);

    // Delete confirmation modal state
    const [showDeleteConfirmation, setShowDeleteConfirmation] = useState(false);
    const [sessionToDelete, setSessionToDelete] = useState<Session | null>(null);

    // Import modal state
    const [showImportModal, setShowImportModal] = useState(false);

    // Search state for debouncing
    const [searchTerm, setSearchTerm] = useState('');
    const [caseSensitive, setCaseSensitive] = useState(false);
    const debouncedSearchTerm = useDebounce(searchTerm, 300); // 300ms debounce

    const containerRef = useRef<HTMLDivElement>(null);

    // Track session to element ref
    const sessionRefs = useRef<Record<string, HTMLElement>>({});
    const setSessionRefs = (itemId: string, element: HTMLDivElement | null) => {
      if (element) {
        sessionRefs.current[itemId] = element;
      } else {
        delete sessionRefs.current[itemId];
      }
    };

    const visibleDateGroups = useMemo(() => {
      let remainingSessions = visibleSessionCount;

      return dateGroups.flatMap((group) => {
        if (remainingSessions <= 0) return [];
        const sessions = group.sessions.slice(0, remainingSessions);
        remainingSessions -= sessions.length;
        return [{ ...group, sessions }];
      });
    }, [dateGroups, visibleSessionCount]);

    // id → name lookup so a diverged session can show its lineage parent's name.
    const sessionNameById = useMemo(() => {
      const map = new Map<string, string>();
      for (const s of sessions) map.set(s.id, s.name);
      return map;
    }, [sessions]);

    const handleScroll = useCallback(
      (target: HTMLDivElement) => {
        const { scrollTop, scrollHeight, clientHeight } = target;
        const threshold = 200;

        if (
          scrollHeight - scrollTop - clientHeight < threshold &&
          visibleSessionCount < filteredSessions.length
        ) {
          setVisibleSessionCount((previousCount) =>
            Math.min(previousCount + VISIBLE_SESSION_BATCH, filteredSessions.length)
          );
        }
      },
      [visibleSessionCount, filteredSessions.length]
    );

    useEffect(() => {
      if (debouncedSearchTerm) {
        setVisibleSessionCount(filteredSessions.length);
      } else {
        setVisibleSessionCount(INITIAL_VISIBLE_SESSIONS);
      }
    }, [debouncedSearchTerm, filteredSessions.length]);

    const loadSessions = useCallback(async () => {
      const hasCachedSessions = getCachedSessionList() !== null;
      if (!hasCachedSessions) {
        setIsLoading(true);
        setShowSkeleton(true);
        setShowContent(false);
        setError(null);
      }
      try {
        const refreshedSessions = await refreshSessionList();
        // Use startTransition to make state updates non-blocking
        startTransition(() => {
          setSessions(refreshedSessions);
          setFilteredSessions(refreshedSessions);
          setError(null);
        });
      } catch (err) {
        console.error('Failed to load sessions:', err);
        if (!hasCachedSessions) {
          setError('Failed to load sessions. Please try again later.');
          setSessions([]);
          setFilteredSessions([]);
        }
      } finally {
        if (!hasCachedSessions) setIsLoading(false);
      }
    }, []);

    useEffect(() => {
      loadSessions();
    }, [loadSessions]);

    // Timing logic to prevent flicker between skeleton and content on initial load
    useEffect(() => {
      if (!isLoading && showSkeleton) {
        setShowSkeleton(false);
        // Use startTransition for non-blocking content show
        startTransition(() => {
          setTimeout(() => {
            setShowContent(true);
            if (isInitialLoad) {
              setIsInitialLoad(false);
            }
          }, 10);
        });
      }
      return () => void 0;
    }, [isLoading, showSkeleton, isInitialLoad]);

    // Memoize date groups calculation to prevent unnecessary recalculations
    const memoizedDateGroups = useMemo(() => {
      if (filteredSessions.length > 0) {
        return groupSessionsByDate(filteredSessions);
      }
      return [];
    }, [filteredSessions]);

    // Update date groups when filtered sessions change
    useEffect(() => {
      startTransition(() => {
        setDateGroups(memoizedDateGroups);
      });
    }, [memoizedDateGroups]);

    // Scroll to the selected session when returning from session history view
    useEffect(() => {
      if (selectedSessionId) {
        const selectedIndex = filteredSessions.findIndex(
          (session) => session.id === selectedSessionId
        );
        if (selectedIndex >= visibleSessionCount) {
          setVisibleSessionCount(selectedIndex + 1);
          return;
        }
        const element = sessionRefs.current[selectedSessionId];
        if (element) {
          element.scrollIntoView({
            block: 'center',
          });
        }
      }
    }, [filteredSessions, selectedSessionId, sessions, visibleSessionCount]);

    // Debounced search effect - performs actual filtering
    useEffect(() => {
      if (!debouncedSearchTerm) {
        startTransition(() => {
          setFilteredSessions(sessions);
          setSearchResults(null);
        });
        return;
      }

      // Use startTransition to make search non-blocking
      startTransition(() => {
        const searchTerm = caseSensitive ? debouncedSearchTerm : debouncedSearchTerm.toLowerCase();
        const filtered = sessions.filter((session) => {
          const description = session.name;
          const workingDir = session.working_dir;
          const sessionId = session.id;

          if (caseSensitive) {
            return (
              description.includes(searchTerm) ||
              sessionId.includes(searchTerm) ||
              workingDir.includes(searchTerm)
            );
          } else {
            return (
              description.toLowerCase().includes(searchTerm) ||
              sessionId.toLowerCase().includes(searchTerm) ||
              workingDir.toLowerCase().includes(searchTerm)
            );
          }
        });

        setFilteredSessions(filtered);
        setSearchResults(filtered.length > 0 ? { count: filtered.length, currentIndex: 1 } : null);
      });
    }, [debouncedSearchTerm, caseSensitive, sessions]);

    // Handle immediate search input (updates search term for debouncing)
    const handleSearch = useCallback((term: string, caseSensitive: boolean) => {
      setSearchTerm(term);
      setCaseSensitive(caseSensitive);
    }, []);

    // Handle search result navigation
    const handleSearchNavigation = (direction: 'next' | 'prev') => {
      if (!searchResults || filteredSessions.length === 0) return;

      let newIndex: number;
      if (direction === 'next') {
        newIndex = (searchResults.currentIndex % filteredSessions.length) + 1;
      } else {
        newIndex =
          searchResults.currentIndex === 1
            ? filteredSessions.length
            : searchResults.currentIndex - 1;
      }

      setSearchResults({ ...searchResults, currentIndex: newIndex });

      // Find the SearchView's container element
      const searchContainer =
        containerRef.current?.querySelector<SearchContainerElement>('.search-container');
      if (searchContainer?._searchHighlighter) {
        // Update the current match in the highlighter
        searchContainer._searchHighlighter.setCurrentMatch(newIndex - 1, true);
      }
    };

    // Handle modal close
    const handleModalClose = useCallback(() => {
      setShowEditModal(false);
      setEditingSession(null);
    }, []);

    const handleModalSave = useCallback(async (sessionId: string, newDescription: string) => {
      // Update state immediately for optimistic UI
      const updateName = (currentSessions: Session[]) =>
        currentSessions.map((session) =>
          session.id === sessionId ? { ...session, name: newDescription } : session
        );
      updateCachedSessionList(updateName);
      setSessions(updateName);
    }, []);

    const handleEditSession = useCallback((session: Session) => {
      setEditingSession(session);
      setShowEditModal(true);
    }, []);

    const handleDeleteSession = useCallback((session: Session) => {
      setSessionToDelete(session);
      setShowDeleteConfirmation(true);
    }, []);

    const handleConfirmDelete = useCallback(async () => {
      if (!sessionToDelete) return;

      setShowDeleteConfirmation(false);
      const sessionToDeleteId = sessionToDelete.id;
      const sessionName = sessionToDelete.name;
      setSessionToDelete(null);

      try {
        await deleteSession({
          path: { session_id: sessionToDeleteId },
          throwOnError: true,
        });
        const removeDeletedSession = (currentSessions: Session[]) =>
          currentSessions.filter((session) => session.id !== sessionToDeleteId);
        updateCachedSessionList(removeDeletedSession);
        setSessions(removeDeletedSession);
        toast.success('Session deleted successfully');
      } catch (error) {
        console.error('Error deleting session:', error);
        const errorMessage = error instanceof Error ? error.message : 'Unknown error';
        toast.error(`Failed to delete session "${sessionName}": ${errorMessage}`);
      }
      await loadSessions();
    }, [sessionToDelete, loadSessions]);

    const handleCancelDelete = useCallback(() => {
      setShowDeleteConfirmation(false);
      setSessionToDelete(null);
    }, []);

    const handleExportSession = useCallback(async (session: Session, e: React.MouseEvent) => {
      e.stopPropagation();

      const response = await exportSession({
        path: { session_id: session.id },
        throwOnError: true,
      });

      const json = response.data;
      const blob = new Blob([json], { type: 'application/json' });
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = `${session.name}.json`;
      document.body.appendChild(a);
      a.click();
      document.body.removeChild(a);
      URL.revokeObjectURL(url);
      toast.success('Session exported successfully');
    }, []);

    const handleImportClick = useCallback(() => {
      setShowImportModal(true);
    }, []);

    const handleImportSession = useCallback(
      async (json: string) => {
        await importSession({ body: { json }, throwOnError: true });
        toast.success('Session imported successfully');
        await loadSessions();
      },
      [loadSessions]
    );

    const handleOpenInNewWindow = useCallback((session: Session, e: React.MouseEvent) => {
      e.stopPropagation();
      window.electron.createChatWindow(
        undefined,
        session.working_dir,
        undefined,
        session.id,
        'pair'
      );
    }, []);

    const handleOpenInDashboard = useCallback(
      async (session: Session, e: React.MouseEvent) => {
        e.stopPropagation();
        try {
          await dashboard.spawnWindow({
            resumeSessionId: session.id,
            cwd: session.working_dir,
            name: session.name,
          });
          navigate('/dashboard');
        } catch (err) {
          toastError({
            title: 'Failed to add session to dashboard',
            msg: err instanceof Error ? err.message : String(err),
          });
        }
      },
      [dashboard, navigate]
    );

    const SessionItem = React.memo(function SessionItem({
      session,
      onEditClick,
      onDeleteClick,
      onExportClick,
      onOpenInNewWindow,
      onOpenInDashboard,
    }: {
      session: Session;
      onEditClick: (session: Session) => void;
      onDeleteClick: (session: Session) => void;
      onExportClick: (session: Session, e: React.MouseEvent) => void;
      onOpenInNewWindow: (session: Session, e: React.MouseEvent) => void;
      onOpenInDashboard: (session: Session, e: React.MouseEvent) => void;
    }) {
      const handleEditClick = useCallback(
        (e: React.MouseEvent) => {
          e.stopPropagation(); // Prevent card click
          onEditClick(session);
        },
        [onEditClick, session]
      );

      const handleDeleteClick = useCallback(
        (e: React.MouseEvent) => {
          e.stopPropagation(); // Prevent card click
          onDeleteClick(session);
        },
        [onDeleteClick, session]
      );

      const handleCardClick = useCallback(() => {
        onSelectSession(session.id);
      }, [session.id]);

      const handleExportClick = useCallback(
        (e: React.MouseEvent) => {
          onExportClick(session, e);
        },
        [onExportClick, session]
      );

      const handleOpenInNewWindowClick = useCallback(
        (e: React.MouseEvent) => {
          onOpenInNewWindow(session, e);
        },
        [onOpenInNewWindow, session]
      );

      const handleOpenInDashboardClick = useCallback(
        (e: React.MouseEvent) => {
          onOpenInDashboard(session, e);
        },
        [onOpenInDashboard, session]
      );

      // Get extension names for this session
      const extensionNames = useMemo(
        () => getSessionExtensionNames(session.extension_data),
        [session.extension_data]
      );
      const billedTokenEstimate = billedSessionTokenEstimate(session);

      return (
        <div
          className="biorouter-list-row session-item flex items-center gap-3 py-2 px-4 relative group"
          ref={(el) => setSessionRefs(session.id, el)}
        >
          {/* Title + metadata */}
          <button
            type="button"
            onClick={handleCardClick}
            className="flex-1 min-w-0 cursor-pointer rounded-sm text-left focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-border-focus"
            aria-label={`Open session ${session.name}`}
          >
            <h3 className="text-sm font-medium truncate">{session.name}</h3>
            {session.diverged_from && (
              <div className="flex items-center gap-1 mt-0.5 text-text-muted text-xs min-w-0">
                <GitBranch className="w-3 h-3 flex-shrink-0" strokeWidth={1.5} />
                <span className="truncate max-w-[320px]">
                  branched from{' '}
                  {sessionNameById.get(session.diverged_from) ?? session.diverged_from}
                </span>
              </div>
            )}
            <div className="flex items-center gap-3 mt-0.5 text-text-muted text-xs">
              <div className="flex items-center gap-1">
                <Calendar className="w-3 h-3 flex-shrink-0" />
                <span>{formatMessageTimestamp(Date.parse(session.updated_at) / 1000)}</span>
              </div>
              <div className="flex items-center gap-1 min-w-0">
                <Folder className="w-3 h-3 flex-shrink-0" />
                <span className="truncate max-w-[240px]">{session.working_dir}</span>
              </div>
            </div>
          </button>

          {/* Right-side stats + hover actions */}
          <div className="flex items-center gap-3 flex-shrink-0">
            <div className="flex items-center gap-3 text-xs text-text-muted font-mono">
              <div className="flex items-center gap-1">
                <MessageSquareText className="w-3 h-3" />
                <span>{session.message_count}</span>
              </div>
              {billedTokenEstimate && (
                <div
                  className="flex items-center gap-1"
                  title={
                    billedTokenEstimate.lowerBound
                      ? 'At least this many tokens; only last-turn usage is available for this legacy session'
                      : 'Billed tokens across every turn, including recorded cache usage'
                  }
                >
                  <Target className="w-3 h-3" />
                  <span className="sr-only">Billed tokens: </span>
                  <span>{formatBilledTokenEstimate(billedTokenEstimate)}</span>
                </div>
              )}
              {extensionNames.length > 0 && (
                <TooltipProvider>
                  <Tooltip>
                    <TooltipTrigger asChild>
                      <div
                        className="flex items-center gap-0.5"
                        onClick={(e) => e.stopPropagation()}
                      >
                        <Puzzle className="w-3 h-3" />
                        <span>{extensionNames.length}</span>
                      </div>
                    </TooltipTrigger>
                    <TooltipContent side="top" className="max-w-xs">
                      <div className="text-xs">
                        <div className="font-medium mb-1">Extensions:</div>
                        <ul className="list-disc list-inside">
                          {extensionNames.map((name) => (
                            <li key={name}>{name}</li>
                          ))}
                        </ul>
                      </div>
                    </TooltipContent>
                  </Tooltip>
                </TooltipProvider>
              )}
            </div>
            <div className="flex gap-1 opacity-100 transition-opacity sm:opacity-0 sm:group-hover:opacity-100 sm:group-focus-within:opacity-100">
              <DropdownMenu>
                <Tooltip>
                  <TooltipTrigger asChild>
                    <DropdownMenuTrigger asChild>
                      <Button
                        onClick={(e) => e.stopPropagation()}
                        variant="outline"
                        size="sm"
                        className="h-7 w-7 p-0"
                        aria-label={`Launch options for ${session.name}`}
                      >
                        <ExternalLink className="w-4 h-4" />
                      </Button>
                    </DropdownMenuTrigger>
                  </TooltipTrigger>
                  <TooltipContent side="top">Launch session</TooltipContent>
                </Tooltip>
                <DropdownMenuContent align="end" onClick={(e) => e.stopPropagation()}>
                  <DropdownMenuItem onClick={(e) => handleOpenInNewWindowClick(e)}>
                    <ExternalLink className="w-4 h-4" />
                    Open in new window
                  </DropdownMenuItem>
                  <DropdownMenuItem onClick={(e) => handleOpenInDashboardClick(e)}>
                    <LayoutDashboard className="w-4 h-4" />
                    Add to dashboard
                  </DropdownMenuItem>
                </DropdownMenuContent>
              </DropdownMenu>
              <Tooltip>
                <TooltipTrigger asChild>
                  <Button
                    onClick={handleEditClick}
                    variant="outline"
                    size="sm"
                    className="h-7 w-7 p-0"
                    aria-label={`Edit ${session.name}`}
                  >
                    <Edit2 className="w-4 h-4" />
                  </Button>
                </TooltipTrigger>
                <TooltipContent side="top">Edit session name</TooltipContent>
              </Tooltip>
              <Tooltip>
                <TooltipTrigger asChild>
                  <Button
                    onClick={handleExportClick}
                    variant="outline"
                    size="sm"
                    className="h-7 w-7 p-0"
                    aria-label={`Export ${session.name}`}
                  >
                    <Download className="w-4 h-4" />
                  </Button>
                </TooltipTrigger>
                <TooltipContent side="top">Export session</TooltipContent>
              </Tooltip>
              <Tooltip>
                <TooltipTrigger asChild>
                  <Button
                    onClick={handleDeleteClick}
                    variant="ghost"
                    size="sm"
                    className="h-8 w-8 p-0 text-text-danger hover:bg-background-danger/10"
                    aria-label={`Delete ${session.name}`}
                  >
                    <Trash2 className="w-4 h-4" />
                  </Button>
                </TooltipTrigger>
                <TooltipContent side="top">Delete session</TooltipContent>
              </Tooltip>
            </div>
          </div>
        </div>
      );
    });

    const renderActualContent = () => {
      if (error) {
        return (
          <div className="flex flex-col items-center justify-center h-full text-text-muted">
            <AlertCircle className="h-12 w-12 text-text-danger mb-4" />
            <p className="text-lg mb-2">Error Loading Sessions</p>
            <p className="text-sm text-center mb-4">{error}</p>
            <Button onClick={loadSessions} variant="default">
              Try Again
            </Button>
          </div>
        );
      }

      if (sessions.length === 0) {
        return (
          <EmptyState
            icon={MessageSquareText}
            title="No conversations yet"
            description="Past conversations will appear here after you start chatting. You can also import an existing session."
            actions={
              <>
                <Button onClick={() => navigate('/pair')}>Start a chat</Button>
                <Button onClick={handleImportClick} variant="outline">
                  <Upload className="h-4 w-4" />
                  Import session
                </Button>
              </>
            }
          />
        );
      }

      if (dateGroups.length === 0 && searchResults !== null) {
        return (
          <EmptyState
            icon={MessageSquareText}
            title="No matching conversations"
            description="Try a different name, folder, or session ID."
            compact
          />
        );
      }

      return (
        <div className="space-y-8">
          {visibleDateGroups.map((group) => (
            <div key={group.label} className="space-y-4">
              <div className="sticky top-0 z-10 bg-background-muted pt-2 pb-2">
                <h2 className="text-xs font-medium text-text-muted uppercase tracking-wider">
                  {group.label}
                </h2>
              </div>
              <div className="session-grid biorouter-list-shell">
                {group.sessions.map((session) => (
                  <SessionItem
                    key={session.id}
                    session={session}
                    onEditClick={handleEditSession}
                    onDeleteClick={handleDeleteSession}
                    onExportClick={handleExportSession}
                    onOpenInNewWindow={handleOpenInNewWindow}
                    onOpenInDashboard={handleOpenInDashboard}
                  />
                ))}
              </div>
            </div>
          ))}

          {visibleSessionCount < filteredSessions.length && (
            <div className="flex justify-center py-8">
              <div className="flex items-center space-x-2 text-text-muted">
                <div className="animate-spin rounded-full h-4 w-4 border-b-2 border-text-muted"></div>
                <span>Loading more sessions...</span>
              </div>
            </div>
          )}
        </div>
      );
    };

    return (
      <>
        <MainPanelLayout>
          <div className="flex-1 flex flex-col min-h-0">
            {/* Flat page header */}
            <div className="flex-shrink-0 border-b border-border-subtle">
              <ReadableContent className="px-8 pt-12 pb-6">
                <div className="flex justify-between items-center mb-1 page-transition">
                  <h1 className="text-2xl font-semibold tracking-tight">Chat history</h1>
                  <Button
                    onClick={handleImportClick}
                    variant="outline"
                    className="flex items-center gap-2"
                  >
                    <Upload className="w-4 h-4" />
                    Import Session
                  </Button>
                </div>
                <p className="text-sm text-text-muted">
                  View and search your past conversations with BioRouter. {getSearchShortcutText()}{' '}
                  to search.
                </p>
              </ReadableContent>
            </div>

            <ReadableContent className="flex-1 min-h-0 relative px-8 pt-6 pb-8">
              <ScrollArea
                handleScroll={handleScroll}
                className="h-full"
                paddingX={1}
                data-search-scroll-area
              >
                <div ref={containerRef} className="h-full relative">
                  <SearchView
                    onSearch={handleSearch}
                    onNavigate={handleSearchNavigation}
                    searchResults={searchResults}
                    className="relative"
                    placeholder="Search history..."
                  >
                    {/* Loading layer - shaped like the rows that will replace it. */}
                    <div
                      className={`absolute inset-0 transition-opacity duration-[var(--motion-fast)] ease-[var(--ease-in)] ${isLoading || showSkeleton ? 'opacity-100 z-10' : 'opacity-0 z-0 pointer-events-none'}`}
                    >
                      {(isLoading || showSkeleton) && <HistoryLoading />}
                    </div>

                    {/* Content layer - always rendered but conditionally visible */}
                    <div
                      className={`relative transition-opacity duration-[var(--motion-base)] ease-[var(--ease-out)] ${showContent ? 'opacity-100 z-10' : 'opacity-0 z-0'}`}
                    >
                      {renderActualContent()}
                    </div>
                  </SearchView>
                </div>
              </ScrollArea>
            </ReadableContent>
          </div>
        </MainPanelLayout>

        <EditSessionModal
          session={editingSession}
          isOpen={showEditModal}
          onClose={handleModalClose}
          onSave={handleModalSave}
        />

        <ImportSessionModal
          isOpen={showImportModal}
          onClose={() => setShowImportModal(false)}
          onImport={handleImportSession}
        />

        <ConfirmationModal
          isOpen={showDeleteConfirmation}
          title="Delete Session"
          message={`Are you sure you want to delete the session "${sessionToDelete?.name}"? This action cannot be undone.`}
          confirmLabel="Delete Session"
          cancelLabel="Cancel"
          confirmVariant="destructive"
          onConfirm={handleConfirmDelete}
          onCancel={handleCancelDelete}
        />
      </>
    );
  }
);

SessionListView.displayName = 'SessionListView';

export default SessionListView;
