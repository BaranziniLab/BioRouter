import type { ModelCostRow } from '../../hooks/useCostTracking';
import { UNKNOWN_MODEL_LABEL } from '../../hooks/useCostTracking';
import { billedTokens, knownBilledTokens } from '../../utils/usageAccounting';
import { formatTooltipMoney } from './CostTracker';

interface ModelBreakdownTableProps {
  rows: ModelCostRow[];
  currency?: string;
}

/** Label a row's model, folding the null/unknown bucket to a readable name. */
export function modelLabel(row: Pick<ModelCostRow, 'provider' | 'model'>): string {
  if (!row.model) {
    return UNKNOWN_MODEL_LABEL;
  }
  return row.provider ? `${row.provider}/${row.model}` : row.model;
}

/** Per-token cost cell — an em dash when this model has no pricing entry. */
function costCell(
  row: Pick<ModelCostRow, 'totalCost' | 'costIsPartial'>,
  currency: string
): string {
  if (row.totalCost === null) return '—';
  const formatted = formatTooltipMoney(row.totalCost, currency);
  return row.costIsPartial ? `≥${formatted}` : formatted;
}

function tokenCell(tokens: number | null): string {
  return tokens === null ? '—' : tokens.toLocaleString();
}

function billedTokenCell(row: ModelCostRow): string {
  const exact = billedTokens(row);
  if (exact !== null) return tokenCell(exact);
  const subtotal = knownBilledTokens(row);
  return subtotal > 0 ? `≥${tokenCell(subtotal)}` : '—';
}

/**
 * Per-model usage breakdown for the cost popover, rendered from the real
 * `token_events` rollup (Issue #1). Rows arrive already priced; a `null` cost
 * is shown as an em dash so an unknown price never masquerades as free.
 */
export function ModelBreakdownTable({ rows, currency = '$' }: ModelBreakdownTableProps) {
  if (rows.length === 0) {
    return null;
  }

  return (
    <table className="w-full border-collapse text-left text-xs" data-testid="model-breakdown-table">
      <thead>
        <tr className="h-8 border-b border-border-subtle text-[11px] uppercase tracking-wider text-text-muted">
          <th className="pr-3 font-medium">Model</th>
          <th className="px-2 text-right font-medium">Turns</th>
          <th className="px-2 text-right font-medium">Fresh in</th>
          <th className="px-2 text-right font-medium">Cache read</th>
          <th className="px-2 text-right font-medium">Cache write</th>
          <th className="px-2 text-right font-medium">Out</th>
          <th className="px-2 text-right font-medium">Billed</th>
          <th className="pl-2 text-right font-medium">Cost</th>
        </tr>
      </thead>
      <tbody>
        {rows.map((row) => (
          <tr
            key={modelLabel(row)}
            className="h-10 border-b border-border-subtle text-text-default last:border-b-0"
          >
            <td className="pr-3 font-mono">{modelLabel(row)}</td>
            <td className="px-2 text-right tabular-nums">{row.turns.toLocaleString()}</td>
            <td className="px-2 text-right tabular-nums">{row.inputTokens.toLocaleString()}</td>
            <td className="px-2 text-right tabular-nums">{tokenCell(row.cacheReadTokens)}</td>
            <td className="px-2 text-right tabular-nums">{tokenCell(row.cacheCreationTokens)}</td>
            <td className="px-2 text-right tabular-nums">{row.outputTokens.toLocaleString()}</td>
            <td className="px-2 text-right tabular-nums">{billedTokenCell(row)}</td>
            <td className="pl-2 text-right tabular-nums">{costCell(row, currency)}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}
