# Theming

This folder holds BioRouter's theme families — the named colour environments the desktop app
can be dressed in, selected independently of light/dark mode. It covers the colour tokens each
family overrides, the palettes they draw from, the accent decisions behind them, and the
verified WCAG contrast ratios for every value. Alma Mater, the UCSF-brand family, has shipped
and is specified here in full; Roche Limit is a proposal that has not shipped.

Come here when you need the authoritative value for a colour token, when you are adding a
component that reads colour tokens and need to know what it will look like in all four
environments, or when you are proposing a new theme family. This folder is colour only — it
deliberately does not cover layout, typography or motion. For the parent design system and its
numbered decisions see [`design.md`](../../../design.md) at the repo root; for the app-wide
structural redesign work see [`../ui-overhaul/`](../ui-overhaul/); for the logo, wordmark and
brand assets see [`../branding/`](../branding/).

## Documents

| Document | What it covers |
|---|---|
| [Alma Mater theme tokens](alma-mater-theme-tokens.md) | The colour-token reference for Alma Mater, BioRouter's UCSF-brand theme family: the UCSF palette it draws from, the accent decision, a complete token-by-token light/dark mapping, and the verified WCAG contrast ratios behind every value. The theme was approved with the Eggplant accent, implemented on 2026-07-10 and has shipped; these tables remain the authoritative source for the values. |

## Interactive studios

These are self-contained HTML pages that render themes on real BioRouter chrome. **They need a
browser to be useful** — open them locally; they are not readable as source.

| File | What it is |
|---|---|
| `theme-system-explorer.html` | "One interface. Four colour environments." — how the token layers and theme axes fit together across both shipped families, showing Parchment and Alma Mater in light and dark modes. |
| `alma-mater-theme-studio.html` | The Alma Mater studio: renders the token set on real BioRouter chrome in light and dark, and carries the accent picker used to compare options while the accent was being chosen. |
| `alma-mater-light-theme-studio.html` | A deeper exploration of Alma Mater's light variant (UCSF Teal). |
| `roche-limit-theme-studio.html` | A proposal for **Roche Limit**, a third theme family drawn from JupyterLab — a white page, recessed grey panels, and one bright orange doing all the signalling. Family id `roche-limit`, light + dark, 59 × 2 tokens, 164/164 contrast pairs at AA. Marked *awaiting approval*; nothing shipped. |

## Related documentation

- [Biorouter Design System](../../../design.md) — the parent design system that these themes
  re-colour, and the register of the numbered `D-NN` decisions the token reference cites.
- [UI overhaul execution status](../ui-overhaul/execution-status.md) — records the sweep that
  verified light + dark × Parchment + Alma Mater through the real persistence path.
- [BioRouter logo and wordmark specification](../branding/logo-and-wordmark-spec.md) — the brand
  assets that sit alongside these palettes, including their own colour tokens.
- [UI cohesion redesign](../ui-overhaul/ui-cohesion-redesign.md) — the structural counterpart:
  the markdown layer, preview panel, terminal and floating surfaces these tokens are applied to.
