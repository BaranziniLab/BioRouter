import { useEffect, useState } from 'react';
import { Brain, Check, ChevronDown } from 'lucide-react';
import type { ModelRef } from '../../../api/types.gen';
import { useConfig } from '../../ConfigContext';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '../../ui/dropdown-menu';

interface Props {
  value: ModelRef;
  onChange: (v: ModelRef) => void;
}

export function IngestModelPicker({ value, onChange }: Props) {
  const { getProviders, getProviderModels } = useConfig();
  const [modelsByProvider, setModelsByProvider] = useState<Record<string, string[]>>({});
  const [providerDisplayNames, setProviderDisplayNames] = useState<Record<string, string>>({});

  useEffect(() => {
    void (async () => {
      try {
        const list = await getProviders(false);
        const configured = list.filter((p) => p.is_configured);
        const names: Record<string, string> = {};
        for (const p of configured) {
          names[p.name] = p.metadata.display_name ?? p.name;
        }
        setProviderDisplayNames(names);

        const map: Record<string, string[]> = {};
        for (const p of configured) {
          try {
            const models = await getProviderModels(p.name);
            if (models.length > 0) map[p.name] = models;
          } catch {
            // ignore per-provider fetch failures
          }
        }
        setModelsByProvider(map);
      } catch (err) {
        console.warn('IngestModelPicker: failed to load providers', err);
      }
    })();
  }, [getProviders, getProviderModels]);

  const providerNames = Object.keys(modelsByProvider);

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <button className="inline-flex items-center gap-1.5 px-2 py-1 rounded-md border border-border-subtle bg-background-surface text-xs hover:border-border-default">
          <Brain className="w-3 h-3 text-text-muted" />
          <span className="truncate max-w-[200px]">
            {value.provider} / {value.model}
          </span>
          <ChevronDown className="w-3 h-3 text-text-muted" />
        </button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="max-h-[400px] overflow-y-auto">
        <DropdownMenuLabel className="text-[10px] uppercase tracking-wide">
          Model for ingest
        </DropdownMenuLabel>
        <DropdownMenuSeparator />
        {providerNames.length === 0 && (
          <div className="px-3 py-2 text-xs text-text-muted">No providers configured.</div>
        )}
        {providerNames.map((providerName) => {
          const models = modelsByProvider[providerName] ?? [];
          return (
            <div key={providerName}>
              <DropdownMenuLabel className="text-[10px] text-text-muted">
                {providerDisplayNames[providerName] ?? providerName}
              </DropdownMenuLabel>
              {models.map((m) => {
                const selected = value.provider === providerName && value.model === m;
                return (
                  <DropdownMenuItem
                    key={m}
                    onClick={() => onChange({ provider: providerName, model: m })}
                  >
                    <span className="flex-1 truncate">{m}</span>
                    {selected && <Check className="w-3 h-3" />}
                  </DropdownMenuItem>
                );
              })}
              <DropdownMenuSeparator />
            </div>
          );
        })}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
