import { clsx, type ClassValue } from 'clsx';
import { extendTailwindMerge } from 'tailwind-merge';

/**
 * tailwind-merge has to be TAUGHT the design system's own scales.
 *
 * It ships knowing Tailwind's stock scales only. Anything else in the `text-*`
 * namespace it classifies as a text COLOUR — so `cn('text-text-inverse',
 * 'text-supporting')` decided the two were the same class group and dropped the
 * colour, silently rendering a tooltip's ink in the default colour on an inverse
 * fill. Every semantic type role (`text-label`, `text-body`, …) hits this the
 * moment a call site pairs it with a `text-text-*` colour, which is the normal
 * case. Registering them under the `text` theme key (Tailwind v4's font-size
 * namespace) restores the correct two groups: one size, one colour.
 *
 * The radius ladder has the mirror problem in reverse: `rounded-element` and
 * `rounded-container` were both unknown, so both SURVIVED a merge and a call
 * site's override no longer beat a primitive's default — source order decided.
 *
 * Keep these two lists in step with the `@theme` block in `src/styles/main.css`.
 */
const twMerge = extendTailwindMerge({
  extend: {
    theme: {
      text: [
        'display',
        'title',
        'heading',
        'subheading',
        'body',
        'label',
        'secondary',
        'supporting',
        'caps',
        'chip',
        'code',
      ],
      radius: ['inner', 'element', 'container', 'surface'],
    },
  },
});

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

export function snakeToTitleCase(snake: string): string {
  return snake
    .split('_')
    .map((word) => word.charAt(0).toUpperCase() + word.slice(1).toLowerCase())
    .join(' ');
}
