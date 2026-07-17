import type { CSSProperties, HTMLAttributes } from 'react';
import glyphUrl from '../../images/glyph.svg';

export function BioRouter({ className = '', style, ...props }: HTMLAttributes<HTMLSpanElement>) {
  // The url() MUST be quoted. Vite inlines glyph.svg as a `data:` URI whose
  // markup uses single quotes and parentheses (`version='1.0'`,
  // `transform='rotate(-90 434.5 177.25)'`). An UNQUOTED url() token may not
  // contain `(`, `'` or whitespace — the tokenizer produces a bad-url-token,
  // which invalidates the whole declaration, so the browser dropped mask-image
  // on the floor. The mark paints `currentColor` and relies on the mask to cut
  // its shape, so it rendered as a SOLID BLOCK of the accent colour — a
  // failure that looks deliberate, which is how it survived.
  //
  // This is also why the old `--biorouter-glyph` indirection failed: a custom
  // property carrying a bad-url-token is itself invalid, so it never reached
  // the style attribute at all and `var(--biorouter-glyph)` resolved to
  // nothing. Quoting fixes both; the indirection bought nothing and is gone.
  const maskImage = `url("${glyphUrl}")`;
  const markStyle = {
    ...style,
    display: 'block',
    backgroundColor: 'currentColor',
    WebkitMaskImage: maskImage,
    maskImage,
    WebkitMaskPosition: 'center',
    maskPosition: 'center',
    WebkitMaskRepeat: 'no-repeat',
    maskRepeat: 'no-repeat',
    WebkitMaskSize: 'contain',
    maskSize: 'contain',
  } as CSSProperties;

  return (
    <span
      role="img"
      aria-label="Biorouter logo"
      className={className}
      {...props}
      style={markStyle}
    />
  );
}

