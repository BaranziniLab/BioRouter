import { useCallback, useEffect, useMemo, useState, useRef } from 'react';
import { Layers } from '../icons/app-icons';
import { DropdownMenu, DropdownMenuContent, DropdownMenuTrigger } from '../ui/dropdown-menu';
import { Input } from '../ui/input';
import { Switch } from '../ui/switch';
import {
  loadSkillOverrides,
  saveSkillOverrides,
  setSkillOverride,
  isSkillEnabled,
  getSkillOverrides,
} from '../../store/skillOverrides';
import { Skill, SkillBundle, ALL_SKILL_DIRS, loadSkillsFromDirs } from '../skills/skillUtils';
import { toastService } from '../../toasts';

interface BottomMenuSkillSelectionProps {
  sessionId: string | null;
}

type SkillEntry =
  | { kind: 'single'; skill: Skill; enabled: boolean }
  | { kind: 'bundle'; bundle: SkillBundle; enabled: boolean };

export const BottomMenuSkillSelection = ({ sessionId }: BottomMenuSkillSelectionProps) => {
  const [searchQuery, setSearchQuery] = useState('');
  const [isOpen, setIsOpen] = useState(false);
  const [allSkills, setAllSkills] = useState<Skill[]>([]);
  const [allBundles, setAllBundles] = useState<SkillBundle[]>([]);
  const [hubUpdateTrigger, setHubUpdateTrigger] = useState(0);
  const [isTransitioning, setIsTransitioning] = useState(false);
  const [pendingSort, setPendingSort] = useState(false);
  const [togglingKey, setTogglingKey] = useState<string | null>(null);
  const [bulkInFlight, setBulkInFlight] = useState(false);
  const [sessionOverrides, setSessionOverrides] = useState<Map<string, boolean>>(new Map());
  const sortTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const isHubView = !sessionId;

  const loadAll = useCallback(() => {
    return loadSkillOverrides().then(() => {
      return loadSkillsFromDirs(ALL_SKILL_DIRS).then(({ singles, bundles }) => {
        setAllSkills(singles);
        setAllBundles(bundles);
      });
    });
  }, []);

  useEffect(() => {
    loadAll();
  }, [loadAll]);

  useEffect(() => {
    if (isOpen) {
      loadAll().then(() => setHubUpdateTrigger((prev) => prev + 1));
    }
  }, [isOpen, loadAll]);

  useEffect(() => {
    return () => {
      if (sortTimeoutRef.current) clearTimeout(sortTimeoutRef.current);
    };
  }, []);

  const handleToggle = useCallback(
    async (key: string, displayName: string) => {
      if (togglingKey === key) return;

      setIsTransitioning(true);
      setTogglingKey(key);

      const scheduleSort = () => {
        setPendingSort(true);
        if (sortTimeoutRef.current) clearTimeout(sortTimeoutRef.current);
        sortTimeoutRef.current = setTimeout(() => {
          setHubUpdateTrigger((prev) => prev + 1);
          setPendingSort(false);
          setIsTransitioning(false);
          setTogglingKey(null);
        }, 800);
      };

      if (isHubView) {
        const currentEnabled = isSkillEnabled(key);
        setSkillOverride(key, !currentEnabled);
        await saveSkillOverrides();
        scheduleSort();
        toastService.success({
          title: 'Skill Updated',
          msg: `${displayName} will be ${!currentEnabled ? 'enabled' : 'disabled'} in new chats`,
        });
        return;
      }

      // Session view: local state only
      const currentEnabled = sessionOverrides.has(key)
        ? sessionOverrides.get(key)!
        : isSkillEnabled(key);
      const newEnabled = !currentEnabled;
      setSessionOverrides((prev) => {
        const next = new Map(prev);
        next.set(key, newEnabled);
        return next;
      });
      scheduleSort();
      toastService.success({
        title: 'Skill Updated',
        msg: `${displayName} ${newEnabled ? 'enabled' : 'disabled'} for this session`,
      });
    },
    [isHubView, togglingKey, sessionOverrides]
  );

  const entries = useMemo((): SkillEntry[] => {
    const hubOverrides = getSkillOverrides();
    const resolveEnabled = (key: string): boolean => {
      if (!isHubView && sessionOverrides.has(key)) return sessionOverrides.get(key)!;
      if (hubOverrides.has(key)) return hubOverrides.get(key)!;
      return true;
    };

    const singles: SkillEntry[] = allSkills.map((skill) => ({
      kind: 'single',
      skill,
      enabled: resolveEnabled(skill.name),
    }));

    const bundles: SkillEntry[] = allBundles.map((bundle) => ({
      kind: 'bundle',
      bundle,
      enabled: resolveEnabled(bundle.bundleName),
    }));

    return [...bundles, ...singles];
    // hubUpdateTrigger intentionally triggers re-evaluation when hub overrides change
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [allSkills, allBundles, isHubView, sessionOverrides, hubUpdateTrigger]);

  const filteredEntries = useMemo(() => {
    const q = searchQuery.toLowerCase();
    if (!q) return entries;
    return entries.filter((e) => {
      if (e.kind === 'single') {
        return (
          e.skill.name.toLowerCase().includes(q) || e.skill.description.toLowerCase().includes(q)
        );
      }
      return (
        e.bundle.bundleName.toLowerCase().includes(q) ||
        e.bundle.skills.some(
          (s) => s.name.toLowerCase().includes(q) || s.description.toLowerCase().includes(q)
        )
      );
    });
  }, [entries, searchQuery]);

  const sortedEntries = useMemo(() => {
    return [...filteredEntries].sort((a, b) => {
      if (a.enabled !== b.enabled) return a.enabled ? -1 : 1;
      const nameA = a.kind === 'single' ? a.skill.name : a.bundle.bundleName;
      const nameB = b.kind === 'single' ? b.skill.name : b.bundle.bundleName;
      return nameA.localeCompare(nameB);
    });
  }, [filteredEntries]);

  const activeCount = useMemo(() => entries.filter((e) => e.enabled).length, [entries]);

  const visibleEnabledCount = useMemo(
    () => sortedEntries.filter((e) => e.enabled).length,
    [sortedEntries]
  );

  const handleBulkToggle = useCallback(async () => {
    if (bulkInFlight || togglingKey !== null || sortedEntries.length === 0) {
      return;
    }

    const targetEnabled = visibleEnabledCount === 0;
    const targets = sortedEntries.filter((e) => e.enabled !== targetEnabled);
    if (targets.length === 0) {
      return;
    }

    setBulkInFlight(true);
    setIsTransitioning(true);

    const keys = targets.map((e) => (e.kind === 'single' ? e.skill.name : e.bundle.bundleName));

    const scheduleSort = () => {
      setPendingSort(true);
      if (sortTimeoutRef.current) clearTimeout(sortTimeoutRef.current);
      sortTimeoutRef.current = setTimeout(() => {
        setHubUpdateTrigger((prev) => prev + 1);
        setPendingSort(false);
        setIsTransitioning(false);
        setBulkInFlight(false);
      }, 800);
    };

    if (isHubView) {
      keys.forEach((k) => setSkillOverride(k, targetEnabled));
      await saveSkillOverrides();
      scheduleSort();
      toastService.success({
        title: 'Skills Updated',
        msg: `${keys.length} skill${keys.length === 1 ? '' : 's'} ${targetEnabled ? 'enabled' : 'disabled'} in new chats`,
      });
      return;
    }

    setSessionOverrides((prev) => {
      const next = new Map(prev);
      keys.forEach((k) => next.set(k, targetEnabled));
      return next;
    });
    scheduleSort();
    toastService.success({
      title: 'Skills Updated',
      msg: `${keys.length} skill${keys.length === 1 ? '' : 's'} ${targetEnabled ? 'enabled' : 'disabled'} for this session`,
    });
  }, [bulkInFlight, togglingKey, sortedEntries, visibleEnabledCount, isHubView]);

  return (
    <DropdownMenu
      open={isOpen}
      onOpenChange={(open) => {
        setIsOpen(open);
        if (!open) {
          setSearchQuery('');
          if (sortTimeoutRef.current) clearTimeout(sortTimeoutRef.current);
          setIsTransitioning(false);
          setPendingSort(false);
          setTogglingKey(null);
          setBulkInFlight(false);
        }
      }}
    >
      <DropdownMenuTrigger asChild>
        <button
          className="flex items-center cursor-pointer [&_svg]:size-4 text-text-default/70 hover:text-text-default hover:scale-100 hover:bg-transparent text-xs"
          title="manage skills"
        >
          <Layers className="mr-1 h-4 w-4" />
          <span>{activeCount}</span>
        </button>
      </DropdownMenuTrigger>
      <DropdownMenuContent
        side="top"
        align="center"
        className="w-64"
        onCloseAutoFocus={(e) => e.preventDefault()}
      >
        <div className="p-2">
          <Input
            type="text"
            placeholder="search skills..."
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            className="h-8 text-sm"
            autoFocus
          />
          {sortedEntries.length > 0 && (
            <button
              type="button"
              onClick={handleBulkToggle}
              disabled={bulkInFlight || togglingKey !== null}
              className="mt-1.5 text-xs text-text-default/70 hover:text-text-default underline-offset-2 hover:underline disabled:opacity-50 disabled:cursor-not-allowed cursor-pointer"
            >
              {visibleEnabledCount === 0
                ? `Enable all (${sortedEntries.length})`
                : `Disable all (${visibleEnabledCount})`}
            </button>
          )}
        </div>
        <div
          className={`max-h-[400px] overflow-y-auto transition-opacity duration-300 ${
            isTransitioning && pendingSort ? 'opacity-50' : 'opacity-100'
          }`}
        >
          {sortedEntries.length === 0 ? (
            <div className="px-2 py-4 text-center text-sm text-text-default/70">
              {searchQuery ? 'no skills found' : 'no skills available'}
            </div>
          ) : (
            sortedEntries.map((entry) => {
              if (entry.kind === 'single') {
                const { skill, enabled } = entry;
                const isToggling = togglingKey === skill.name;
                const rowDisabled = isToggling || bulkInFlight;
                return (
                  <div
                    key={skill.folderPath}
                    className={`flex items-center justify-between px-2 py-2 hover:bg-background-medium transition-all duration-300 ${
                      rowDisabled ? 'cursor-wait opacity-70' : 'cursor-pointer'
                    }`}
                    onClick={() => !rowDisabled && handleToggle(skill.name, skill.name)}
                    title={skill.description || skill.name}
                  >
                    <div className="text-sm font-medium text-text-default">{skill.name}</div>
                    <div onClick={(e) => e.stopPropagation()}>
                      <Switch
                        checked={enabled}
                        onCheckedChange={() => handleToggle(skill.name, skill.name)}
                        variant="mono"
                        disabled={rowDisabled}
                      />
                    </div>
                  </div>
                );
              }

              // Bundle entry
              const { bundle, enabled } = entry;
              const isToggling = togglingKey === bundle.bundleName;
              const rowDisabled = isToggling || bulkInFlight;
              const subNames = bundle.skills.map((s) => s.name).join(', ');
              return (
                <div
                  key={bundle.folderPath}
                  className={`flex items-start justify-between px-2 py-2 hover:bg-background-medium transition-all duration-300 ${
                    rowDisabled ? 'cursor-wait opacity-70' : 'cursor-pointer'
                  }`}
                  onClick={() => !rowDisabled && handleToggle(bundle.bundleName, bundle.bundleName)}
                  title={`Bundle: ${subNames}`}
                >
                  <div className="flex-1 min-w-0 pr-2">
                    <div className="text-sm font-medium text-text-default">
                      {bundle.bundleName}
                      <span className="ml-1 text-[11px] text-text-subtle font-normal">bundle</span>
                    </div>
                    <div className="text-[11px] text-text-subtle truncate">{subNames}</div>
                  </div>
                  <div onClick={(e) => e.stopPropagation()} className="flex-shrink-0 mt-0.5">
                    <Switch
                      checked={enabled}
                      onCheckedChange={() => handleToggle(bundle.bundleName, bundle.bundleName)}
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
