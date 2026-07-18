import type { HTMLAttributes } from 'react';
import { BioRouter } from './BioRouter';

// Thin wrapper around the BioRouter mark. Retired from the live UI by the brand
// rollout (the sidebar/welcome now use BioRouterWordmark/BioRouterMark); kept so
// the icons barrel and its test still resolve. Props match BioRouter, which
// renders a masked <span>, not an <svg>.
export function BioRouterIcon(props: HTMLAttributes<HTMLSpanElement>) {
  return <BioRouter {...props} />;
}
