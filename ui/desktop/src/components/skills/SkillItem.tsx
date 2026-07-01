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
    <div
      className="biorouter-list-row flex cursor-pointer items-start gap-3 px-3 py-3 group"
      onClick={onClick}
    >
      <div className="flex-1 min-w-0">
        <div className="flex items-center gap-1.5">
          <p className="text-sm text-text-default">{skill.name}</p>
          {builtin && <BuiltInBadge />}
        </div>
        <p className="text-xs text-text-muted mt-0.5 line-clamp-1">{skill.description}</p>
        {skill.sourceDir !== BIOROUTER_SKILLS_DIR && (
          <p className="text-[11px] text-text-subtle mt-0.5 font-mono">{skill.sourceDir}</p>
        )}
      </div>
      <div className="flex items-center gap-2 flex-shrink-0 mt-0.5">
        <div
          className="flex gap-1 opacity-0 group-hover:opacity-100 transition-opacity"
          onClick={(e) => e.stopPropagation()}
        >
          <Button
            variant="ghost"
            size="sm"
            className="h-7 w-7 p-0"
            onClick={() => onClick()}
            title="Open in Finder"
          >
            <FolderDot className="h-3.5 w-3.5" />
          </Button>
          <Button
            variant="ghost"
            size="sm"
            className="h-7 w-7 p-0"
            onClick={() => onShare()}
            title="Copy SKILL.md to clipboard"
          >
            <Share2 className="h-3.5 w-3.5" />
          </Button>
          {!builtin && (
            <Button
              variant="ghost"
              size="sm"
              className="h-7 w-7 p-0 text-destructive hover:text-destructive hover:bg-destructive/10"
              onClick={() => onDelete()}
              title="Delete"
            >
              <Trash2 className="h-3.5 w-3.5" />
            </Button>
          )}
        </div>
        <div onClick={(e) => e.stopPropagation()}>
          <Switch checked={enabled} onCheckedChange={onToggle} variant="mono" />
        </div>
      </div>
    </div>
  );
}
