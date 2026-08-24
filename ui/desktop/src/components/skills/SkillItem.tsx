import { Button } from '../ui/button';
import { Switch } from '../ui/switch';
import BuiltInBadge from '../ui/BuiltInBadge';
import { Copy, Trash2, FolderDot } from '../icons/app-icons';
import type { CatalogSkill } from '../../api';

interface SkillItemProps {
  skill: CatalogSkill;
  enabled: boolean;
  onClick: () => void;
  /**
   * Omitted where the skill is not the user's to delete: one Biorouter ships
   * and re-seeds on every start, or one an installed extension supplies. A
   * delete that succeeds and silently reverts is worse than no button — the
   * lesson `BUILTIN_SKILL_NAMES` was written for, applied to a second case.
   */
  onDelete?: () => void;
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
  // ⚠ From the daemon, not from the hand-synced `BUILTIN_SKILL_NAMES` copy.
  // Rust owns the seeder, so Rust owns the answer.
  const builtin = skill.builtin;
  return (
    <div className="biorouter-list-row flex items-start gap-3 px-3 py-3 group">
      <button
        type="button"
        className="flex-1 min-w-0 cursor-pointer rounded-inner text-left focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-border-focus"
        onClick={onClick}
        aria-label={`Open skill ${skill.name}`}
      >
        <div className="flex items-center gap-1.5 min-w-0">
          <p className="text-label text-text-default truncate">{skill.name}</p>
          {builtin && <BuiltInBadge />}
        </div>
        <p className="text-supporting text-text-muted mt-0.5 line-clamp-1">{skill.description}</p>
        {skill.source.kind !== 'biorouter' && (
          <p className="text-supporting text-text-subtle mt-0.5 font-mono truncate">
            {skill.sourceRoot}
          </p>
        )}
      </button>
      <div className="flex items-center gap-2 flex-shrink-0 mt-0.5">
        <div
          className="flex gap-1 opacity-100 transition-opacity sm:opacity-0 sm:group-hover:opacity-100 sm:group-focus-within:opacity-100"
          onClick={(e) => e.stopPropagation()}
        >
          <Button
            variant="ghost"
            shape="round"
            onClick={() => onClick()}
            title="Open in Finder"
            aria-label={`Open ${skill.name} in Finder`}
          >
            <FolderDot className="h-4 w-4" />
          </Button>
          <Button
            variant="ghost"
            shape="round"
            onClick={() => onShare()}
            title="Copy SKILL.md to clipboard"
            aria-label={`Copy ${skill.name} SKILL.md to clipboard`}
          >
            <Copy className="h-4 w-4" />
          </Button>
          {onDelete && !builtin && (
            <Button
              variant="ghost"
              size="sm"
              className="text-text-danger"
              onClick={() => onDelete()}
              title="Delete this skill"
              aria-label={`Delete ${skill.name}`}
            >
              <Trash2 className="h-4 w-4" />
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
