// Single source of truth for the "Built-in" badge so it renders identically
// everywhere (Skills view, Extensions view, chat dropdowns, slash popover).
//
// `text-caps` is the one caps style (11/16/500, +0.08em); it replaces the old
// sub-11px literal together with its hand-rolled weight and tracking — nothing
// below 11px survives the type roles. `leading-none` is a DELIBERATE override of
// the role's 16px line-height: this badge sits inline inside dense list rows and
// must not add height to every row it appears in.
export default function BuiltInBadge({
  title = 'Ships with Biorouter.\nCan be toggled off but not deleted.',
}: {
  title?: string;
}) {
  return (
    <span
      className="inline-flex flex-shrink-0 items-center rounded-inner bg-background-strong/50 px-1 py-[1px] text-caps leading-none text-text-muted/90 uppercase"
      title={title}
    >
      Built-in
    </span>
  );
}
