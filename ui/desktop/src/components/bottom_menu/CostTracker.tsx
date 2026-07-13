import { useState, useEffect } from 'react';
import { useModelAndProvider } from '../ModelAndProviderContext';
import { Tooltip, TooltipContent, TooltipTrigger } from '../ui/Tooltip';
import { fetchModelPricing } from '../../utils/pricing';
import { PricingData } from '../../api';
import type { ModelCostRow } from '../../hooks/useCostTracking';
import { ModelBreakdownTable } from './ModelBreakdownTable';

interface CostTrackerProps {
  inputTokens?: number;
  outputTokens?: number;
  sessionCosts?: {
    [key: string]: {
      inputTokens: number;
      outputTokens: number;
      totalCost: number;
    };
  };
  /** Real per-model rows from the token ledger. When present, they replace the
   * legacy client-side model-switch guessing for the popover breakdown. */
  modelCostRows?: ModelCostRow[];
}

/** Sum the priced per-model rows; rows with unknown pricing (null) count 0. */
export function sumModelRowsCost(rows: ModelCostRow[]): number {
  return rows.reduce((acc, row) => acc + (row.totalCost ?? 0), 0);
}

const COST_TRIGGER_CLASS =
  'flex h-7 items-center justify-center rounded-md px-0.5 transition-colors cursor-default text-text-default/70 hover:bg-background-medium hover:text-text-default';
const COST_SYMBOL_CLASS = 'mr-0.5 text-xs font-mono font-semibold leading-none text-current';

// The tokens CostTracker receives are the ACCUMULATED (billed) input/output
// across the whole conversation, not the last turn — see Issue #1. Spell that
// out in the tooltip so nobody reads it as the last message's size.
const BILLED_EXPLAINER =
  'Every turn resends the full conversation, so billed tokens exceed the last message’s count.';

// A prominent "N billed tokens (X in + Y out)" summary line for the tooltip.
export function billedTokensSummary(inputTokens: number, outputTokens: number): string {
  const total = inputTokens + outputTokens;
  return `${total.toLocaleString()} billed tokens (${inputTokens.toLocaleString()} in + ${outputTokens.toLocaleString()} out, accumulated across all turns)`;
}

export function formatTooltipMoney(amount: number, currency = '$'): string {
  if (!Number.isFinite(amount) || amount <= 0) {
    return `${currency}0.00`;
  }

  if (amount < 0.01) {
    return `<${currency}0.01`;
  }

  return `${currency}${amount.toLocaleString(undefined, {
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  })}`;
}

