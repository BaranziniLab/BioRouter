// Single source of truth for the "Built-in" badge so it renders identically
// everywhere (Skills view, Extensions view, chat dropdowns, slash popover).
export default function BuiltInBadge({
  title = 'Ships with BioRouter.\nCan be toggled off but not deleted.',
}: {
  title?: string;
}) {
  return (
    <span
      className="inline-flex flex-shrink-0 items-center rounded-sm bg-background-strong/50 px-1 py-[1px] text-[9px] font-medium leading-none text-text-muted/90 uppercase tracking-wide"
      title={title}
    >
      Built-in
    </span>
  );
}
