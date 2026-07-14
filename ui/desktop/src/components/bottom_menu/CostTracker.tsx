import { useEffect, useState } from 'react';
import { useModelAndProvider } from '../ModelAndProviderContext';
import { Tooltip, TooltipContent, TooltipTrigger } from '../ui/Tooltip';
import { Button } from '../ui/button';
import { fetchModelPricing } from '../../utils/pricing';
import { PricingData } from '../../api';
import type { ModelCostRow, SessionCostRow, SessionCosts } from '../../hooks/useCostTracking';
import { knownBilledTokens, sumBilledTokens } from '../../utils/usageAccounting';
import { ModelBreakdownTable } from './ModelBreakdownTable';

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
  'h-7 min-w-0 px-1 font-mono text-xs font-normal text-text-default/70 hover:bg-background-medium hover:text-text-default';

const BILLED_EXPLAINER =
  'Every turn resends the full conversation, so billed tokens exceed the last message’s count.';

export function billedTokensSummary(
  inputTokens: number,
  outputTokens: number,
  cacheReadTokens: number | null = 0,
  cacheCreationTokens: number | null = 0,
  exactTotal?: number | null
): string {
  const knownSubtotal = knownBilledTokens({
    inputTokens,
    outputTokens,
    cacheReadTokens,
    cacheCreationTokens,
  });
  const total =
    exactTotal === undefined
      ? cacheReadTokens === null || cacheCreationTokens === null
        ? null
        : knownSubtotal
      : exactTotal;
  const buckets = [
    `${inputTokens.toLocaleString()} fresh in`,
    `${cacheReadTokens === null ? '—' : cacheReadTokens.toLocaleString()} cache read`,
    `${cacheCreationTokens === null ? '—' : cacheCreationTokens.toLocaleString()} cache write`,
    `${outputTokens.toLocaleString()} out`,
  ];
  const headline =
    total === null
      ? knownSubtotal > 0
        ? `≥${knownSubtotal.toLocaleString()} billed tokens`
        : '— billed tokens'
      : `${total.toLocaleString()} billed tokens`;
  return `${headline} (${buckets.join(' + ')}, accumulated across all turns)`;
}

function sumNullableTokens(
  rows: ModelCostRow[],
  key: 'cacheReadTokens' | 'cacheCreationTokens'
): number | null {
  let total = 0;
  for (const row of rows) {
    const value = row[key];
    if (value === null) return null;
    total += value;
  }
  return total;
}

export function formatTooltipMoney(amount: number | null, currency = '$'): string {
  if (amount === null || !Number.isFinite(amount) || amount < 0) return '—';
  if (amount > 0 && amount < 0.01) return `<${currency}0.01`;
  return `${currency}${amount.toLocaleString(undefined, {
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  })}`;
}

export function formatCostEstimate(estimate: CostEstimate, currency = '$'): string {
  const amount = formatTooltipMoney(estimate.amount, currency);
  if (estimate.amount === null) return amount;
  return estimate.partial ? `≥${amount}` : amount;
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
            : `${estimate.partial ? 'Known session cost subtotal' : 'Session cost'} ${label}`
        }
      >
        {label}
      </Button>
    </TooltipTrigger>
  );
}

function totalLabel(estimate: CostEstimate, currency = '$') {
  if (estimate.amount === null) return 'Total cost: — (pricing unavailable)';
  const label = formatCostEstimate(estimate, currency);
  return estimate.partial ? `Known subtotal: ${label}` : `Total: ${label}`;
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
        setPricingFailed(costData === null);
      } catch {
        setPricingFailed(true);
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
    const cacheReadTotal = sumNullableTokens(modelCostRows, 'cacheReadTokens');
    const cacheCreationTotal = sumNullableTokens(modelCostRows, 'cacheCreationTokens');
    const exactTotal = sumBilledTokens(modelCostRows);
    return (
      <Tooltip>
        <CostTrigger estimate={estimate} />
        <TooltipContent className="max-w-none">
          <div className="flex flex-col gap-2">
            <div className="whitespace-pre-line">
              {`${billedTokensSummary(
                inputTotal,
                outputTotal,
                cacheReadTotal,
                cacheCreationTotal,
                exactTotal
              )}\n${BILLED_EXPLAINER}`}
            </div>
            <div className="font-medium">Per-model breakdown</div>
            <ModelBreakdownTable rows={modelCostRows} />
            <div className="text-right font-medium">{totalLabel(estimate)}</div>
            {estimate.partial && (
              <div className="max-w-xl text-text-muted">
                This is a lower bound because pricing is unavailable for one or more models or token
                buckets.
              </div>
            )}
          </div>
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
        cacheRead: sum.cacheRead + (row.cacheReadTokens ?? 0),
        cacheCreation: sum.cacheCreation + (row.cacheCreationTokens ?? 0),
      }),
      { input: 0, output: 0, cacheRead: 0, cacheCreation: 0 }
    );
    return (
      <Tooltip>
        <CostTrigger estimate={estimate} />
        <TooltipContent className="whitespace-pre-line">
          {`${billedTokensSummary(
            totals.input,
            totals.output,
            totals.cacheRead,
            totals.cacheCreation
          )}\n${BILLED_EXPLAINER}\n\n${totalLabel(estimate)}`}
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
          {`${pricingFailed ? 'Pricing data unavailable' : 'Cost data not available'} for ${currentProvider}/${currentModel}\n${billedTokensSummary(inputTokens, outputTokens)}\n${BILLED_EXPLAINER}`}
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
        {`${billedTokensSummary(inputTokens, outputTokens)}\n${BILLED_EXPLAINER}\n\nFresh-token subtotal: ${formatCostEstimate(estimate, costInfo.currency)}\nCache buckets are not available until the session ledger loads.`}
      </TooltipContent>
    </Tooltip>
  );
}
