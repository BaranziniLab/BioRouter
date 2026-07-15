import { useEffect, useState } from 'react';
import { useModelAndProvider } from '../ModelAndProviderContext';
import { Tooltip, TooltipContent, TooltipTrigger } from '../ui/Tooltip';
import { Button } from '../ui/button';
import { fetchModelPricing } from '../../utils/pricing';
import { PricingData } from '../../api';
import type { ModelCostRow, SessionCostRow, SessionCosts } from '../../hooks/useCostTracking';

interface CostTrackerProps {
  inputTokens?: number;
  outputTokens?: number;
  sessionCosts?: SessionCosts;
  modelCostRows?: ModelCostRow[];
}

export interface CostEstimate {
  amount: number | null;
  partial: boolean;
}

export function aggregateModelRowsCost(rows: ModelCostRow[]): CostEstimate {
  let amount = 0;
  let hasKnownCost = false;
  let partial = false;
  for (const row of rows) {
    if (row.totalCost === null) {
      partial = true;
    } else {
      amount += row.totalCost;
      hasKnownCost = true;
    }
    partial ||= row.costIsPartial ?? false;
  }
  return { amount: hasKnownCost ? amount : null, partial };
}

function aggregateSessionCosts(rows: SessionCostRow[]): CostEstimate {
  let amount = 0;
  let hasKnownCost = false;
  let partial = false;
  for (const row of rows) {
    if (row.totalCost === null) {
      partial = true;
    } else {
      amount += row.totalCost;
      hasKnownCost = true;
    }
    partial ||= row.costIsPartial ?? false;
  }
  return { amount: hasKnownCost ? amount : null, partial };
}

const COST_TRIGGER_CLASS =
  'h-7 min-w-0 px-0.5 font-mono text-xs font-normal text-text-default/70 hover:bg-background-medium hover:text-text-default';

export function sessionTokensSummary(inputTokens: number, outputTokens: number): string {
  return `Input: ${inputTokens.toLocaleString()} tokens\nOutput: ${outputTokens.toLocaleString()} tokens`;
}

export function formatTooltipMoney(amount: number | null, currency = '$'): string {
  if (amount === null || !Number.isFinite(amount) || amount < 0) return 'Unavailable';
  if (amount > 0 && amount < 0.01) return `<${currency}0.01`;
  return `${currency}${amount.toLocaleString(undefined, {
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  })}`;
}

export function formatCostEstimate(estimate: CostEstimate, currency = '$'): string {
  return formatTooltipMoney(estimate.amount, currency);
}

function CostTrigger({ estimate, currency = '$' }: { estimate: CostEstimate; currency?: string }) {
  const label = formatCostEstimate(estimate, currency);
  return (
    <TooltipTrigger asChild>
      <Button
        type="button"
        variant="ghost"
        size="xs"
        className={COST_TRIGGER_CLASS}
        aria-label={
          estimate.amount === null
            ? 'Session cost unavailable'
            : `${estimate.partial ? 'Estimated session total' : 'Session cost'} ${label}`
        }
      >
        {label}
      </Button>
    </TooltipTrigger>
  );
}

export function costEstimateSummary(estimate: CostEstimate, currency = '$') {
  if (estimate.amount === null) return 'Total cost unavailable';
  const label = formatCostEstimate(estimate, currency);
  return estimate.partial
    ? `Estimated total: ${label}\nConservative estimate based on available token and pricing data.`
    : `Total cost: ${label}`;
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

  useEffect(() => {
    const checkPricingSetting = () => {
      setShowPricing(localStorage.getItem('show_pricing') !== 'false');
    };

    checkPricingSetting();
    window.addEventListener('storage', checkPricingSetting);
    return () => window.removeEventListener('storage', checkPricingSetting);
  }, []);

  useEffect(() => {
    const loadCostInfo = async () => {
      if (!currentModel || !currentProvider) {
        setCostInfo(null);
        setIsLoading(false);
        return;
      }

      setIsLoading(true);
      try {
        const costData = await fetchModelPricing(currentProvider, currentModel);
        setCostInfo(costData);
      } catch {
        setCostInfo(null);
      } finally {
        setIsLoading(false);
      }
    };

    loadCostInfo();
  }, [currentModel, currentProvider]);

  if (!showPricing) return null;

  if (modelCostRows && modelCostRows.length > 0) {
    const estimate = aggregateModelRowsCost(modelCostRows);
    const inputTotal = modelCostRows.reduce((sum, row) => sum + row.inputTokens, 0);
    const outputTotal = modelCostRows.reduce((sum, row) => sum + row.outputTokens, 0);
    return (
      <Tooltip>
        <CostTrigger estimate={estimate} />
        <TooltipContent className="whitespace-pre-line">
          {`${sessionTokensSummary(inputTotal, outputTotal)}\n${costEstimateSummary(estimate)}`}
        </TooltipContent>
      </Tooltip>
    );
  }

  const legacyRows = sessionCosts ? Object.values(sessionCosts) : [];
  if (legacyRows.length > 0) {
    const estimate = aggregateSessionCosts(legacyRows);
    const totals = legacyRows.reduce(
      (sum, row) => ({
        input: sum.input + row.inputTokens,
        output: sum.output + row.outputTokens,
      }),
      { input: 0, output: 0 }
    );
    return (
      <Tooltip>
        <CostTrigger estimate={estimate} />
        <TooltipContent className="whitespace-pre-line">
          {`${sessionTokensSummary(totals.input, totals.output)}\n${costEstimateSummary(estimate)}`}
        </TooltipContent>
      </Tooltip>
    );
  }

  if (!currentModel || !currentProvider) return null;

  if (isLoading) {
    return (
      <div className="flex h-7 items-center justify-center rounded-md px-1 text-text-muted">
        <span className="text-xs font-mono">...</span>
      </div>
    );
  }

  if (!costInfo) {
    const estimate = { amount: null, partial: true };
    return (
      <Tooltip>
        <CostTrigger estimate={estimate} />
        <TooltipContent className="whitespace-pre-line">
          {`${sessionTokensSummary(inputTokens, outputTokens)}\n${costEstimateSummary(estimate)}`}
        </TooltipContent>
      </Tooltip>
    );
  }

  const freshSubtotal =
    inputTokens * (costInfo.input_token_cost ?? 0) +
    outputTokens * (costInfo.output_token_cost ?? 0);
  const estimate = { amount: freshSubtotal, partial: true };
  return (
    <Tooltip>
      <CostTrigger estimate={estimate} currency={costInfo.currency} />
      <TooltipContent className="whitespace-pre-line">
        {`${sessionTokensSummary(inputTokens, outputTokens)}\n${costEstimateSummary(estimate, costInfo.currency)}`}
      </TooltipContent>
    </Tooltip>
  );
}
