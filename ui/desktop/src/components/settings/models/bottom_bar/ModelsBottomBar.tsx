import { SlidersHorizontal, Brain } from '../../../icons/app-icons';
import React, { useEffect, useState } from 'react';
import { useModelAndProvider } from '../../../ModelAndProviderContext';
import { SwitchModelModal } from '../subcomponents/SwitchModelModal';
import { LeadWorkerSettings } from '../subcomponents/LeadWorkerSettings';
import { View } from '../../../../utils/navigationUtils';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '../../../ui/dropdown-menu';
import { useCurrentModelInfo } from '../../../BaseChat';
import { useConfig } from '../../../ConfigContext';
import { getProviderMetadata } from '../modelInterface';
import { Alert } from '../../../alerts';
import BottomMenuAlertPopover from '../../../bottom_menu/BottomMenuAlertPopover';
import { Tooltip, TooltipContent, TooltipTrigger } from '../../../ui/Tooltip';
import { PrivacyBadge } from '../../../ui/PrivacyBadge';
import {
  disclosureRequiredForTier,
  useDisclosure,
} from '../../../privacy/disclosureCopy';
import type { SessionClassification } from '../../../../api/types.gen';

interface ModelsBottomBarProps {
  sessionId: string | null;
  dropdownRef: React.RefObject<HTMLDivElement>;
  setView: (view: View) => void;
  alerts: Alert[];
  /** Hide the inline alert green-dot when the context window indicator is
   * surfaced separately (e.g. in the picker popover's dedicated row). */
  hideAlertPopover?: boolean;
  /**
   * The focused chat's privacy tier (issue #56, R10 / §14.2).
   *
   * ⚠ This is the SESSION's ratcheted classification, not the bound provider's
   * `metadata.tier`. `providerOrdering.ts` records why, and it is not a
   * preference: `GET /config/providers` serves the *type-level* tier, so an
   * `ollama` re-pointed off this machine by `OLLAMA_HOST` still arrives here
   * claiming `private` while its instance resolves `public`. A badge hung on
   * that field would read Private in exactly the demotion case the tier exists
   * to catch. The session classification is computed server-side from the
   * instance and only ever ratchets upward, so it can be asserted.
   *
   * `undefined` — a chat whose session has not loaded — renders nothing rather
   * than asserting Public, matching `SessionNamePill`.
   */
  privacyTier?: SessionClassification;
}

const MAX_INLINE_MODEL_LABEL_CHARS = 24;

