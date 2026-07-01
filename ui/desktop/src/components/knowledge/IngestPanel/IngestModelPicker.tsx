import { useEffect, useMemo, useState } from 'react';
import { Brain, Check, ChevronDown } from 'lucide-react';
import type { ProviderDetails } from '../../../api';
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
import { fetchModelsForProviders } from '../../settings/models/modelInterface';
import {
  getOrderedProviderGroups,
  type OrderedProviderGroup,
} from '../../settings/providers/providerOrdering';

interface Props {
  value: ModelRef;
  onChange: (v: ModelRef) => void;
}

interface ProviderModelsSection extends OrderedProviderGroup {
  modelsByProvider: Record<string, string[]>;
}

export function IngestModelPicker({ value, onChange }: Props) {
  const { getProviders, getProviderModels } = useConfig();
  const [open, setOpen] = useState(false);
  const [sections, setSections] = useState<ProviderModelsSection[]>([]);
  const [providerDisplayNames, setProviderDisplayNames] = useState<Record<string, string>>({});

  useEffect(() => {
    void (async () => {
      try {
        const providers = await getProviders(false);
        const configuredProviders = providers.filter((provider) => provider.is_configured);
        const names = configuredProviders.reduce<Record<string, string>>((acc, provider) => {
          acc[provider.name] = provider.metadata.display_name ?? provider.name;
          return acc;
        }, {});
        setProviderDisplayNames(names);

        const modelResults = await fetchModelsForProviders(configuredProviders, getProviderModels);
        const availableModelsByProvider = modelResults.reduce<Record<string, string[]>>(
          (acc, { provider, models }) => {
            const availableModels = (models ?? []).filter(Boolean);
            if (availableModels.length > 0) {
              acc[provider.name] = availableModels;
            }
            return acc;
          },
          {}
        );

        const availableProviders = configuredProviders.filter(
          (provider) => (availableModelsByProvider[provider.name] ?? []).length > 0
        );
        const orderedSections = getOrderedProviderGroups(availableProviders)
          .map((section) => {
            const sectionProviders = section.providers.filter(
              (provider) => (availableModelsByProvider[provider.name] ?? []).length > 0
            );
            return {
              ...section,
              providers: sectionProviders,
              modelsByProvider: sectionProviders.reduce<Record<string, string[]>>(
                (acc, provider) => {
                  acc[provider.name] = availableModelsByProvider[provider.name] ?? [];
                  return acc;
                },
                {}
              ),
            };
          })
          .filter((section) => section.providers.length > 0);

        setSections(orderedSections);
      } catch (err) {
        console.warn('IngestModelPicker: failed to load providers', err);
        setSections([]);
      }
    })();
  }, [getProviderModels, getProviders]);

  const hasModels = sections.length > 0;
  const currentProviderLabel = providerDisplayNames[value.provider] ?? value.provider;
  const selectedProvider = useMemo(
    () =>
      sections
        .flatMap((section) => section.providers)
        .find((provider) => provider.name === value.provider) ?? null,
    [sections, value.provider]
  );

  function renderProvider(provider: ProviderDetails, section: ProviderModelsSection) {
    const models = section.modelsByProvider[provider.name] ?? [];
    if (models.length === 0) {
      return null;
    }

    return (
      <section key={provider.name} className="space-y-2">
        <div className="text-[11px] font-medium uppercase tracking-wider text-text-muted">
          {providerDisplayNames[provider.name] ?? provider.name}
        </div>
        <div className="grid gap-2 sm:grid-cols-2">
          {models.map((model) => {
            const selected = value.provider === provider.name && value.model === model;
            return (
              <button
                key={`${provider.name}:${model}`}
                type="button"
                onClick={() => {
                  onChange({ provider: provider.name, model });
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
                    {providerDisplayNames[provider.name] ?? provider.name}
                  </span>
                </span>
                {selected && <Check className="h-4 w-4 shrink-0 text-text-default" />}
              </button>
            );
          })}
        </div>
      </section>
    );
  }

  return (
    <>
      <button
        data-testid="knowledge-model-picker-trigger"
        type="button"
        onClick={() => setOpen(true)}
        className="inline-flex min-h-10 w-full items-center justify-between gap-2 rounded-xl border border-border-subtle bg-background-default/85 px-3 py-2 text-xs shadow-[0_4px_14px_-14px_rgba(32,25,15,0.32)] transition-colors hover:border-border-default hover:bg-background-default focus:outline-none focus:ring-1 focus:ring-ring"
      >
        <span className="flex min-w-0 items-center gap-2">
          <Brain className="h-3.5 w-3.5 shrink-0 text-text-muted" />
          <span className="min-w-0 truncate">
            {selectedProvider
              ? `${currentProviderLabel} / ${value.model}`
              : `${value.provider} / ${value.model}`}
          </span>
        </span>
        <span className="flex shrink-0 items-center gap-1 text-[11px] uppercase tracking-wider text-text-muted">
          Model
          <ChevronDown className="h-3.5 w-3.5" />
        </span>
      </button>

      <Dialog open={open} onOpenChange={setOpen}>
        <DialogContent className="flex max-h-[80vh] flex-col overflow-hidden p-0 sm:max-w-[760px]">
          <DialogHeader className="px-6 pt-6 pb-0">
            <DialogTitle>Choose ingest model</DialogTitle>
            <DialogDescription>
              Pick from configured, available models only. The list follows the model settings
              order: institutional providers first, then local, then commercial.
            </DialogDescription>
          </DialogHeader>

          <div className="min-h-0 flex-1 overflow-y-auto px-6 py-4">
            {!hasModels && (
              <div className="rounded-xl border border-border-subtle bg-background-surface px-4 py-3 text-sm text-text-muted">
                No configured providers have available models yet.
              </div>
            )}

            <div className="space-y-6">
              {sections.map((section) => (
                <section key={section.key} className="space-y-4">
                  <div className="text-xs font-medium uppercase tracking-wider text-text-muted flex items-center gap-2">
                    <span className={`h-1.5 w-1.5 rounded-full ${section.accentClassName}`} />
                    {section.label}
                  </div>
                  <div className="space-y-4">
                    {section.providers.map((provider) => renderProvider(provider, section))}
                  </div>
                </section>
              ))}
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
