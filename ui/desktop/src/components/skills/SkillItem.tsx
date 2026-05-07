import { Skill, BIOROUTER_SKILLS_DIR } from './skillUtils';
import { Button } from '../ui/button';
import { Share2, Trash2, FolderDot } from '../icons/app-icons';

interface SkillItemProps {
  skill: Skill;
  onClick: () => void;
  onDelete: () => void;
  onShare: () => void;
}

export default function SkillItem({ skill, onClick, onDelete, onShare }: SkillItemProps) {
  return (
    <div
      className="flex items-start py-3 border-b border-border-subtle last:border-b-0 hover:bg-background-medium/30 transition-colors group cursor-pointer gap-3 px-2"
      onClick={onClick}
    >
      <div className="flex-1 min-w-0">
        <p className="text-sm font-semibold text-text-default">{skill.name}</p>
        <p className="text-xs text-text-muted mt-0.5 line-clamp-1">{skill.description}</p>
        {skill.sourceDir !== BIOROUTER_SKILLS_DIR && (
          <p className="text-[11px] text-text-subtle mt-0.5 font-mono">{skill.sourceDir}</p>
        )}
      </div>
      <div className="flex gap-1 opacity-0 group-hover:opacity-100 transition-opacity flex-shrink-0 mt-0.5">
        <Button
          variant="ghost"
          size="sm"
          className="h-7 w-7 p-0"
          onClick={(e) => { e.stopPropagation(); onClick(); }}
          title="Open in Finder"
        >
          <FolderDot className="h-3.5 w-3.5" />
        </Button>
        <Button
          variant="ghost"
          size="sm"
          className="h-7 w-7 p-0"
          onClick={(e) => { e.stopPropagation(); onShare(); }}
          title="Copy SKILL.md to clipboard"
        >
          <Share2 className="h-3.5 w-3.5" />
        </Button>
        <Button
          variant="ghost"
          size="sm"
          className="h-7 w-7 p-0 text-destructive hover:text-destructive hover:bg-destructive/10"
          onClick={(e) => { e.stopPropagation(); onDelete(); }}
          title="Delete"
        >
          <Trash2 className="h-3.5 w-3.5" />
        </Button>
      </div>
    </div>
  );
}
