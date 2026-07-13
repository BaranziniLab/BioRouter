import { useEffect, useState } from 'react';
import { fetchModelPricing } from '../utils/pricing';
import { getSessionUsage, ModelUsageRow, Session } from '../api';

export interface ModelCostRow {
  /** Provider that served the turns, or undefined for the unknown bucket. */
  provider?: string;
  /** Model that served the turns, or undefined for the unknown bucket. */
  model?: string;
  inputTokens: number;
  outputTokens: number;
  totalTokens: number;
  turns: number;
  /** Client-side cost from the pricing table, or null when pricing is unknown. */
  totalCost: number | null;
}

interface UseCostTrackingProps {
  session?: Session | null;
}

/**
 * The label a `null` model_id row (turns recorded before model attribution, or
 * providers that reported no model) shows in the breakdown.
 */
export const UNKNOWN_MODEL_LABEL = 'unknown';

/** Stable key for a `(provider, model)` group, matching the old cost map keys. */
export function modelRowKey(provider?: string | null, model?: string | null): string {
  return `${provider ?? UNKNOWN_MODEL_LABEL}/${model ?? UNKNOWN_MODEL_LABEL}`;
}

/**
 * Price the real per-model rows the backend recorded. `pricingFor` returns the
 * per-token input/output cost for a `(provider, model)` pair, or null when the
 * pricing table has no entry — that row's cost stays null (token-only), never
 * zero, so an unknown price is visibly distinct from a genuinely free model.
 */
export async function buildModelCostRows(
  rows: ModelUsageRow[],
  pricingFor: (
    provider: string,
    model: string
  ) => Promise<{ input_token_cost?: number; output_token_cost?: number } | null>
): Promise<ModelCostRow[]> {
  return Promise.all(
    rows.map(async (row) => {
      const provider = row.provider ?? undefined;
      const model = row.modelId ?? undefined;
      let totalCost: number | null = null;
      if (provider && model) {
        const pricing = await pricingFor(provider, model);
        if (
          pricing &&
          (pricing.input_token_cost !== undefined || pricing.output_token_cost !== undefined)
        ) {
          totalCost =
            row.inputTokens * (pricing.input_token_cost || 0) +
            row.outputTokens * (pricing.output_token_cost || 0);
        }
      }
      return {
        provider,
        model,
        inputTokens: row.inputTokens,
        outputTokens: row.outputTokens,
        totalTokens: row.totalTokens,
        turns: row.turns,
        totalCost,
      };
    })
  );
}

/**
 * Per-model cost tracking backed by the real `token_events` ledger.
 *
 * The previous implementation *guessed* the split by watching model-change
 * events in the renderer and attributing the whole current session total to
 * whichever model was active at the moment of the switch — it never saw turns
 * from before the app opened and mis-attributed everything after a mid-thread
 * switch (Issue #1). This fetches the authoritative `(model, provider)` rollup
 * from `GET /sessions/{id}/usage` instead and only prices it client-side.
 */
export const useCostTracking = ({ session }: UseCostTrackingProps) => {
  const [modelRows, setModelRows] = useState<ModelCostRow[]>([]);

  const sessionId = session?.id;
  // Re-fetch when the accumulated total moves (a new billed turn landed) so the
  // breakdown tracks the conversation without polling.
  const accumulatedTotal = session?.accumulated_total_tokens ?? 0;

  useEffect(() => {
    let cancelled = false;

    const load = async () => {
      if (!sessionId) {
        setModelRows([]);
        return;
      }
      try {
        const response = await getSessionUsage({
          path: { session_id: sessionId },
          throwOnError: false,
        });
        const rows = response.data?.models ?? [];
        const priced = await buildModelCostRows(rows, fetchModelPricing);
        if (!cancelled) {
          setModelRows(priced);
        }
      } catch {
        if (!cancelled) {
          setModelRows([]);
        }
      }
    };

    load();
    return () => {
      cancelled = true;
    };
  }, [sessionId, accumulatedTotal]);

  // Back-compat shape for CostTracker: keyed `${provider}/${model}` map.
  const sessionCosts: {
    [key: string]: { inputTokens: number; outputTokens: number; totalCost: number };
  } = {};
  for (const row of modelRows) {
    sessionCosts[modelRowKey(row.provider, row.model)] = {
      inputTokens: row.inputTokens,
      outputTokens: row.outputTokens,
      totalCost: row.totalCost ?? 0,
    };
  }

  return {
    sessionCosts,
    modelRows,
  };
};
