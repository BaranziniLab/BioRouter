import { Skill, BIOROUTER_SKILLS_DIR, isBuiltinSkill } from './skillUtils';
import { Button } from '../ui/button';
import { Switch } from '../ui/switch';
import BuiltInBadge from '../ui/BuiltInBadge';
import { Share2, Trash2, FolderDot } from '../icons/app-icons';

interface SkillItemProps {
  skill: Skill;
  enabled: boolean;
  onClick: () => void;
  onDelete: () => void;
  onShare: () => void;
  onToggle: (enabled: boolean) => void;
}

export default function SkillItem({
  skill,
  enabled,
  onClick,
  onDelete,
  onShare,
  onToggle,
}: SkillItemProps) {
  const builtin = isBuiltinSkill(skill.name);
  return (
    <div className="biorouter-list-row flex items-start gap-3 px-3 py-3 group">
      <button
        type="button"
        className="flex-1 min-w-0 cursor-pointer rounded-sm text-left focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-border-focus"
        onClick={onClick}
        aria-label={`Open skill ${skill.name}`}
      >
        <div className="flex items-center gap-1.5">
          <p className="text-sm text-text-default">{skill.name}</p>
          {builtin && <BuiltInBadge />}
        </div>
        <p className="text-xs text-text-muted mt-0.5 line-clamp-1">{skill.description}</p>
        {skill.sourceDir !== BIOROUTER_SKILLS_DIR && (
          <p className="text-[11px] text-text-subtle mt-0.5 font-mono">{skill.sourceDir}</p>
        )}
      </button>
      <div className="flex items-center gap-2 flex-shrink-0 mt-0.5">
        <div
          className="flex gap-1 opacity-100 transition-opacity sm:opacity-0 sm:group-hover:opacity-100 sm:group-focus-within:opacity-100"
          onClick={(e) => e.stopPropagation()}
        >
          <Button
            variant="ghost"
            size="sm"
            className="h-7 w-7 p-0"
            onClick={() => onClick()}
            title="Open in Finder"
            aria-label={`Open ${skill.name} in Finder`}
          >
            <FolderDot className="h-3.5 w-3.5" />
          </Button>
          <Button
            variant="ghost"
            size="sm"
            className="h-7 w-7 p-0"
            onClick={() => onShare()}
            title="Copy SKILL.md to clipboard"
            aria-label={`Copy ${skill.name} SKILL.md to clipboard`}
          >
            <Share2 className="h-3.5 w-3.5" />
          </Button>
          {!builtin && (
            <Button
              variant="ghost"
              size="sm"
              className="h-7 w-7 p-0 text-text-danger hover:bg-background-danger/10"
              onClick={() => onDelete()}
              title="Delete"
              aria-label={`Delete ${skill.name}`}
            >
              <Trash2 className="h-3.5 w-3.5" />
            </Button>
          )}
        </div>
        <div onClick={(e) => e.stopPropagation()}>
          <Switch
            checked={enabled}
            onCheckedChange={onToggle}
            variant="mono"
            aria-label={`${enabled ? 'Disable' : 'Enable'} ${skill.name}`}
          />
        </div>
      </div>
    </div>
  );
}
