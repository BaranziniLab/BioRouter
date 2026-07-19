/**
 * Shared theme-token parsing and cascade resolution.
 *
 * One implementation, used by every guard that needs to know what a token
 * actually resolves to in a given theme: check-contrast.mjs, the codegen's
 * regression check, and the duplication assertions. Previously each of these
 * reimplemented the parser and the WCAG maths, which is how a syntax palette
 * came to be verified against a surface the app never painted.
 *
 * The family list is DISCOVERED from the stylesheet, not hardcoded. Adding a
 * theme family therefore requires no edit here and no edit in the guards — the
 * new family is picked up and audited automatically.
 */

/** Body of every block whose selector satisfies `test`. */
export function blocks(css, test) {
  const out = [];
  const re = /(^|\n)([^\n{}]+)\{/g;
  let m;
  while ((m = re.exec(css))) {
    const selector = m[2].trim();
    if (!test(selector)) continue;
    let depth = 1;
    let i = re.lastIndex;
    for (; i < css.length && depth > 0; i++) {
      if (css[i] === '{') depth++;
      else if (css[i] === '}') depth--;
    }
    out.push({ selector, body: css.slice(re.lastIndex, i - 1) });
  }
  return out;
}

export function parseDecls(body) {
  const o = {};
  // Strip nested blocks (e.g. @keyframes inside @theme) before reading decls.
  const flat = body.replace(/[^{}]*\{[^{}]*\}/g, '');
  for (const m of flat.matchAll(/(--[\w-]+)\s*:\s*([^;]+);/g)) o[m[1]] = m[2].trim();
  return o;
}

const merge = (...bs) => Object.assign({}, ...bs.map((b) => parseDecls(b.body)));

/**
 * Discover every theme family declared in the stylesheet.
 *
 * Matches `[data-theme='x']` only on the two token-block selectors, so the
 * per-family component overrides (e.g. the modal scrim rule) do not register
 * as families.
 */
export function discoverFamilies(css) {
  const found = new Set();
  for (const { selector } of blocks(css, () => true)) {
    const m = selector.match(/^(?::root|\.dark)\[data-theme='([^']+)'\]$/);
    if (m) found.add(m[1]);
  }
  return [...found].sort();
}

/**
 * Build the resolved scope for every family × mode in the stylesheet, plus the
 * bare Parchment default under the id `parchment`.
 *
 * The cascade is modelled the way the browser applies it:
 *   light  = :root                 < :root[data-theme=X]
 *   dark   = :root < .dark         < :root[data-theme=X] < .dark[data-theme=X]
 *
 * NOTE ON SOURCE ORDER: `:root[data-theme=X]` and `.dark[data-theme=X]` have
 * IDENTICAL specificity (0,2,0), so in the browser only document order
 * separates them — the dark block must physically follow the light one. This
 * function assumes that ordering; `assertBlockOrder` below verifies it, because
 * a file where they are swapped renders light tokens in dark mode while every
 * contrast assertion still passes.
 */
export function buildScopes(css) {
  const THEME = merge(...blocks(css, (s) => s.startsWith('@theme')));
  const LIGHT = merge(...blocks(css, (s) => s === ':root'));
  const DARK = merge(...blocks(css, (s) => s === '.dark'));

  const scopes = {
    'parchment:light': { decls: { ...LIGHT }, theme: THEME },
    'parchment:dark': { decls: { ...LIGHT, ...DARK }, theme: THEME },
  };

  for (const family of discoverFamilies(css)) {
    const L = merge(...blocks(css, (s) => s === `:root[data-theme='${family}']`));
    const D = merge(...blocks(css, (s) => s === `.dark[data-theme='${family}']`));
    scopes[`${family}:light`] = { decls: { ...LIGHT, ...L }, theme: THEME };
    scopes[`${family}:dark`] = { decls: { ...LIGHT, ...DARK, ...L, ...D }, theme: THEME };
  }
  return scopes;
}

/**
 * Verify that each family's dark block physically follows its light block.
 * Equal specificity means source order decides; getting this wrong renders
 * light tokens in dark mode with no assertion failing.
 */
export function assertBlockOrder(css) {
  const problems = [];
  for (const family of discoverFamilies(css)) {
    const li = css.indexOf(`:root[data-theme='${family}'] {`);
    const di = css.indexOf(`.dark[data-theme='${family}'] {`);
    if (li === -1 || di === -1) continue;
    if (di < li) {
      problems.push(
        `${family}: .dark[data-theme='${family}'] appears BEFORE :root[data-theme='${family}']. ` +
          `They have equal specificity (0,2,0), so dark mode will render light tokens.`
      );
    }
  }
  return problems;
}

/** Resolve a token through its var() chain to a literal. Returns null if unset. */
export function resolveRaw(name, scope) {
  let v = scope.decls[name] ?? scope.theme[name];
  for (let i = 0; i < 12 && v && v.startsWith('var('); i++) {
    const inner = v.slice(4, v.indexOf(')')).trim();
    v = scope.decls[inner] ?? scope.theme[inner];
  }
  return v ? v.trim() : null;
}

/** Resolve to a 6-digit hex, or null if the value is not one. */
export function resolveHex(name, scope) {
  const v = resolveRaw(name, scope);
  return v && /^#[0-9a-f]{6}$/i.test(v) ? v.toLowerCase() : null;
}

/* ── WCAG maths — the single implementation ── */

export const luminance = (h) => {
  // FAIL CLOSED. This slices fixed hex offsets, so any other notation silently
  // yields NaN — and `if (ratio < floor)` is FALSE for NaN, which means a
  // non-hex colour would quietly exempt itself from every contrast check that
  // uses a bare comparison. The generator's terminal and splash assertions had
  // exactly that hole: `mark.navy: 'rgb(27, 27, 25)'` emitted an invisible
  // 1.09:1 boot mark with every gate green. Throwing here closes it for every
  // caller at once rather than per-assertion.
  if (typeof h !== 'string' || !/^#[0-9a-fA-F]{6}$/.test(h.trim())) {
    throw new TypeError(
      `luminance() needs a 6-digit hex, got ${JSON.stringify(h)}. ` +
        `Non-hex values cannot be contrast-checked — give the token a hex, or ` +
        `exclude it from the check explicitly.`
    );
  }
  const c = [1, 3, 5]
    .map((i) => parseInt(h.slice(i, i + 2), 16) / 255)
    .map((v) => (v <= 0.03928 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4)));
  return 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
};

export const contrast = (a, b) => {
  const [x, y] = [luminance(a), luminance(b)];
  return (Math.max(x, y) + 0.05) / (Math.min(x, y) + 0.05);
};