export function Rain({ className = '' }) {
  return (
    <svg
      width="103"
      height="103"
      viewBox="0 0 103 103"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
      className={className}
    >
      <g id="wind">
        <g id="1" className="animate-[wind_2s_linear_infinite]">
          <path
            id="Vector 42_2"
            d="M66 33L62 37"
            stroke="url(#paint10_linear_2096_5587)"
            strokeLinecap="round"
          />
          <path
            id="Vector 41_2"
            d="M70 43L68 45"
            stroke="url(#paint11_linear_2096_5587)"
            strokeLinecap="round"
          />
          <path
            id="Vector 51_2"
            d="M60 34L59 35"
            stroke="url(#paint12_linear_2096_5587)"
            strokeLinecap="round"
          />
          <path
            id="Vector 43_2"
            d="M56 47L52 51"
            stroke="url(#paint13_linear_2096_5587)"
            strokeLinecap="round"
          />
          <path
            id="Vector 46_2"
            d="M98 1L94 5"
            stroke="url(#paint14_linear_2096_5587)"
            strokeLinecap="round"
          />
          <path
            id="Vector 47_2"
            d="M102 11L100 13"
            stroke="url(#paint15_linear_2096_5587)"
            strokeLinecap="round"
          />
          <path
            id="Vector 48_2"
            d="M88 15L84 19"
            stroke="url(#paint16_linear_2096_5587)"
            strokeLinecap="round"
          />
          <path
            id="Vector 45_2"
            d="M76 26L72 30"
            stroke="url(#paint17_linear_2096_5587)"
            strokeLinecap="round"
          />
          <path
            id="Vector 49_2"
            d="M87 28L83 32"
            stroke="url(#paint18_linear_2096_5587)"
            strokeLinecap="round"
          />
          <path
            id="Vector 50_2"
            d="M76 34L74 36"
            stroke="url(#paint19_linear_2096_5587)"
            strokeLinecap="round"
          />
        </g>
        <g id="2" className="animate-[wind_2s_linear_1s_infinite]">
          <path
            id="Vector 42"
            d="M66 33L62 37"
            stroke="url(#paint0_linear_2096_5587)"
            strokeLinecap="round"
          />
          <path
            id="Vector 41"
            d="M70 43L68 45"
            stroke="url(#paint1_linear_2096_5587)"
            strokeLinecap="round"
          />
          <path
            id="Vector 51"
            d="M60 34L59 35"
            stroke="url(#paint2_linear_2096_5587)"
            strokeLinecap="round"
          />
          <path
            id="Vector 43"
            d="M56 47L52 51"
            stroke="url(#paint3_linear_2096_5587)"
            strokeLinecap="round"
          />
          <path
            id="Vector 46"
            d="M98 1L94 5"
            stroke="url(#paint4_linear_2096_5587)"
            strokeLinecap="round"
          />
          <path
            id="Vector 47"
            d="M102 11L100 13"
            stroke="url(#paint5_linear_2096_5587)"
            strokeLinecap="round"
          />
          <path
            id="Vector 48"
            d="M88 15L84 19"
            stroke="url(#paint6_linear_2096_5587)"
            strokeLinecap="round"
          />
          <path
            id="Vector 45"
            d="M76 26L72 30"
            stroke="url(#paint7_linear_2096_5587)"
            strokeLinecap="round"
          />
          <path
            id="Vector 49"
            d="M87 28L83 32"
            stroke="url(#paint8_linear_2096_5587)"
            strokeLinecap="round"
          />
          <path
            id="Vector 50"
            d="M76 34L74 36"
            stroke="url(#paint9_linear_2096_5587)"
            strokeLinecap="round"
          />
        </g>
      </g>
      <defs>
        <linearGradient
          id="paint0_linear_2096_5587"
          x1="64"
          y1="32.9437"
          x2="64"
          y2="37.0563"
          gradientUnits="userSpaceOnUse"
        >
          <stop style={{ stopColor: 'var(--color-coral-500)' }} />
          <stop offset="1" style={{ stopColor: 'var(--color-coral-600)' }} />
        </linearGradient>
        <linearGradient
          id="paint1_linear_2096_5587"
          x1="69"
          y1="42.9719"
          x2="69"
          y2="45.0281"
          gradientUnits="userSpaceOnUse"
        >
          <stop style={{ stopColor: 'var(--color-coral-500)' }} />
          <stop offset="1" style={{ stopColor: 'var(--color-coral-600)' }} />
        </linearGradient>
        <linearGradient
          id="paint2_linear_2096_5587"
          x1="59.5"
          y1="33.9859"
          x2="59.5"
          y2="35.0141"
          gradientUnits="userSpaceOnUse"
        >
          <stop style={{ stopColor: 'var(--color-coral-500)' }} />
          <stop offset="1" style={{ stopColor: 'var(--color-coral-600)' }} />
        </linearGradient>
        <linearGradient
          id="paint3_linear_2096_5587"
          x1="54"
          y1="46.9437"
          x2="54"
          y2="51.0563"
          gradientUnits="userSpaceOnUse"
        >
          <stop style={{ stopColor: 'var(--color-coral-500)' }} />
          <stop offset="1" style={{ stopColor: 'var(--color-coral-600)' }} />
        </linearGradient>
        <linearGradient
          id="paint4_linear_2096_5587"
          x1="96"
          y1="0.943728"
          x2="96"
          y2="5.05625"
          gradientUnits="userSpaceOnUse"
        >
          <stop style={{ stopColor: 'var(--color-coral-500)' }} />
          <stop offset="1" style={{ stopColor: 'var(--color-coral-600)' }} />
        </linearGradient>
        <linearGradient
          id="paint5_linear_2096_5587"
          x1="101"
          y1="10.9719"
          x2="101"
          y2="13.0281"
          gradientUnits="userSpaceOnUse"
        >
          <stop style={{ stopColor: 'var(--color-coral-500)' }} />
          <stop offset="1" style={{ stopColor: 'var(--color-coral-600)' }} />
        </linearGradient>
        <linearGradient
          id="paint6_linear_2096_5587"
          x1="86"
          y1="14.9437"
          x2="86"
          y2="19.0563"
          gradientUnits="userSpaceOnUse"
        >
          <stop style={{ stopColor: 'var(--color-coral-500)' }} />
          <stop offset="1" style={{ stopColor: 'var(--color-coral-600)' }} />
        </linearGradient>
        <linearGradient
          id="paint7_linear_2096_5587"
          x1="74"
          y1="25.9437"
          x2="74"
          y2="30.0563"
          gradientUnits="userSpaceOnUse"
        >
          <stop style={{ stopColor: 'var(--color-coral-500)' }} />
          <stop offset="1" style={{ stopColor: 'var(--color-coral-600)' }} />
        </linearGradient>
        <linearGradient
          id="paint8_linear_2096_5587"
          x1="85"
          y1="27.9437"
          x2="85"
          y2="32.0563"
          gradientUnits="userSpaceOnUse"
        >
          <stop style={{ stopColor: 'var(--color-coral-500)' }} />
          <stop offset="1" style={{ stopColor: 'var(--color-coral-600)' }} />
        </linearGradient>
        <linearGradient
          id="paint9_linear_2096_5587"
          x1="75"
          y1="33.9719"
          x2="75"
          y2="36.0281"
          gradientUnits="userSpaceOnUse"
        >
          <stop style={{ stopColor: 'var(--color-coral-500)' }} />
          <stop offset="1" style={{ stopColor: 'var(--color-coral-600)' }} />
        </linearGradient>
        <linearGradient
          id="paint10_linear_2096_5587"
          x1="64"
          y1="32.9437"
          x2="64"
          y2="37.0563"
          gradientUnits="userSpaceOnUse"
        >
          <stop style={{ stopColor: 'var(--color-coral-500)' }} />
          <stop offset="1" style={{ stopColor: 'var(--color-coral-600)' }} />
        </linearGradient>
        <linearGradient
          id="paint11_linear_2096_5587"
          x1="69"
          y1="42.9719"
          x2="69"
          y2="45.0281"
          gradientUnits="userSpaceOnUse"
        >
          <stop style={{ stopColor: 'var(--color-coral-500)' }} />
          <stop offset="1" style={{ stopColor: 'var(--color-coral-600)' }} />
        </linearGradient>
        <linearGradient
          id="paint12_linear_2096_5587"
          x1="59.5"
          y1="33.9859"
          x2="59.5"
          y2="35.0141"
          gradientUnits="userSpaceOnUse"
        >
          <stop style={{ stopColor: 'var(--color-coral-500)' }} />
          <stop offset="1" style={{ stopColor: 'var(--color-coral-600)' }} />
        </linearGradient>
        <linearGradient
          id="paint13_linear_2096_5587"
          x1="54"
          y1="46.9437"
          x2="54"
          y2="51.0563"
          gradientUnits="userSpaceOnUse"
        >
          <stop style={{ stopColor: 'var(--color-coral-500)' }} />
          <stop offset="1" style={{ stopColor: 'var(--color-coral-600)' }} />
        </linearGradient>
        <linearGradient
          id="paint14_linear_2096_5587"
          x1="96"
          y1="0.943728"
          x2="96"
          y2="5.05625"
          gradientUnits="userSpaceOnUse"
        >
          <stop style={{ stopColor: 'var(--color-coral-500)' }} />
          <stop offset="1" style={{ stopColor: 'var(--color-coral-600)' }} />
        </linearGradient>
        <linearGradient
          id="paint15_linear_2096_5587"
          x1="101"
          y1="10.9719"
          x2="101"
          y2="13.0281"
          gradientUnits="userSpaceOnUse"
        >
          <stop style={{ stopColor: 'var(--color-coral-500)' }} />
          <stop offset="1" style={{ stopColor: 'var(--color-coral-600)' }} />
        </linearGradient>
        <linearGradient
          id="paint16_linear_2096_5587"
          x1="86"
          y1="14.9437"
          x2="86"
          y2="19.0563"
          gradientUnits="userSpaceOnUse"
        >
          <stop style={{ stopColor: 'var(--color-coral-500)' }} />
          <stop offset="1" style={{ stopColor: 'var(--color-coral-600)' }} />
        </linearGradient>
        <linearGradient
          id="paint17_linear_2096_5587"
          x1="74"
          y1="25.9437"
          x2="74"
          y2="30.0563"
          gradientUnits="userSpaceOnUse"
        >
          <stop style={{ stopColor: 'var(--color-coral-500)' }} />
          <stop offset="1" style={{ stopColor: 'var(--color-coral-600)' }} />
        </linearGradient>
        <linearGradient
          id="paint18_linear_2096_5587"
          x1="85"
          y1="27.9437"
          x2="85"
          y2="32.0563"
          gradientUnits="userSpaceOnUse"
        >
          <stop style={{ stopColor: 'var(--color-coral-500)' }} />
          <stop offset="1" style={{ stopColor: 'var(--color-coral-600)' }} />
        </linearGradient>
        <linearGradient
          id="paint19_linear_2096_5587"
          x1="75"
          y1="33.9719"
          x2="75"
          y2="36.0281"
          gradientUnits="userSpaceOnUse"
        >
          <stop style={{ stopColor: 'var(--color-coral-500)' }} />
          <stop offset="1" style={{ stopColor: 'var(--color-coral-600)' }} />
        </linearGradient>
      </defs>
    </svg>
  );
}
