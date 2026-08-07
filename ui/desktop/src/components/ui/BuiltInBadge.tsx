import { Badge } from './badge';

// Single source of truth for the "Built-in" badge so it renders identically
// everywhere (Skills view, Extensions view, chat dropdowns, slash popover).
//
// It is now literally the `Badge` primitive rather than a fifth hand-rolled
// small-label recipe that merely looked like one (§3.4). What it used to author
// for itself — a 50%-diluted surface, a 1px vertical padding, a 90%-diluted
// muted ink and a `leading-none` override — was four separate values that no
// other small label in the app shared, and each was a place the two could drift.
// "Built-in" is a STATUS ("this ships with Biorouter"), so it takes the 20px
// badge tier, which is short enough to sit inside a 32px row without adding
// height to it — the thing `leading-none` was there to guarantee.
export default function BuiltInBadge({
  title = 'Ships with Biorouter.\nCan be toggled off but not deleted.',
}: {
  title?: string;
}) {
  return (
    <Badge tone="neutral" uppercase title={title}>
      Built-in
    </Badge>
  );
}
