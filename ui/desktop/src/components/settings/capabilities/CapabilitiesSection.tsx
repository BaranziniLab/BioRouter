import { useCallback, useMemo, useState } from 'react';
import { Switch } from '../../ui/switch';
import { useConfig, FixedExtensionEntry } from '../../ConfigContext';
import { nameToKey } from '../extensions/utils';
import { toggleExtensionDefault } from '../extensions';
import { CAPABILITIES, CapabilityMeta } from './capabilities';

interface CapabilityItemProps {
  meta: CapabilityMeta;
  entry?: FixedExtensionEntry;
  onToggle: (entry: FixedExtensionEntry) => Promise<void>;
}

function CapabilityItem({ meta, entry, onToggle }: CapabilityItemProps) {
  const [isToggling, setIsToggling] = useState(false);
  // Treat a not-yet-loaded capability as enabled — they are on by default.
  const enabled = entry ? entry.enabled : true;

  const handleToggle = async () => {
    if (!entry || isToggling) return;
    setIsToggling(true);
    try {
      await onToggle(entry);
    } finally {
      setIsToggling(false);
    }
  };

  return (
    <div
      className={`flex items-center justify-between text-text-default py-3 px-2 -mx-2 rounded-lg transition-colors duration-150 ${
        enabled ? 'bg-background-medium' : 'hover:bg-background-medium'
      }`}
    >
      <div className="flex">
        <div>
          <p className="text-sm font-medium text-text-default">{meta.label}</p>
          <p className="text-xs text-text-muted mt-0.5">{meta.description}</p>
        </div>
      </div>

      <div className="relative flex items-center gap-2 flex-shrink-0">
        <Switch
          checked={enabled}
          onCheckedChange={handleToggle}
          disabled={!entry || isToggling}
          variant="mono"
          aria-label={`Toggle ${meta.label} capability`}
        />
      </div>
    </div>
  );
}

export const CapabilitiesSection = () => {
  const { extensionsList, addExtension, getExtensions } = useConfig();

  const entriesByKey = useMemo(() => {
    const map = new Map<string, FixedExtensionEntry>();
    for (const ext of extensionsList) {
      map.set(nameToKey(ext.name), ext);
    }
    return map;
  }, [extensionsList]);

  const handleToggle = useCallback(
    async (entry: FixedExtensionEntry) => {
      await toggleExtensionDefault({
        toggle: entry.enabled ? 'toggleOff' : 'toggleOn',
        extensionConfig: entry,
        addToConfig: addExtension,
      });
      await getExtensions(true);
    },
    [addExtension, getExtensions]
  );

  return (
    <div className="space-y-1">
      {CAPABILITIES.map((meta) => (
        <CapabilityItem
          key={meta.key}
          meta={meta}
          entry={entriesByKey.get(meta.key)}
          onToggle={handleToggle}
        />
      ))}
    </div>
  );
};
