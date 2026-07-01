import { useState, useEffect } from 'react';
import kebabCase from 'lodash/kebabCase';
import { Switch } from '../../../ui/switch';
import { Gear } from '../../../icons';
import { FixedExtensionEntry } from '../../../ConfigContext';
import BuiltInBadge from '../../../ui/BuiltInBadge';
import { getSubtitle, getFriendlyTitle, isBuiltInExtension } from './ExtensionList';

interface ExtensionItemProps {
  extension: FixedExtensionEntry;
  onToggle: (extension: FixedExtensionEntry) => Promise<boolean | void> | void;
  onConfigure?: (extension: FixedExtensionEntry) => void;
  isStatic?: boolean;
}

export default function ExtensionItem({
  extension,
  onToggle,
  onConfigure,
  isStatic,
}: ExtensionItemProps) {
  const [visuallyEnabled, setVisuallyEnabled] = useState(extension.enabled);
  const [isToggling, setIsToggling] = useState(false);

  const handleToggle = async (ext: FixedExtensionEntry) => {
    if (isToggling) return;
    setIsToggling(true);
    const newState = !ext.enabled;
    setVisuallyEnabled(newState);
    try {
      await onToggle(ext);
    } catch {
      setVisuallyEnabled(!newState);
    } finally {
      setIsToggling(false);
    }
  };

  useEffect(() => {
    if (!isToggling) {
      setVisuallyEnabled(extension.enabled);
    }
  }, [extension.enabled, isToggling]);

  const { description, command } = getSubtitle(extension);

  const editable =
    !isStatic && (extension.type === 'builtin' || !('bundled' in extension && extension.bundled));

  return (
    <div
      id={`extension-${kebabCase(extension.name)}`}
      className="biorouter-list-row flex items-center gap-4 px-3 py-3 group"
    >
      <div className="flex-1 min-w-0">
        <div className="flex items-center gap-1.5">
          <p className="text-sm font-medium text-text-default leading-snug">
            {getFriendlyTitle(extension)}
          </p>
          {isBuiltInExtension(extension) && <BuiltInBadge />}
        </div>
        {description && (
          <p className="text-xs text-text-muted mt-0.5 line-clamp-1">{description}</p>
        )}
        {command && <p className="text-xs font-mono text-text-muted mt-0.5 truncate">{command}</p>}
      </div>
      <div className="flex items-center gap-2 flex-shrink-0">
        {editable && (
          <button
            className="opacity-0 group-hover:opacity-100 transition-opacity text-text-muted hover:text-text-default"
            aria-label={`Configure ${getFriendlyTitle(extension)} extension`}
            onClick={() => onConfigure?.(extension)}
          >
            <Gear className="w-4 h-4" />
          </button>
        )}
        <Switch
          checked={(isToggling && visuallyEnabled) || extension.enabled}
          onCheckedChange={() => handleToggle(extension)}
          disabled={isToggling}
          variant="mono"
          aria-label={`Toggle ${getFriendlyTitle(extension)} extension`}
        />
      </div>
    </div>
  );
}
