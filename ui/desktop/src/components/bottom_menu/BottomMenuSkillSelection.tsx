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
import { Skill, ALL_SKILL_DIRS, loadSkillsFromDirs } from '../skills/skillUtils';
import { toastService } from '../../toasts';

interface BottomMenuSkillSelectionProps {
  sessionId: string | null;
}

export const BottomMenuSkillSelection = ({ sessionId }: BottomMenuSkillSelectionProps) => {
  const [searchQuery, setSearchQuery] = useState('');
  const [isOpen, setIsOpen] = useState(false);
  const [allSkills, setAllSkills] = useState<Skill[]>([]);
  const [hubUpdateTrigger, setHubUpdateTrigger] = useState(0);
  const [isTransitioning, setIsTransitioning] = useState(false);
  const [pendingSort, setPendingSort] = useState(false);
  const [togglingSkill, setTogglingSkill] = useState<string | null>(null);
  // Per-session overrides (local only, no backend API)
  const [sessionOverrides, setSessionOverrides] = useState<Map<string, boolean>>(new Map());
  const sortTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const isHubView = !sessionId;

  useEffect(() => {
    loadSkillOverrides().then(() => {
      loadSkillsFromDirs(ALL_SKILL_DIRS).then(setAllSkills);
    });
  }, []);

  useEffect(() => {
    if (isOpen) {
      loadSkillOverrides().then(() => {
        loadSkillsFromDirs(ALL_SKILL_DIRS).then(setAllSkills);
        setHubUpdateTrigger((prev) => prev + 1);
      });
    }
  }, [isOpen]);

  useEffect(() => {
    return () => {
      if (sortTimeoutRef.current) clearTimeout(sortTimeoutRef.current);
    };
  }, []);

  const handleToggle = useCallback(
    async (skill: Skill) => {
      if (togglingSkill === skill.name) return;

      setIsTransitioning(true);
      setTogglingSkill(skill.name);

      if (isHubView) {
        const currentEnabled = isSkillEnabled(skill.name);
        setSkillOverride(skill.name, !currentEnabled);
        await saveSkillOverrides();
        setPendingSort(true);
        if (sortTimeoutRef.current) clearTimeout(sortTimeoutRef.current);
        sortTimeoutRef.current = setTimeout(() => {
          setHubUpdateTrigger((prev) => prev + 1);
          setPendingSort(false);
          setIsTransitioning(false);
          setTogglingSkill(null);
        }, 800);
        toastService.success({
          title: 'Skill Updated',
          msg: `${skill.name} will be ${!currentEnabled ? 'enabled' : 'disabled'} in new chats`,
        });
        return;
      }

      // Session view: local state only
      const currentEnabled = sessionOverrides.has(skill.name)
        ? sessionOverrides.get(skill.name)!
        : isSkillEnabled(skill.name);
      const newEnabled = !currentEnabled;
      setSessionOverrides((prev) => {
        const next = new Map(prev);
        next.set(skill.name, newEnabled);
        return next;
      });
      setPendingSort(true);
      if (sortTimeoutRef.current) clearTimeout(sortTimeoutRef.current);
      sortTimeoutRef.current = setTimeout(() => {
        setPendingSort(false);
        setIsTransitioning(false);
        setTogglingSkill(null);
      }, 800);
      toastService.success({
        title: 'Skill Updated',
        msg: `${skill.name} ${newEnabled ? 'enabled' : 'disabled'} for this session`,
      });
    },
    [isHubView, togglingSkill, sessionOverrides]
  );

  const skillsList = useMemo(() => {
    const hubOverrides = getSkillOverrides();
    return allSkills.map((skill) => {
      let enabled: boolean;
      if (!isHubView && sessionOverrides.has(skill.name)) {
        enabled = sessionOverrides.get(skill.name)!;
      } else if (hubOverrides.has(skill.name)) {
        enabled = hubOverrides.get(skill.name)!;
      } else {
        enabled = true; // default enabled
      }
      return { ...skill, enabled };
    });
    // hubUpdateTrigger intentionally triggers re-evaluation when hub overrides change
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [allSkills, isHubView, sessionOverrides, hubUpdateTrigger]);

  const filteredSkills = useMemo(() => {
    const q = searchQuery.toLowerCase();
    return skillsList.filter(
      (s) => s.name.toLowerCase().includes(q) || s.description.toLowerCase().includes(q)
    );
  }, [skillsList, searchQuery]);

  const sortedSkills = useMemo(() => {
    return [...filteredSkills].sort((a, b) => {
      if (a.enabled !== b.enabled) return a.enabled ? -1 : 1;
      return a.name.localeCompare(b.name);
    });
  }, [filteredSkills]);

  const activeCount = useMemo(() => skillsList.filter((s) => s.enabled).length, [skillsList]);

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
          setTogglingSkill(null);
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
          <p className="text-xs text-text-default/60 mt-1.5">
            {isHubView ? 'Skills for new chats' : 'Skills for this chat session'}
          </p>
        </div>
        <div
          className={`max-h-[400px] overflow-y-auto transition-opacity duration-300 ${
            isTransitioning && pendingSort ? 'opacity-50' : 'opacity-100'
          }`}
        >
          {sortedSkills.length === 0 ? (
            <div className="px-2 py-4 text-center text-sm text-text-default/70">
              {searchQuery ? 'no skills found' : 'no skills available'}
            </div>
          ) : (
            sortedSkills.map((skill) => {
              const isToggling = togglingSkill === skill.name;
              return (
                <div
                  key={skill.folderPath}
                  className={`flex items-center justify-between px-2 py-2 hover:bg-background-medium transition-all duration-300 ${
                    isToggling ? 'cursor-wait opacity-70' : 'cursor-pointer'
                  }`}
                  onClick={() => !isToggling && handleToggle(skill)}
                  title={skill.description || skill.name}
                >
                  <div className="text-sm font-medium text-text-default">{skill.name}</div>
                  <div onClick={(e) => e.stopPropagation()}>
                    <Switch
                      checked={skill.enabled}
                      onCheckedChange={() => handleToggle(skill)}
                      variant="mono"
                      disabled={isToggling}
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
