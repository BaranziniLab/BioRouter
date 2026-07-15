import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { cn } from '../../utils';
import { TOOLTIP_SURFACE_CLASS_NAME } from './Tooltip';

const TOOLTIP_ATTRIBUTE = 'data-biorouter-tooltip';
const GENERATED_ARIA_LABEL_ATTRIBUTE = 'data-biorouter-tooltip-aria-label';
const TOOLTIP_SELECTOR = `[${TOOLTIP_ATTRIBUTE}]`;
const INTERACTIVE_SELECTOR =
  'button, a[href], input, select, textarea, iframe, [role="button"], [role="menuitem"], [role="menuitemcheckbox"], [role="menuitemradio"]';

interface TooltipState {
  target: HTMLElement;
  text: string;
  x: number;
  y: number;
  side: 'top' | 'bottom';
}

function upgradeNativeTitle(element: HTMLElement) {
  const rawTitle = element.getAttribute('title');
  if (rawTitle === null) {
    element.removeAttribute(TOOLTIP_ATTRIBUTE);
    if (element.hasAttribute(GENERATED_ARIA_LABEL_ATTRIBUTE)) {
      element.removeAttribute('aria-label');
      element.removeAttribute(GENERATED_ARIA_LABEL_ATTRIBUTE);
    }
    return;
  }

  const title = rawTitle.trim();
  if (!title) return;

  element.setAttribute(TOOLTIP_ATTRIBUTE, title);
  element.setAttribute('title', '');

  if (
    element.matches(INTERACTIVE_SELECTOR) &&
    (element.hasAttribute(GENERATED_ARIA_LABEL_ATTRIBUTE) ||
      (!element.hasAttribute('aria-label') && !element.hasAttribute('aria-labelledby')))
  ) {
    element.setAttribute('aria-label', title);
    element.setAttribute(GENERATED_ARIA_LABEL_ATTRIBUTE, '');
  }
}

function upgradeTitlesWithin(root: HTMLElement) {
  if (root.hasAttribute('title')) upgradeNativeTitle(root);
  root.querySelectorAll<HTMLElement>('[title]').forEach(upgradeNativeTitle);
}

function tooltipTargetFromEvent(event: Event): HTMLElement | null {
  return event.target instanceof Element
    ? event.target.closest<HTMLElement>(TOOLTIP_SELECTOR)
    : null;
}

export function AppTooltipLayer() {
  const [tooltip, setTooltip] = useState<TooltipState | null>(null);
  const [horizontalShift, setHorizontalShift] = useState(0);
  const tooltipRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    upgradeTitlesWithin(document.body);

    const observer = new MutationObserver((records) => {
      records.forEach((record) => {
        if (record.type === 'attributes' && record.target instanceof HTMLElement) {
          upgradeNativeTitle(record.target);
          return;
        }

        record.addedNodes.forEach((node) => {
          if (node instanceof HTMLElement) upgradeTitlesWithin(node);
        });
      });
    });

    observer.observe(document.body, {
      attributeFilter: ['title'],
      attributes: true,
      childList: true,
      subtree: true,
    });

    const showTooltip = (event: Event) => {
      const target = tooltipTargetFromEvent(event);
      const text = target?.getAttribute(TOOLTIP_ATTRIBUTE)?.trim();
      if (!target || !text) return;

      const bounds = target.getBoundingClientRect();
      const side = bounds.top >= 48 ? 'top' : 'bottom';
      setHorizontalShift(0);
      setTooltip({
        target,
        text,
        x: bounds.left + bounds.width / 2,
        y: side === 'top' ? bounds.top - 8 : bounds.bottom + 8,
        side,
      });
    };

    const hideTooltip = (event?: Event) => {
      if (event?.type === 'pointerout') {
        const currentTarget = tooltipTargetFromEvent(event);
        const relatedTarget = (event as PointerEvent).relatedTarget;
        const nextTarget =
          relatedTarget instanceof Element
            ? relatedTarget.closest<HTMLElement>(TOOLTIP_SELECTOR)
            : null;
        if (currentTarget && currentTarget === nextTarget) return;
      }
      setTooltip(null);
    };

    const hideOnEscape = (event: KeyboardEvent) => {
      if (event.key === 'Escape') hideTooltip();
    };

    document.addEventListener('pointerover', showTooltip, true);
    document.addEventListener('pointerout', hideTooltip, true);
    document.addEventListener('focusin', showTooltip, true);
    document.addEventListener('focusout', hideTooltip, true);
    document.addEventListener('pointerdown', hideTooltip, true);
    document.addEventListener('keydown', hideOnEscape, true);
    window.addEventListener('blur', hideTooltip);
    window.addEventListener('resize', hideTooltip);
    window.addEventListener('scroll', hideTooltip, true);

    return () => {
      observer.disconnect();
      document.removeEventListener('pointerover', showTooltip, true);
      document.removeEventListener('pointerout', hideTooltip, true);
      document.removeEventListener('focusin', showTooltip, true);
      document.removeEventListener('focusout', hideTooltip, true);
      document.removeEventListener('pointerdown', hideTooltip, true);
      document.removeEventListener('keydown', hideOnEscape, true);
      window.removeEventListener('blur', hideTooltip);
      window.removeEventListener('resize', hideTooltip);
      window.removeEventListener('scroll', hideTooltip, true);
    };
  }, []);

  useLayoutEffect(() => {
    const surface = tooltipRef.current;
    if (!surface || !tooltip) return;

    const bounds = surface.getBoundingClientRect();
    const viewportPadding = 8;
    const unshiftedLeft = tooltip.x - bounds.width / 2;
    const unshiftedRight = tooltip.x + bounds.width / 2;
    let nextShift = 0;
    if (unshiftedLeft < viewportPadding) {
      nextShift = viewportPadding - unshiftedLeft;
    } else if (unshiftedRight > window.innerWidth - viewportPadding) {
      nextShift = window.innerWidth - viewportPadding - unshiftedRight;
    }

    if (nextShift !== horizontalShift) setHorizontalShift(nextShift);
  }, [horizontalShift, tooltip]);

  useEffect(() => {
    if (tooltip && !tooltip.target.isConnected) setTooltip(null);
  }, [tooltip]);

  if (!tooltip) return null;

  return createPortal(
    <div
      ref={tooltipRef}
      role="tooltip"
      data-slot="tooltip-content"
      data-side={tooltip.side}
      className={cn(
        TOOLTIP_SURFACE_CLASS_NAME,
        'pointer-events-none fixed max-w-xs whitespace-pre-line text-left leading-snug animate-in fade-in-0 zoom-in-95 duration-[var(--motion-fast)]'
      )}
      style={{
        left: tooltip.x + horizontalShift,
        top: tooltip.y,
        transform: tooltip.side === 'top' ? 'translate(-50%, -100%)' : 'translate(-50%, 0)',
      }}
    >
      {tooltip.text}
      <span
        aria-hidden="true"
        className={cn(
          'absolute left-1/2 size-2.5 -translate-x-1/2 rotate-45 bg-background-accent',
          tooltip.side === 'top' ? 'top-full -translate-y-[7px]' : 'bottom-full translate-y-[7px]'
        )}
      />
    </div>,
    document.body
  );
}
