import { isContextSkill } from '../settings/contexts/contexts';
import { useCallback, useEffect, useMemo, useState, useRef } from 'react';
import { Layers } from '../icons/app-icons';
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuTrigger,
} from '../ui/dropdown-menu';
import { Input } from '../ui/input';
import { Switch } from '../ui/switch';
import {
  loadSkillOverrides,
  saveSkillOverrides,
  setSkillOverride,
  isSkillEnabled,
  getSkillOverrides,
} from '../../store/skillOverrides';
import {
  Skill,
  SkillBundle,
  ALL_SKILL_DIRS,
  loadSkillsFromDirs,
  isBuiltinSkill,
} from '../skills/skillUtils';
import { toastService } from '../../toasts';
import BuiltInBadge from '../ui/BuiltInBadge';
import { Tooltip, TooltipContent, TooltipTrigger } from '../ui/Tooltip';

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
  const [sessionOverrides, setSessionOverrides] = useState<Map<string, boolean>>(new Map());
  const saveQueueRef = useRef<Promise<void>>(Promise.resolve());
  const saveGenerationRef = useRef(0);
  const isHubView = !sessionId;

  const loadAll = useCallback(() => {
    return loadSkillOverrides().then(() => {
      return loadSkillsFromDirs(ALL_SKILL_DIRS).then(({ singles, bundles }) => {
        // ⚠ Contexts ship with the app, so they are not what a user means by
        // "skills enabled". Filtering here fixes the chip count, the
        // Enable/Disable-all counts and all four toasts at once, because every
        // one of them derives from this array. Filtering inside
        // `loadSkillsFromDirs` instead would silently change the workflow
        // pickers too, which share that helper.
        setAllSkills(singles.filter((skill) => !isContextSkill(skill.name)));
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

  const persistOverrides = useCallback(() => {
    const generation = ++saveGenerationRef.current;
    const save = saveQueueRef.current.catch(() => undefined).then(() => saveSkillOverrides());
    saveQueueRef.current = save;
    void save.catch(async () => {
      toastService.error({
        title: 'Skill Update Failed',
        msg: 'The skill preference could not be saved.',
      });
      if (generation !== saveGenerationRef.current) return;
      const restored = await loadSkillOverrides();
      if (restored && generation === saveGenerationRef.current) {
        setHubUpdateTrigger((prev) => prev + 1);
      }
    });
  }, []);

  const handleToggle = useCallback(
    (key: string, displayName: string) => {
      if (isHubView) {
        const currentEnabled = isSkillEnabled(key);
        setSkillOverride(key, !currentEnabled);
        setHubUpdateTrigger((prev) => prev + 1);
        persistOverrides();
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
      toastService.success({
        title: 'Skill Updated',
        msg: `${displayName} ${newEnabled ? 'enabled' : 'disabled'} for this session`,
      });
    },
    [isHubView, persistOverrides, sessionOverrides]
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

  const handleBulkToggle = useCallback(() => {
    if (sortedEntries.length === 0) {
      return;
    }

    const targetEnabled = visibleEnabledCount === 0;
    const targets = sortedEntries.filter((e) => e.enabled !== targetEnabled);
    if (targets.length === 0) {
      return;
    }

    const keys = targets.map((e) => (e.kind === 'single' ? e.skill.name : e.bundle.bundleName));

    if (isHubView) {
      keys.forEach((k) => setSkillOverride(k, targetEnabled));
      setHubUpdateTrigger((prev) => prev + 1);
      persistOverrides();
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
    toastService.success({
      title: 'Skills Updated',
      msg: `${keys.length} skill${keys.length === 1 ? '' : 's'} ${targetEnabled ? 'enabled' : 'disabled'} for this session`,
    });
  }, [isHubView, persistOverrides, sortedEntries, visibleEnabledCount]);

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
              // `text-supporting`, not the `text-secondary` main.css prescribes
              // for a dense control — the override is explained once in
              // ChatInput.tsx, search "THE RAILS' TYPE".
              className="flex h-7 items-center rounded-md px-0.5 cursor-pointer [&_svg]:size-4 text-text-default/70 hover:bg-background-medium hover:text-text-default text-supporting"
              aria-label={`Manage skills (${activeCount} enabled)`}
            >
              <Layers className="mr-0.5 h-4 w-4" />
              <span>{activeCount}</span>
            </button>
          </DropdownMenuTrigger>
        </TooltipTrigger>
        <TooltipContent side="top">Manage skills</TooltipContent>
      </Tooltip>
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
            className="h-8"
            autoFocus
          />
          {sortedEntries.length > 0 && (
            <button
              type="button"
              onClick={handleBulkToggle}
              className="mt-1.5 cursor-pointer text-supporting text-text-default/70 underline-offset-2 hover:text-text-default hover:underline"
            >
              {visibleEnabledCount === 0
                ? `Enable all (${sortedEntries.length})`
                : `Disable all (${visibleEnabledCount})`}
            </button>
          )}
        </div>
        <div className="max-h-[400px] overflow-y-auto">
          {sortedEntries.length === 0 ? (
            <div className="px-3 py-4 text-center text-secondary text-text-muted">
              {searchQuery ? 'no skills found' : 'no skills available'}
            </div>
          ) : (
            sortedEntries.map((entry) => {
              if (entry.kind === 'single') {
                const { skill, enabled } = entry;
                return (
                  <DropdownMenuCheckboxItem
                    key={skill.folderPath}
                    checked={enabled}
                    showIndicator={false}
                    onCheckedChange={() => handleToggle(skill.name, skill.name)}
                    onSelect={(event) => event.preventDefault()}
                    className="flex cursor-pointer items-center justify-between px-2 py-2 transition-colors duration-[var(--motion-fast)] hover:bg-background-medium"
                    title={skill.description || skill.name}
                  >
                    <div className="flex items-center gap-1.5 min-w-0 pr-2">
                      <div className="font-medium text-text-default truncate">
                        {skill.name}
                      </div>
                      {isBuiltinSkill(skill.name) && <BuiltInBadge />}
                    </div>
                    <div className="pointer-events-none" aria-hidden="true">
                      <Switch checked={enabled} variant="mono" tabIndex={-1} aria-hidden="true" />
                    </div>
                  </DropdownMenuCheckboxItem>
                );
              }

              // Bundle entry
              const { bundle, enabled } = entry;
              const subNames = bundle.skills.map((s) => s.name).join(', ');
              return (
                <DropdownMenuCheckboxItem
                  key={bundle.folderPath}
                  checked={enabled}
                  showIndicator={false}
                  onCheckedChange={() => handleToggle(bundle.bundleName, bundle.bundleName)}
                  onSelect={(event) => event.preventDefault()}
                  className="flex cursor-pointer items-start justify-between px-2 py-2 transition-colors duration-[var(--motion-fast)] hover:bg-background-medium"
                  title={`Bundle: ${subNames}`}
                >
                  <div className="flex-1 min-w-0 pr-2">
                    <div className="font-medium text-text-default">
                      {bundle.bundleName}
                      <span className="ml-1 text-supporting text-text-subtle">bundle</span>
                    </div>
                    <div className="text-supporting text-text-subtle truncate">{subNames}</div>
                  </div>
                  <div className="pointer-events-none mt-0.5 flex-shrink-0" aria-hidden="true">
                    <Switch checked={enabled} variant="mono" tabIndex={-1} aria-hidden="true" />
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
