import { useEffect, useMemo, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { Brain, Check, ChevronDown, LoaderCircle } from '../../icons/app-icons';
import type { ProviderDetails } from '../../../api';
import type { ModelRef } from '../../../api/types.gen';
import { useConfig } from '../../ConfigContext';
import { Badge } from '../../ui/badge';
import { Button } from '../../ui/button';
import { EmptyState } from '../../ui/empty-state';
import { Popover, PopoverContent, PopoverTrigger } from '../../ui/popover';
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from '../../ui/command';
import { fetchModelsForProviders } from '../../settings/models/modelInterface';
import {
  getOrderedProviderGroups,
  type OrderedProviderGroup,
} from '../../settings/providers/providerOrdering';
import { providerCanRunIngest } from './resolveIngestModel';

interface Props {
  /** `null` when no model could be resolved — the trigger says so instead of naming a vendor. */
  value: ModelRef | null;
  /**
   * Why `value` is null, when it is. "No model configured" is a verdict on the
   * user's setup, and it is wrong in all three of the other states: `loading` is
   * an answer still in flight, `unavailable` is a knowledge base whose manifest
   * could not be read, and `unsupported` is a perfectly good configuration whose
   * only model happens to be one a knowledge macro cannot drive — nothing there
   * is the user's configuration to fix.
   */
  valueState?: 'resolved' | 'loading' | 'unavailable' | 'unsupported';
  onChange: (v: ModelRef) => void;
  disabled?: boolean;
  saving?: boolean;
}

interface ProviderModelsSection extends OrderedProviderGroup {
  modelsByProvider: Record<string, string[]>;
}

/**
 * The "no configured provider has a model" state (ui-spec §4.12 #8).
 *
 * ⚠ Its own component, and rendered only inside the OPEN popover, because
 * `useNavigate` throws outside a router. Hoisting the hook to the picker would
 * make the picker un-renderable in every test that does not wrap it in a
 * `MemoryRouter` — and the picker is rendered for real by `IngestPanel.test.tsx`,
 * which is one of the suites §9 says this pass must not break.
 */
function NoModelsAvailable() {
  const navigate = useNavigate();
  return (
    <EmptyState
      compact
      icon={Brain}
      title="No models available"
      description="Configure a provider in Settings."
      actions={
        <Button
          type="button"
          variant="secondary"
          onClick={() => navigate('/settings', { state: { section: 'models' } })}
        >
          Open settings
        </Button>
      }
    />
  );
}

/**
 * Which model digests staged sources (ui-spec §4.0, §4.4).
 *
 * ⚠ **A `Popover` + searchable list, not a second 760px modal.** Choosing a
 * model from a rail footer is a picker interaction; the dialog it used to open
 * was the same width as the KB manager and covered the rail it was launched
 * from. It is also not a `DropdownMenu`: a menu's typeahead fights a nested text
 * field for every keystroke (see `components/ui/command.tsx`).
 *
 * The trigger takes the KB selector's chrome exactly (§4.1) — one Select shape
 * for both selectors in this rail — and `Set as default` moves from a
 * hand-rolled span in the trigger to a chip in the menu footer, where it
 * describes what picking a row will DO instead of decorating the resting state.
 */
export function IngestModelPicker({
  value,
  valueState = 'resolved',
  onChange,
  disabled = false,
  saving = false,
}: Props) {
  const { getProviders, getProviderModels } = useConfig();
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState('');
  const [sections, setSections] = useState<ProviderModelsSection[]>([]);
  const [providerDisplayNames, setProviderDisplayNames] = useState<Record<string, string>>({});
  /**
   * Display names of the configured providers this picker left out, so the
   * footer can say so. Derived rather than constant: a user with no coding-agent
   * provider configured is told nothing, because for them there is nothing to
   * explain and the line would be pure noise.
   */
  const [excludedProviderLabels, setExcludedProviderLabels] = useState<string[]>([]);

  useEffect(() => {
    void (async () => {
      try {
        const providers = await getProviders(false);
        const allConfigured = providers.filter((provider) => provider.is_configured);
        const names = allConfigured.reduce<Record<string, string>>((acc, provider) => {
          acc[provider.name] = provider.metadata.display_name ?? provider.name;
          return acc;
        }, {});
        setProviderDisplayNames(names);

        // A provider that cannot carry tool calls on the macro path is not a
        // model the user can choose badly — it is a model that writes nothing,
        // every time, after a full run. See `providerCanRunIngest`. The names
        // still go into `providerDisplayNames` above, so the trigger can label a
        // value that was stored before this exclusion existed, and the footer
        // can name what it left out instead of removing them in silence.
        const configuredProviders = allConfigured.filter((provider) =>
          providerCanRunIngest(provider.name)
        );
        setExcludedProviderLabels(
          allConfigured
            .filter((provider) => !providerCanRunIngest(provider.name))
            .map((provider) => names[provider.name] ?? provider.name)
        );

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
        setExcludedProviderLabels([]);
      }
    })();
  }, [getProviderModels, getProviders]);

  const hasModels = sections.length > 0;
  const currentProviderLabel = value
    ? (providerDisplayNames[value.provider] ?? value.provider)
    : '';
  const selectedProvider = useMemo(
    () =>
      value
        ? (sections
            .flatMap((section) => section.providers)
            .find((provider) => provider.name === value.provider) ?? null)
        : null,
    [sections, value]
  );
  const triggerLabel = value
    ? selectedProvider
      ? `${currentProviderLabel} / ${value.model}`
      : `${value.provider} / ${value.model}`
    : valueState === 'loading'
      ? 'Loading model…'
      : valueState === 'unavailable'
        ? 'Knowledge base unavailable'
        : valueState === 'unsupported'
          ? // Not "No model configured": there IS one, it is the chat model, and
            // it works in chat. Naming the real obstacle is what stops the user
            // being sent to Settings to fix something that is not broken.
            'Chat model can’t digest sources'
          : 'No model configured';

  const needle = query.trim().toLowerCase();
  function matches(provider: ProviderDetails, model: string): boolean {
    if (!needle) return true;
    const label = providerDisplayNames[provider.name] ?? provider.name;
    return (
      model.toLowerCase().includes(needle) ||
      provider.name.toLowerCase().includes(needle) ||
      label.toLowerCase().includes(needle)
    );
  }

  const visibleRows = sections.flatMap((section) =>
    section.providers.flatMap((provider) =>
      (section.modelsByProvider[provider.name] ?? [])
        .filter((model) => matches(provider, model))
        .map((model) => ({ provider, model }))
    )
  );

  return (
    <Popover open={open} onOpenChange={(next) => (disabled ? undefined : setOpen(next))}>
      <PopoverTrigger asChild>
        <button
          data-testid="knowledge-model-picker-trigger"
          type="button"
          disabled={disabled}
          aria-label="Knowledge model"
          aria-expanded={open}
          className="biorouter-focus-surface flex h-control-md w-full items-center gap-2 rounded-element border border-border-emphasized bg-background-default px-3 text-left text-label transition-[color,background-color,border-color,box-shadow] hover:inset-ring-2 hover:inset-ring-border-emphasized/30 disabled:cursor-not-allowed disabled:opacity-60"
        >
          <Brain className="h-icon-row w-icon-row shrink-0 text-text-muted" aria-hidden="true" />
          <span className="shrink-0 text-caps text-text-muted">Model</span>
          <span
            className={`min-w-0 flex-1 truncate ${value ? 'text-text-default' : 'text-text-muted'}`}
          >
            {triggerLabel}
          </span>
          {saving ? (
            <>
              <span className="sr-only">Saving</span>
              <LoaderCircle
                aria-hidden="true"
                className="h-icon-row w-icon-row shrink-0 animate-spin text-text-muted"
              />
            </>
          ) : (
            <ChevronDown
              aria-hidden="true"
              className={`h-icon-row w-icon-row shrink-0 text-text-muted transition-transform ${open ? 'rotate-180' : ''}`}
            />
          )}
        </button>
      </PopoverTrigger>

      <PopoverContent
        align="start"
        sideOffset={6}
        className="w-64 p-0"
        onOpenAutoFocus={(event) => event.preventDefault()}
      >
        <Command
          label="Knowledge models"
          query={query}
          onQueryChange={setQuery}
          className="max-h-[400px]"
        >
          <CommandInput placeholder="Search models" aria-label="Search models" autoFocus />
          <CommandList aria-label="Knowledge models">
            {!hasModels ? (
              <NoModelsAvailable />
            ) : visibleRows.length === 0 ? (
              <CommandEmpty>
                <p className="text-body text-text-muted">No models match</p>
                <p className="mt-1 text-supporting text-text-muted">
                  Try a different name or provider.
                </p>
              </CommandEmpty>
            ) : (
              sections.map((section) =>
                section.providers.map((provider) => {
                  const models = (section.modelsByProvider[provider.name] ?? []).filter((model) =>
                    matches(provider, model)
                  );
                  if (models.length === 0) return null;
                  return (
                    // ⚠ The heading comes from `providerOrdering.ts` and must keep
                    // coming from there. This is a third model-selection surface
                    // the design's §14.2 table never lists, and for a long time it
                    // was the ONLY consumer of the group's label field — the
                    // settings grid printed its own literals. Inline the words
                    // here and this picker will quietly go on using the old
                    // taxonomy after every other screen has been relabelled.
                    <CommandGroup
                      key={`${section.key}:${provider.name}`}
                      heading={providerDisplayNames[provider.name] ?? provider.name}
                    >
                      {models.map((model) => {
                        const selected = value?.provider === provider.name && value.model === model;
                        return (
                          <CommandItem
                            key={`${provider.name}:${model}`}
                            selected={selected}
                            onSelect={() => {
                              onChange({ provider: provider.name, model });
                              setOpen(false);
                            }}
                          >
                            <span className="min-w-0 flex-1 truncate">{model}</span>
                            {selected && (
                              <Check
                                className="h-icon-row w-icon-row shrink-0 text-text-default"
                                aria-hidden="true"
                              />
                            )}
                          </CommandItem>
                        );
                      })}
                    </CommandGroup>
                  );
                })
              )
            )}
          </CommandList>
          {(hasModels || excludedProviderLabels.length > 0) && (
            <div className="flex-none border-t border-border-subtle px-2 py-2">
              {hasModels && (
                <>
                  <Badge variant="chip">Set as default</Badge>
                  <p className="mt-1 text-supporting text-text-muted">
                    This model digests staged sources and scheduled knowledge curation. Chat replies
                    still use the model selected in the chat composer.
                  </p>
                </>
              )}
              {/* A provider that vanishes from a list without explanation reads
                  as a bug, and the user's next move is to go and re-check a
                  configuration that is already correct. Naming it — and saying
                  it still works in chat — is the difference between an omission
                  and an answer. Rendered only when something was actually left
                  out, so the line never appears for a user it cannot help. */}
              {excludedProviderLabels.length > 0 && (
                <p
                  data-testid="knowledge-model-picker-excluded"
                  className={`text-supporting text-text-muted ${hasModels ? 'mt-2' : ''}`}
                >
                  {excludedProviderLabels.join(' and ')}{' '}
                  {excludedProviderLabels.length === 1 ? 'is' : 'are'} not listed: these models only
                  receive tools inside a chat, so a digest would finish having written nothing. They
                  still work in chat.
                </p>
              )}
            </div>
          )}
        </Command>
      </PopoverContent>
    </Popover>
  );
}