export default function ModelsBottomBar({
  sessionId,
  dropdownRef,
  setView,
  alerts,
  hideAlertPopover = false,
  privacyTier,
}: ModelsBottomBarProps) {
  const {
    currentModel,
    currentProvider,
    getCurrentModelAndProviderForDisplay,
    getCurrentModelDisplayName,
    getCurrentProviderDisplayName,
  } = useModelAndProvider();
  const currentModelInfo = useCurrentModelInfo();
  const { read, getProviders } = useConfig();
  const [displayProvider, setDisplayProvider] = useState<string | null>(null);
  const [displayModelName, setDisplayModelName] = useState<string>('Select Model');
  const [isAddModelModalOpen, setIsAddModelModalOpen] = useState(false);
  const [isLeadWorkerModalOpen, setIsLeadWorkerModalOpen] = useState(false);
  const [isLeadWorkerActive, setIsLeadWorkerActive] = useState(false);
  const [providerDefaultModel, setProviderDefaultModel] = useState<string | null>(null);
  /**
   * Task 30A (issue #56, DR-17 requirement 3). Does the model bound to this
   * chat need the one-line disclosure?
   *
   * ⚠ It hangs off the bound PROVIDER's tier, never off {@link privacyTier}.
   * That prop is the chat's ratcheted CLASSIFICATION, and a fresh chat on Versa
   * is classified `public` while its model is emphatically not a public model —
   * so a line keyed on it would tell the user something false about the one
   * provider this whole feature exists to make safe to use.
   *
   * `null` while unresolved: say nothing rather than guess, in a chip that is
   * re-rendered on every keystroke in the composer.
   */
  const [needsDisclosure, setNeedsDisclosure] = useState<boolean | null>(null);

  // Check if lead/worker mode is active
  useEffect(() => {
    const checkLeadWorker = async () => {
      try {
        const leadModel = await read('BIOROUTER_LEAD_MODEL', false);
        setIsLeadWorkerActive(!!leadModel);
      } catch (error) {
        console.error('Error checking lead model:', error);
        setIsLeadWorkerActive(false);
      }
    };
    checkLeadWorker();
  }, [read]);

  // Refresh lead/worker status when modal closes
  const handleLeadWorkerModalClose = () => {
    setIsLeadWorkerModalOpen(false);
    // Refresh the lead/worker status after modal closes
    const checkLeadWorker = async () => {
      try {
        const leadModel = await read('BIOROUTER_LEAD_MODEL', false);
        const currentModel = await read('BIOROUTER_MODEL', false);
        setIsLeadWorkerActive(!!leadModel);
        setLeadModelName((leadModel as string) || '');
        setCurrentActiveModel((currentModel as string) || '');
      } catch (error) {
        console.error('Error checking lead model after modal close:', error);
        setIsLeadWorkerActive(false);
      }
    };
    checkLeadWorker();
  };

  // Since currentModelInfo.mode is not working, let's determine mode differently
  // We'll need to get the lead model and compare it with the current model
  const [leadModelName, setLeadModelName] = useState<string>('');
  const [currentActiveModel, setCurrentActiveModel] = useState<string>('');

  // Get lead model name and current model for comparison
  useEffect(() => {
    const getModelInfo = async () => {
      try {
        const leadModel = await read('BIOROUTER_LEAD_MODEL', false);
        const currentModel = await read('BIOROUTER_MODEL', false);
        setLeadModelName((leadModel as string) || '');
        setCurrentActiveModel((currentModel as string) || '');
      } catch (error) {
        console.error('Error getting model info:', error);
      }
    };
    getModelInfo();
  }, [read]);

  // Determine the mode based on which model is currently active
  const modelMode = isLeadWorkerActive
    ? currentActiveModel === leadModelName
      ? 'lead'
      : 'worker'
    : undefined;

  // Determine which model to display - activeModel takes priority when lead/worker is active
  const displayModel =
    isLeadWorkerActive && currentModelInfo?.model
      ? currentModelInfo.model
      : currentModel || providerDefaultModel || displayModelName;
  const fullModelLabel =
    isLeadWorkerActive && modelMode ? `${displayModel} (${modelMode})` : displayModel;
  const inlineModelLabel =
    fullModelLabel.length > MAX_INLINE_MODEL_LABEL_CHARS
      ? `${fullModelLabel.slice(0, MAX_INLINE_MODEL_LABEL_CHARS - 3)}...`
      : fullModelLabel;

  // Update display provider when current provider changes
  useEffect(() => {
    if (currentProvider) {
      (async () => {
        const providerDisplayName = await getCurrentProviderDisplayName();
        if (providerDisplayName) {
          setDisplayProvider(providerDisplayName);
        } else {
          const modelProvider = await getCurrentModelAndProviderForDisplay();
          setDisplayProvider(modelProvider.provider);
        }
      })();
    }
  }, [currentProvider, getCurrentProviderDisplayName, getCurrentModelAndProviderForDisplay]);

  // Fetch provider default model when provider changes and no current model
  useEffect(() => {
    if (currentProvider && !currentModel) {
      (async () => {
        try {
          const metadata = await getProviderMetadata(currentProvider, getProviders);
          setProviderDefaultModel(metadata.default_model);
        } catch (error) {
          console.error('Failed to get provider default model:', error);
          setProviderDefaultModel(null);
        }
      })();
    } else if (currentModel) {
      // Clear provider default when we have a current model
      setProviderDefaultModel(null);
    }
  }, [currentProvider, currentModel, getProviders]);

  // Update display model name when current model changes
  useEffect(() => {
    (async () => {
      const displayName = await getCurrentModelDisplayName();
      setDisplayModelName(displayName);
    })();
  }, [currentModel, getCurrentModelDisplayName]);

  // Task 30A. The bound provider's own tier, resolved from the registry the
  // daemon serves — never a list kept here.
  useEffect(() => {
    if (!currentProvider) {
      setNeedsDisclosure(null);
      return;
    }
    let cancelled = false;
    (async () => {
      try {
        const metadata = await getProviderMetadata(currentProvider, getProviders);
        if (!cancelled) setNeedsDisclosure(disclosureRequiredForTier(metadata.tier));
      } catch {
        // A provider Biorouter cannot classify is one it cannot vouch for.
        // Fail-safe here means fail towards telling the user.
        if (!cancelled) setNeedsDisclosure(true);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [currentProvider, getProviders]);

  // ⚠ Unconditional on the master privacy switch — DR-15 turns off enforcement,
  // not the truth. See `privacy/disclosureCopy.ts`.
  const { copy: disclosure } = useDisclosure(needsDisclosure === true);
  const disclosureLine = needsDisclosure === true ? (disclosure?.short ?? null) : null;

  // §14.2's line for the chat's tier. A "Private" PILL cannot fit in this chip
  // — the trigger is `max-w-[120px]` and the label is already truncated at 24
  // characters — so the chip carries the dense dot and the WORD goes where
  // there is room for it: the tooltip and the dropdown header.
  const privacyLine =
    privacyTier === 'private'
      ? 'Private chat — only private models can read it'
      : privacyTier === 'public'
        ? 'Public chat'
        : null;

  return (
    <div className="relative flex items-center" ref={dropdownRef}>
      {!hideAlertPopover && <BottomMenuAlertPopover alerts={alerts} />}
      <DropdownMenu>
        <Tooltip>
          <TooltipTrigger asChild>
            <DropdownMenuTrigger
              aria-label={`Current model: ${fullModelLabel}${privacyLine ? ` (${privacyLine})` : ''}`}
              className="flex h-7 min-w-0 max-w-[120px] flex-shrink-0 items-center rounded-md px-0.5 hover:cursor-pointer text-text-default/70 hover:bg-background-medium hover:text-text-default transition-colors"
            >
              <div className="flex min-w-0 max-w-full items-center gap-0.5 truncate">
                <Brain className="size-[18px] flex-shrink-0" />
                <span className="truncate text-xs">{inlineModelLabel}</span>
                {privacyTier && <PrivacyBadge tier={privacyTier} dense className="ml-1" />}
              </div>
            </DropdownMenuTrigger>
          </TooltipTrigger>
          <TooltipContent side="top">
            Model: {fullModelLabel}
            {privacyLine && ` · ${privacyLine}`}
            {disclosureLine && (
              <span className="mt-1 block max-w-[280px] [overflow-wrap:anywhere]">
                {disclosureLine}
              </span>
            )}
          </TooltipContent>
        </Tooltip>
        <DropdownMenuContent side="top" align="center" className="w-64 p-0 font-sans">
          <div className="border-b border-border-subtle px-3 py-2.5">
            <div className="text-sm font-medium text-text-default">Current model</div>
            <div className="mt-0.5 text-[11px] leading-4 text-text-muted">
              {displayModelName}
              {displayProvider && ` · ${displayProvider}`}
            </div>
            {privacyLine && (
              <div className="mt-1 text-[11px] leading-4 text-text-muted">{privacyLine}</div>
            )}
            {/*
              Issue #56, DR-17 requirement 3 — the standing one-line disclosure,
              in the one place on this chip with room for a sentence. The words
              come from the daemon; a literal here would be a second definition
              and would be the one that shipped stale.
            */}
            {disclosureLine && (
              <div
                data-testid="non-private-model-chip-note"
                className="mt-1 text-[11px] leading-4 text-text-muted [overflow-wrap:anywhere]"
              >
                {disclosureLine}
              </div>
            )}
          </div>
          <div className="p-1.5">
            <DropdownMenuItem
              className="h-auto rounded-md px-2 py-1.5 text-xs font-medium text-text-default"
              onClick={() => setIsAddModelModalOpen(true)}
            >
              <span>Change Model</span>
              <SlidersHorizontal className="ml-auto size-3.5" />
            </DropdownMenuItem>
            <DropdownMenuItem
              className="h-auto rounded-md px-2 py-1.5 text-xs font-medium text-text-default"
              onClick={() => setIsLeadWorkerModalOpen(true)}
            >
              <span>Lead/Worker Settings</span>
              <SlidersHorizontal className="ml-auto size-3.5" />
            </DropdownMenuItem>
          </div>
        </DropdownMenuContent>
      </DropdownMenu>

      {isAddModelModalOpen ? (
        <SwitchModelModal
          sessionId={sessionId}
          privacyTier={privacyTier}
          setView={setView}
          onClose={() => setIsAddModelModalOpen(false)}
        />
      ) : null}

      {isLeadWorkerModalOpen ? (
        <LeadWorkerSettings isOpen={isLeadWorkerModalOpen} onClose={handleLeadWorkerModalClose} />
      ) : null}
    </div>
  );
}
