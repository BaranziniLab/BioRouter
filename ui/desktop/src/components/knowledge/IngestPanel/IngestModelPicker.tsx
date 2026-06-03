import { useEffect, useState } from 'react';
import { Brain, Check, ChevronDown } from 'lucide-react';
import type { ModelRef } from '../../../api/types.gen';
import { useConfig } from '../../ConfigContext';
import { Button } from '../../ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '../../ui/dialog';

interface Props {
  value: ModelRef;
  onChange: (v: ModelRef) => void;
}

export function IngestModelPicker({ value, onChange }: Props) {
  const { getProviders, getProviderModels } = useConfig();
  const [open, setOpen] = useState(false);
  const [modelsByProvider, setModelsByProvider] = useState<Record<string, string[]>>({});
  const [providerDisplayNames, setProviderDisplayNames] = useState<Record<string, string>>({});

  useEffect(() => {
    void (async () => {
      try {
        const list = await getProviders(false);
        const configured = list.filter((provider) => provider.is_configured);
        const names: Record<string, string> = {};
        for (const provider of configured) {
          names[provider.name] = provider.metadata.display_name ?? provider.name;
        }
        setProviderDisplayNames(names);

        const map: Record<string, string[]> = {};
        for (const provider of configured) {
          try {
            const models = await getProviderModels(provider.name);
            if (models.length > 0) {
              map[provider.name] = models;
            }
          } catch {
            // Ignore per-provider fetch failures so one misconfigured provider
            // does not blank the whole picker.
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
    <>
      <button
        data-testid="knowledge-model-picker-trigger"
        type="button"
        onClick={() => setOpen(true)}
        className="inline-flex w-full items-center justify-between gap-2 rounded-xl border border-border-subtle bg-background-surface px-3 py-2 text-xs transition-colors hover:border-border-default hover:bg-background-default"
      >
        <span className="flex min-w-0 items-center gap-2">
          <Brain className="h-3.5 w-3.5 shrink-0 text-text-muted" />
          <span className="min-w-0 truncate">
            {value.provider} / {value.model}
          </span>
        </span>
        <span className="flex shrink-0 items-center gap-1 text-[10px] uppercase tracking-wide text-text-muted">
          Model
          <ChevronDown className="h-3.5 w-3.5" />
        </span>
      </button>

      <Dialog open={open} onOpenChange={setOpen}>
        <DialogContent className="max-h-[80vh] overflow-hidden p-0 sm:max-w-[760px]">
          <DialogHeader className="px-6 pt-6 pb-0">
            <DialogTitle>Choose ingest model</DialogTitle>
            <DialogDescription>
              Pick the model BioRouter should use for knowledge-base digestion. This chooser stays
              scrollable even in smaller windows.
            </DialogDescription>
          </DialogHeader>

          <div className="overflow-y-auto px-6 py-4">
            {providerNames.length === 0 && (
              <div className="rounded-xl border border-border-subtle bg-background-surface px-4 py-3 text-sm text-text-muted">
                No providers are configured yet.
              </div>
            )}

            <div className="space-y-4">
              {providerNames.map((providerName) => {
                const models = modelsByProvider[providerName] ?? [];
                return (
                  <section key={providerName} className="space-y-2">
                    <div className="text-[11px] font-medium uppercase tracking-[0.18em] text-text-muted">
                      {providerDisplayNames[providerName] ?? providerName}
                    </div>
                    <div className="grid gap-2 sm:grid-cols-2">
                      {models.map((model) => {
                        const selected = value.provider === providerName && value.model === model;
                        return (
                          <button
                            key={model}
                            type="button"
                            onClick={() => {
                              onChange({ provider: providerName, model });
                              setOpen(false);
                            }}
                            className={`flex items-center gap-3 rounded-xl border px-3 py-3 text-left transition-colors ${
                              selected
                                ? 'border-border-default bg-background-default'
                                : 'border-border-subtle bg-background-surface hover:border-border-default hover:bg-background-default'
                            }`}
                          >
                            <Brain className="h-4 w-4 shrink-0 text-text-muted" />
                            <span className="min-w-0 flex-1">
                              <span className="block truncate text-sm font-medium">{model}</span>
                              <span className="block text-[11px] text-text-muted">
                                {providerDisplayNames[providerName] ?? providerName}
                              </span>
                            </span>
                            {selected && <Check className="h-4 w-4 shrink-0 text-text-default" />}
                          </button>
                        );
                      })}
                    </div>
                  </section>
                );
              })}
            </div>
          </div>

          <div className="border-t border-border-subtle px-6 py-3">
            <Button type="button" variant="outline" size="sm" onClick={() => setOpen(false)}>
              Close
            </Button>
          </div>
        </DialogContent>
      </Dialog>
    </>
  );
}
