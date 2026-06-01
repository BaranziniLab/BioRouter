export function RightSidePlaceholder() {
  return (
    <div className="flex flex-col h-full">
      <div className="flex items-center justify-between px-6 py-3 border-b border-border-subtle">
        <span className="text-xs text-text-muted">knowledge graph (coming in Plan 5)</span>
        <button
          disabled
          title="Coming in Plan 5"
          className="text-xs text-text-muted border border-border-subtle px-2.5 py-1 rounded-md opacity-50 cursor-not-allowed"
        >
          Change log
        </button>
      </div>
      <div className="flex-1 flex items-center justify-center text-text-muted text-sm">
        Graph view will render here once you ingest some sources.
      </div>
    </div>
  );
}
