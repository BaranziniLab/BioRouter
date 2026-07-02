import { useEffect, useMemo, useState } from 'react';
import { Brain, Check, ChevronDown, LoaderCircle } from 'lucide-react';
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
  disabled?: boolean;
  saving?: boolean;
}

interface ProviderModelsSection extends OrderedProviderGroup {
  modelsByProvider: Record<string, string[]>;
}

export function IngestModelPicker({ value, onChange, disabled = false, saving = false }: Props) {
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
      <section key={provider.name} className="space-y-1.5">
        <div className="px-1 text-[11px] font-medium uppercase tracking-wider text-text-muted">
          {providerDisplayNames[provider.name] ?? provider.name}
        </div>
        <div className="space-y-0.5">
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
                className={`flex w-full items-center gap-3 rounded-lg px-3 py-2.5 text-left transition-[background-color,box-shadow] duration-150 ${
                  selected
                    ? 'bg-background-medium/82 shadow-[0_8px_20px_-22px_rgba(32,25,15,0.38)]'
                    : 'hover:bg-background-medium/62 hover:shadow-[0_8px_18px_-24px_rgba(32,25,15,0.3)]'
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
        onClick={() => {
          if (!disabled) setOpen(true);
        }}
        disabled={disabled}
        className="group inline-flex min-h-10 w-full items-center justify-between gap-2 rounded-xl bg-background-default/52 px-3 py-2 text-left transition-[background-color,box-shadow] duration-150 hover:bg-background-medium/82 hover:shadow-[0_12px_26px_-22px_rgba(32,25,15,0.42)] focus:outline-none focus:ring-1 focus:ring-ring disabled:cursor-not-allowed disabled:opacity-60"
      >
        <span className="flex min-w-0 flex-1 items-center gap-2">
          <Brain className="h-4 w-4 shrink-0 text-text-muted" />
          <span className="shrink-0 text-[11px] font-medium uppercase tracking-wider text-text-muted">
            Model
          </span>
          <span className="min-w-0 truncate text-xs font-medium text-text-default">
            {selectedProvider
              ? `${currentProviderLabel} / ${value.model}`
              : `${value.provider} / ${value.model}`}
          </span>
        </span>
        <span className="flex shrink-0 items-center gap-1.5 rounded-lg bg-background-medium/64 px-2 py-1 text-[11px] font-medium text-text-default shadow-[0_8px_18px_-18px_rgba(32,25,15,0.32)] transition-colors group-hover:bg-background-default/76">
          {saving && <LoaderCircle className="h-3.5 w-3.5 animate-spin text-text-muted" />}
          {saving ? 'Saving' : 'Set default'}
          {!saving && <ChevronDown className="h-3.5 w-3.5 text-text-muted" />}
        </span>
      </button>

      <Dialog open={open} onOpenChange={setOpen}>
        <DialogContent className="flex max-h-[82vh] flex-col overflow-hidden p-0 sm:max-w-[760px]">
          <DialogHeader className="px-6 pb-3 pt-6">
            <DialogTitle>Choose knowledge model</DialogTitle>
            <DialogDescription>
              This model digests staged sources and scheduled knowledge curation. Conversation
              replies still use the model selected in the chat composer.
            </DialogDescription>
          </DialogHeader>

          <div className="min-h-0 flex-1 overflow-y-auto px-6 py-2">
            {!hasModels && (
              <div className="rounded-lg bg-background-medium/52 px-4 py-3 text-sm text-text-muted">
                No configured providers have available models yet.
              </div>
            )}

            <div className="space-y-6">
              {sections.map((section) => (
                <section key={section.key} className="space-y-3">
                  <div className="flex items-center gap-2 px-1 text-xs font-medium uppercase tracking-wider text-text-muted">
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

          <div className="px-6 pb-5 pt-3">
            <Button type="button" variant="secondary" size="sm" onClick={() => setOpen(false)}>
              Close
            </Button>
          </div>
        </DialogContent>
      </Dialog>
    </>
  );
}