export function CostTracker({
  inputTokens = 0,
  outputTokens = 0,
  sessionCosts,
  modelCostRows,
}: CostTrackerProps) {
  const { currentModel, currentProvider } = useModelAndProvider();
  const [costInfo, setCostInfo] = useState<PricingData | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [showPricing, setShowPricing] = useState(true);
  const [pricingFailed, setPricingFailed] = useState(false);

  // Check if pricing is enabled
  useEffect(() => {
    const checkPricingSetting = () => {
      const stored = localStorage.getItem('show_pricing');
      setShowPricing(stored !== 'false');
    };

    checkPricingSetting();
    window.addEventListener('storage', checkPricingSetting);
    return () => window.removeEventListener('storage', checkPricingSetting);
  }, []);

  useEffect(() => {
    const loadCostInfo = async () => {
      if (!currentModel || !currentProvider) {
        setIsLoading(false);
        return;
      }

      setIsLoading(true);
      try {
        const costData = await fetchModelPricing(currentProvider, currentModel);
        if (costData) {
          setCostInfo(costData);
          setPricingFailed(false);
        } else {
          setPricingFailed(true);
          setCostInfo(null);
        }
      } catch {
        setPricingFailed(true);
        setCostInfo(null);
      } finally {
        setIsLoading(false);
      }
    };

    loadCostInfo();
  }, [currentModel, currentProvider]);

  // Return null early if pricing is disabled
  if (!showPricing) {
    return null;
  }

  // Real per-model rows (Issue #1): when the token ledger gives us an actual
  // breakdown, show it as a table and total it directly, instead of the legacy
  // client-side model-switch guessing. This path is independent of the current
  // model's pricing lookup, so a switched-away model still appears.
  if (modelCostRows && modelCostRows.length > 0) {
    const total = sumModelRowsCost(modelCostRows);
    return (
      <Tooltip>
        <TooltipTrigger asChild>
          <div className={COST_TRIGGER_CLASS}>
            <span className={COST_SYMBOL_CLASS}>$</span>
            <span className="text-xs font-mono">{total.toFixed(2)}</span>
          </div>
        </TooltipTrigger>
        <TooltipContent className="max-w-none">
          <div className="flex flex-col gap-2">
            <div className="whitespace-pre-line">
              {`${billedTokensSummary(inputTokens, outputTokens)}\n${BILLED_EXPLAINER}`}
            </div>
            <div className="font-medium">Per-model breakdown</div>
            <ModelBreakdownTable rows={modelCostRows} />
            <div className="text-right font-medium">Total: {formatTooltipMoney(total)}</div>
          </div>
        </TooltipContent>
      </Tooltip>
    );
  }

  const calculateCost = (): number => {
    // If we have session costs, calculate the total across all models
    if (sessionCosts) {
      let totalCost = 0;

      // Add up all historical costs from different models
      Object.values(sessionCosts).forEach((modelCost) => {
        totalCost += modelCost.totalCost;
      });

      // Add current model cost if we have pricing info
      if (
        costInfo &&
        (costInfo.input_token_cost !== undefined || costInfo.output_token_cost !== undefined)
      ) {
        const currentInputCost = inputTokens * (costInfo.input_token_cost || 0);
        const currentOutputCost = outputTokens * (costInfo.output_token_cost || 0);
        totalCost += currentInputCost + currentOutputCost;
      }

      return totalCost;
    }

    // Fallback to simple calculation for current model only
    if (
      !costInfo ||
      (costInfo.input_token_cost === undefined && costInfo.output_token_cost === undefined)
    ) {
      return 0;
    }

    const inputCost = inputTokens * (costInfo.input_token_cost || 0);
    const outputCost = outputTokens * (costInfo.output_token_cost || 0);
    const total = inputCost + outputCost;

    return total;
  };

  const formatCost = (cost: number): string => {
    return cost.toFixed(2);
  };

  // Show loading state or when we don't have model/provider info
  if (!currentModel || !currentProvider) {
    return null;
  }

  // If still loading, show a placeholder
  if (isLoading) {
    return (
      <div className="flex h-7 items-center justify-center rounded-md px-1 text-text-muted">
        <span className="text-xs font-mono">...</span>
      </div>
    );
  }

  // If no cost info found, try to return a default
  if (
    !costInfo ||
    (costInfo.input_token_cost === undefined && costInfo.output_token_cost === undefined)
  ) {
    // If it's a known free/local provider, show $0.000000 without "not available" message
    const freeProviders = ['llamacpp', 'ollama', 'local', 'localhost'];
    if (freeProviders.includes(currentProvider.toLowerCase())) {
      return (
        <Tooltip>
          <TooltipTrigger asChild>
            <div className={COST_TRIGGER_CLASS}>
              <span className={COST_SYMBOL_CLASS}>$</span>
              <span className="text-xs font-mono">0.00</span>
            </div>
          </TooltipTrigger>
          <TooltipContent>
            {`Local model — ${billedTokensSummary(inputTokens, outputTokens)}\n${BILLED_EXPLAINER}`}
          </TooltipContent>
        </Tooltip>
      );
    }

    // Otherwise show as unavailable
    const getUnavailableTooltip = () => {
      if (pricingFailed) {
        return `Pricing data unavailable for ${currentModel}\n${billedTokensSummary(inputTokens, outputTokens)}\n${BILLED_EXPLAINER}`;
      }
      return `Cost data not available for ${currentModel}\n${billedTokensSummary(inputTokens, outputTokens)}\n${BILLED_EXPLAINER}`;
    };

    return (
      <Tooltip>
        <TooltipTrigger asChild>
          <div className={COST_TRIGGER_CLASS}>
            <span className={COST_SYMBOL_CLASS}>$</span>
            <span className="text-xs font-mono">0.00</span>
          </div>
        </TooltipTrigger>
        <TooltipContent>{getUnavailableTooltip()}</TooltipContent>
      </Tooltip>
    );
  }

  const totalCost = calculateCost();

  // Build tooltip content
  const getTooltipContent = (): string => {
    // Lead every tooltip with the billed (accumulated) token figure + why it
    // is larger than the last message — this is the number the user is charged
    // for and the source of the Issue #1 confusion.
    const billedHeader = `${billedTokensSummary(inputTokens, outputTokens)}\n${BILLED_EXPLAINER}\n\n`;

    // Handle error states first
    if (pricingFailed) {
      return `${billedHeader}Pricing data unavailable for ${currentProvider}/${currentModel}`;
    }

    // Handle session costs
    if (sessionCosts && Object.keys(sessionCosts).length > 0) {
      // Show session breakdown
      let tooltip = `${billedHeader}Session cost breakdown:\n`;

      Object.entries(sessionCosts).forEach(([modelKey, cost]) => {
        const costStr = formatTooltipMoney(cost.totalCost, costInfo?.currency || '$');
        tooltip += `${modelKey}: ${costStr} (${cost.inputTokens.toLocaleString()} in, ${cost.outputTokens.toLocaleString()} out)\n`;
      });

      // Add current model if it has costs
      if (costInfo && (inputTokens > 0 || outputTokens > 0)) {
        const currentCost =
          inputTokens * (costInfo.input_token_cost || 0) +
          outputTokens * (costInfo.output_token_cost || 0);
        if (currentCost > 0) {
          tooltip += `${currentProvider}/${currentModel} (current): ${formatTooltipMoney(currentCost, costInfo.currency || '$')} (${inputTokens.toLocaleString()} in, ${outputTokens.toLocaleString()} out)\n`;
        }
      }

      tooltip += `\nTotal session cost: ${formatTooltipMoney(totalCost, costInfo?.currency || '$')}`;
      return tooltip;
    }

    // Default tooltip for single model
    return `${billedHeader}Input: ${inputTokens.toLocaleString()} tokens (${formatTooltipMoney(inputTokens * (costInfo?.input_token_cost || 0), costInfo?.currency || '$')}) | Output: ${outputTokens.toLocaleString()} tokens (${formatTooltipMoney(outputTokens * (costInfo?.output_token_cost || 0), costInfo?.currency || '$')})`;
  };

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <div className={COST_TRIGGER_CLASS}>
          <span className={COST_SYMBOL_CLASS}>$</span>
          <span className="text-xs font-mono">{formatCost(totalCost)}</span>
        </div>
      </TooltipTrigger>
      <TooltipContent>{getTooltipContent()}</TooltipContent>
    </Tooltip>
  );
}
