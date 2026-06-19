// Single source of truth for the "Built-in" badge so it renders identically
// everywhere (Skills view, Extensions view, chat dropdowns, slash popover).
export default function BuiltInBadge({
  title = 'Ships with Biorouter. Can be toggled off but not deleted.',
}: {
  title?: string;
}) {
  return (
    <span
      className="text-[11px] uppercase tracking-wider px-1.5 py-0.5 rounded bg-background-strong text-text-default flex-shrink-0"
      title={title}
    >
      Built-in
    </span>
  );
}
