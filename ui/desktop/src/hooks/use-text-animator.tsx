import SplitType from 'split-type';
import { useEffect, useRef } from 'react';

interface TextSplitterOptions {
  resizeCallback?: () => void;
  splitTypeTypes?: ('lines' | 'words' | 'chars')[];
}

// Class to split text into lines, words, and characters for animation
export class TextSplitter {
  textElement: HTMLElement;
  onResize: (() => void) | null;
  splitText: SplitType;
  previousContainerWidth: number | null = null;

  constructor(textElement: HTMLElement, options: TextSplitterOptions = {}) {
    if (!textElement || !(textElement instanceof HTMLElement)) {
      throw new Error('Invalid text element provided.');
    }

    const { resizeCallback, splitTypeTypes } = options;
    this.textElement = textElement;
    this.onResize = typeof resizeCallback === 'function' ? resizeCallback : null;

    const splitOptions = splitTypeTypes ? { types: splitTypeTypes } : {};
    this.splitText = new SplitType(this.textElement, splitOptions);

    if (this.onResize) {
      this.initResizeObserver();
    }
  }

  initResizeObserver() {
    // Use a simpler approach to avoid type issues
    const resizeObserver = new ResizeObserver(() => {
      // Just check the current width directly from the element
      if (this.textElement) {
        const currentWidth = Math.floor(this.textElement.getBoundingClientRect().width);

        if (this.previousContainerWidth && this.previousContainerWidth !== currentWidth) {
          this.splitText.split({ types: ['chars'] });
          this.onResize?.();
        }

        this.previousContainerWidth = currentWidth;
      }
    });

    resizeObserver.observe(this.textElement);
  }

  getLines(): HTMLElement[] {
    return this.splitText.lines ?? [];
  }

  getChars(): HTMLElement[] {
    return this.splitText.chars ?? [];
  }
}

export class TextAnimator {
  textElement: HTMLElement;
  splitter!: TextSplitter;
  originalChars!: string[];
  activeAnimations: globalThis.Animation[] = [];
  activeTimeouts: ReturnType<typeof setTimeout>[] = [];

  constructor(textElement: HTMLElement) {
    if (!textElement || !(textElement instanceof HTMLElement)) {
      throw new Error('Invalid text element provided.');
    }

    this.textElement = textElement;
    this.splitText();
  }

  private splitText() {
    this.splitter = new TextSplitter(this.textElement, {
      splitTypeTypes: ['words', 'chars'],
    });
    this.originalChars = this.splitter.getChars().map((char) => char.textContent || '');
  }

  animate() {
    this.reset();

    const chars = this.splitter.getChars();

    chars.forEach((char, position) => {
      char.style.opacity = '0';
      char.style.display = 'inline-block';
      char.style.position = 'relative';
      char.style.transform = 'translateX(-0.18em)';
      char.style.filter = 'blur(2px)';

      const animation = char.animate(
        [
          {
            opacity: 0,
            transform: 'translateX(-0.18em)',
            filter: 'blur(2px)',
          },
          {
            opacity: 0.7,
            transform: 'translateX(-0.04em)',
            filter: 'blur(0.6px)',
          },
          {
            opacity: 1,
            transform: 'translateX(0)',
            filter: 'blur(0)',
          },
        ],
        {
          duration: 420,
          easing: 'cubic-bezier(0.22, 1, 0.36, 1)',
          delay: position * 12,
          iterations: 1,
          fill: 'forwards',
        }
      );

      this.activeAnimations.push(animation);

      animation.onfinish = () => {
        char.style.opacity = '1';
        char.style.transform = '';
        char.style.filter = '';
      };
    });
  }

  reset() {
    // Clear all timeouts
    this.activeTimeouts.forEach((timeoutId) => clearTimeout(timeoutId));
    this.activeTimeouts = [];

    // Cancel all animations
    this.activeAnimations.forEach((animation) => animation.cancel());
    this.activeAnimations = [];

    // Reset text content
    const chars = this.splitter.getChars();
    chars.forEach((char, index) => {
      if (this.originalChars[index]) {
        char.textContent = this.originalChars[index];
      }
      char.style.opacity = '';
      char.style.transform = '';
      char.style.filter = '';
    });
  }
}

interface UseTextAnimatorProps {
  text: string;
}

export function useTextAnimator({ text }: UseTextAnimatorProps) {
  const elementRef = useRef<HTMLSpanElement>(null);
  const animator = useRef<TextAnimator | null>(null);

  useEffect(() => {
    if (!elementRef.current) return;

    if (window.matchMedia?.('(prefers-reduced-motion: reduce)').matches) {
      return;
    }

    // Create animator
    animator.current = new TextAnimator(elementRef.current);

    // Small delay to ensure content is ready
    const timeoutId = setTimeout(() => {
      animator.current?.animate();
    }, 100);

    // Cleanup
    return () => {
      window.clearTimeout(timeoutId);
      if (animator.current) {
        animator.current.reset();
      }
    };
  }, [text]); // Re-run when text changes

  return elementRef;
}
