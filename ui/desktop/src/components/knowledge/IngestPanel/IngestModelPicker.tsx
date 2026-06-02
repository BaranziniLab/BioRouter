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
        <button className="inline-flex w-full items-center justify-between gap-2 rounded-lg border border-border-subtle bg-background-surface px-3 py-2 text-xs hover:border-border-default">
          <span className="flex min-w-0 items-center gap-2">
            <Brain className="h-3.5 w-3.5 flex-shrink-0 text-text-muted" />
            <span className="truncate">
              {value.provider} / {value.model}
            </span>
          </span>
          <span className="flex items-center gap-1 text-[10px] uppercase tracking-wide text-text-muted">
            Model
            <ChevronDown className="h-3.5 w-3.5" />
          </span>
        </button>
      </DropdownMenuTrigger>
      <DropdownMenuContent
        side="bottom"
        align="start"
        avoidCollisions={false}
        className="z-[1100] max-h-[400px] w-[var(--radix-dropdown-menu-trigger-width)] overflow-y-auto"
      >
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
