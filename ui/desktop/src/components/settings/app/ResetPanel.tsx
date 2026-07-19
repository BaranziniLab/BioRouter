import { useCallback, useEffect, useMemo, useState } from 'react';
import { Check, ChevronDown, History, Loader2, RotateCcw } from '../../icons/app-icons';
import { ENTITY_ICONS, type EntityIcon } from '../../icons/entity-icons';
import { previewReset, resetAppData } from '../../../api';
import type { ResetCategory, ResetCounts } from '../../../api';
import { toastService } from '../../../toasts';
import { clearAllSessionCache } from '../../../utils/sessionCache';
import { clearSessionListCache } from '../../../utils/sessionListCache';
import { LocalMessageStorage } from '../../../utils/localMessageStorage';
import { clearDashboardState } from '../../Dashboard/dashboardStorage';
import { Button } from '../../ui/button';
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from '../../ui/collapsible';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '../../ui/dialog';

type ResetPanelProps = {
  onReset?: (categories: ResetCategory[]) => void;
};

type CategoryDefinition = {
  id: ResetCategory;
  title: string;
  description: string;
  countKey: keyof ResetCounts;
  countLabel: string;
  singularCountLabel?: string;
  icon: EntityIcon;
};

const CATEGORIES: CategoryDefinition[] = [
  {
    id: 'applications',
    title: 'Applications',
    description: 'Delete every application created with Agent Drafter.',
    countKey: 'applications',
    countLabel: 'built',
    icon: ENTITY_ICONS.application,
  },
  {
    id: 'knowledge',
    title: 'Knowledge & memory',
    description: 'Recreate the empty Soul knowledge base and clear saved memory.',
    countKey: 'knowledgeBases',
    countLabel: 'bases',
    singularCountLabel: 'base',
    icon: ENTITY_ICONS.knowledge,
  },
  {
    id: 'skills',
    title: 'Installed skills',
    description: 'Remove user-installed skills and restore built-in skill files.',
    countKey: 'skills',
    countLabel: 'custom',
    icon: ENTITY_ICONS.skill,
  },
  {
    id: 'extensions',
    title: 'Custom extensions',
    description: 'Remove added extensions while keeping bundled capabilities.',
    countKey: 'extensions',
    countLabel: 'custom',
    icon: ENTITY_ICONS.extension,
  },
  {
    id: 'schedules',
    title: 'Schedules',
    description: 'Remove custom schedules and restore Daily Meditation.',
    countKey: 'schedules',
    countLabel: 'custom',
    icon: ENTITY_ICONS.schedule,
  },
  {
    id: 'workflows',
    title: 'Workflows',
    description: 'Remove managed workflows and restore the Meditation workflow.',
    countKey: 'workflows',
    countLabel: 'custom',
    icon: ENTITY_ICONS.workflow,
  },
  {
    id: 'history',
    title: 'Conversation & usage history',
    description: 'Clear every conversation, token meter, cost total, and checkpoint.',
    countKey: 'conversations',
    countLabel: 'conversations',
    singularCountLabel: 'conversation',
    icon: History,
  },
];

const ALL_CATEGORIES = CATEGORIES.map((category) => category.id);

function clearRendererHistory() {
  LocalMessageStorage.clearHistory();
  clearAllSessionCache();
  clearSessionListCache();
  clearDashboardState();
}

function clearKnowledgeSelections() {
  for (let index = localStorage.length - 1; index >= 0; index -= 1) {
    const key = localStorage.key(index);
    if (key?.startsWith('knowledge_active_kb') || key?.startsWith('knowledge_hidden_kbs')) {
      localStorage.removeItem(key);
    }
  }
}

export default function ResetPanel({ onReset }: ResetPanelProps) {
  const [selected, setSelected] = useState<Set<ResetCategory>>(new Set());
  const [expanded, setExpanded] = useState<ResetCategory | null>(null);
  const [pendingCategories, setPendingCategories] = useState<ResetCategory[] | null>(null);
  const [counts, setCounts] = useState<ResetCounts | null>(null);
  const [loadingCounts, setLoadingCounts] = useState(true);
  const [resetting, setResetting] = useState(false);
  const [status, setStatus] = useState<string | null>(null);

  const loadCounts = useCallback(async () => {
    setLoadingCounts(true);
    try {
      const response = await previewReset<true>({ throwOnError: true });
      setCounts(response.data.counts);
    } catch (error) {
      console.error('Failed to inspect reset data:', error);
      setCounts(null);
    } finally {
      setLoadingCounts(false);
    }
  }, []);

  useEffect(() => {
    void loadCounts();
  }, [loadCounts]);

  const selectedCategories = useMemo(
    () => CATEGORIES.filter((category) => selected.has(category.id)).map((category) => category.id),
    [selected]
  );

  const toggleCategory = (category: ResetCategory) => {
    setStatus(null);
    setSelected((current) => {
      const next = new Set(current);
      if (next.has(category)) next.delete(category);
      else next.add(category);
      return next;
    });
  };

  const openConfirmation = (categories: ResetCategory[]) => {
    setPendingCategories(categories);
  };

  const handleReset = async () => {
    if (!pendingCategories?.length) return;
    const categories = pendingCategories;
    setResetting(true);
    setStatus(null);
    try {
      await resetAppData<true>({ body: { categories }, throwOnError: true });
      if (categories.includes('history')) clearRendererHistory();
      if (categories.includes('knowledge')) clearKnowledgeSelections();
      setSelected(new Set());
      setPendingCategories(null);
      setStatus('Reset complete. Factory defaults have been restored for the selected areas.');
      await loadCounts();
      onReset?.(categories);
      window.dispatchEvent(new CustomEvent('biorouter:data-reset', { detail: { categories } }));
      toastService.success({
        title: 'Reset complete',
        msg: `${categories.length} ${categories.length === 1 ? 'area was' : 'areas were'} restored.`,
      });
    } catch (error) {
      console.error('Failed to reset app data:', error);
      const message =
        error instanceof Error ? error.message : 'Biorouter could not reset the selected data.';
      setStatus(message);
      toastService.error({ title: 'Reset failed', msg: message });
    } finally {
      setResetting(false);
    }
  };

  const pendingDefinitions = pendingCategories
    ? CATEGORIES.filter((category) => pendingCategories.includes(category.id))
    : [];
  const isEverything = pendingCategories?.length === ALL_CATEGORIES.length;

  return (
    <div className="biorouter-settings-section" data-testid="reset-panel">
      <div className="biorouter-settings-section-header flex flex-wrap items-end justify-between gap-2">
        <div>
          <h2 className="mb-1 text-[11px] font-medium uppercase tracking-wider text-text-muted">
            Reset
          </h2>
          <p className="text-xs text-text-muted">
            Choose what to clean up. Built-in content is restored; models, credentials, and
            preferences are kept.
          </p>
        </div>
        <div className="flex items-center gap-2 pb-0.5">
          <span className="text-xs tabular-nums text-text-muted">
            {selected.size} of {CATEGORIES.length} selected
          </span>
          <Button
            type="button"
            size="xs"
            variant="ghost"
            onClick={() => setSelected(new Set(ALL_CATEGORIES))}
          >
            Select all
          </Button>
          {selected.size > 0 && (
            <Button type="button" size="xs" variant="ghost" onClick={() => setSelected(new Set())}>
              Clear
            </Button>
          )}
        </div>
      </div>

      <div className="biorouter-settings-list">
        {CATEGORIES.map((category) => {
          const isSelected = selected.has(category.id);
          const isExpanded = expanded === category.id;
          const count = counts?.[category.countKey];
          const countText =
            count === undefined
              ? null
              : `${count.toLocaleString()} ${
                  count === 1 && category.singularCountLabel
                    ? category.singularCountLabel
                    : category.countLabel
                }`;
          return (
            <Collapsible
              key={category.id}
              open={isExpanded}
              onOpenChange={(open) => setExpanded(open ? category.id : null)}
              data-testid={`reset-option-${category.id}`}
              className={`biorouter-settings-row ${isSelected ? 'bg-background-accent/5' : ''}`}
            >
              <div className="flex min-h-12 items-center gap-3 px-3 py-2">
                <CollapsibleTrigger
                  aria-label={`${isExpanded ? 'Hide' : 'Show'} details for ${category.title}`}
                  className="group flex min-w-0 flex-1 items-center justify-between gap-3 text-left"
                >
                  <span className="min-w-0 truncate text-sm font-medium text-text-default">
                    {category.title}
                  </span>
                  <span className="flex shrink-0 items-center gap-2">
                    <span className="flex items-center gap-1 text-[11px] tabular-nums text-text-muted">
                      {loadingCounts ? (
                        <Loader2 className="h-3 w-3 animate-spin" />
                      ) : (
                        (countText ?? '—')
                      )}
                    </span>
                    <ChevronDown
                      aria-hidden="true"
                      className={`h-3.5 w-3.5 text-text-muted transition-transform ${
                        isExpanded ? 'rotate-180' : ''
                      }`}
                    />
                  </span>
                </CollapsibleTrigger>
                <button
                  type="button"
                  aria-pressed={isSelected}
                  aria-label={`${isSelected ? 'Deselect' : 'Select'} ${category.title} for reset`}
                  className={`flex h-5 w-5 shrink-0 items-center justify-center rounded border transition-colors ${
                    isSelected
                      ? 'border-border-accent bg-background-accent text-text-on-accent'
                      : 'border-border-strong bg-background-default hover:border-border-accent'
                  }`}
                  onClick={() => toggleCategory(category.id)}
                >
                  {isSelected && <Check className="h-3 w-3" />}
                </button>
              </div>
              <CollapsibleContent className="-mt-1 px-3 pb-2.5 text-xs leading-5 text-text-muted">
                {category.description}
              </CollapsibleContent>
            </Collapsible>
          );
        })}
      </div>

      <div className="mt-3 flex flex-wrap items-center justify-between gap-3">
        <div className="flex items-start gap-2">
          <RotateCcw className="mt-0.5 h-4 w-4 shrink-0 text-text-danger" />
          <p className="max-w-xl text-xs leading-5 text-text-muted">
            Resetting is permanent. Export anything you want to keep before continuing.
          </p>
        </div>
        <div className="flex items-center gap-2">
          <Button
            type="button"
            variant="outline"
            className="border-border-danger text-text-danger hover:bg-background-danger/10"
            disabled={selectedCategories.length === 0 || resetting}
            onClick={() => openConfirmation(selectedCategories)}
          >
            Reset selected
          </Button>
          <Button
            type="button"
            variant="destructive"
            disabled={resetting}
            onClick={() => openConfirmation(ALL_CATEGORIES)}
          >
            <RotateCcw />
            Reset everything
          </Button>
        </div>
      </div>

      {status && (
        <p
          className={`mt-2 text-xs ${status.startsWith('Reset complete') ? 'text-text-muted' : 'text-text-danger'}`}
          role="status"
        >
          {status}
        </p>
      )}

      <Dialog
        open={pendingCategories !== null}
        onOpenChange={(open) => !open && !resetting && setPendingCategories(null)}
      >
        <DialogContent dismissible={!resetting} className="sm:max-w-[520px]">
          <DialogHeader>
            <DialogTitle>{isEverything ? 'Reset everything?' : 'Reset selected data?'}</DialogTitle>
            <DialogDescription>
              This cannot be undone. Biorouter will restore built-in content after removing:
            </DialogDescription>
          </DialogHeader>

          <div className="grid grid-cols-1 gap-2 rounded-lg border border-border-subtle bg-background-muted/40 p-3 sm:grid-cols-2">
            {pendingDefinitions.map((category) => {
              const Icon = category.icon;
              return (
                <div
                  key={category.id}
                  className="flex items-center gap-2 text-sm text-text-default"
                >
                  <Icon className="h-4 w-4 text-text-muted" />
                  {category.title}
                </div>
              );
            })}
          </div>

          <p className="text-xs leading-5 text-text-muted">
            Your configured models, provider credentials, theme, and app preferences will stay in
            place.
          </p>

          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              disabled={resetting}
              onClick={() => setPendingCategories(null)}
            >
              Cancel
            </Button>
            <Button type="button" variant="destructive" disabled={resetting} onClick={handleReset}>
              {resetting && <Loader2 className="animate-spin" />}
              {resetting ? 'Resetting…' : isEverything ? 'Reset everything' : 'Reset selected'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
