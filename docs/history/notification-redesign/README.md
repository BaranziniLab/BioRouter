# Notification redesign

This folder documents completed work and is kept for the record, not as current guidance. It
holds a single design spec, written 2026-07-12, for `NotificationSurface` — one shared
primitive that owns the layout of both transient toasts and inline alerts in the desktop app,
replacing per-caller padding and react-toastify's centred status icon. **The work happened and
the primitive shipped:** `ui/desktop/src/components/alerts/` now contains
`NotificationSurface.tsx` and `AlertBox.tsx` with tests under `__tests__/`. The current truth
about how the component behaves is therefore the source itself, not this spec. One piece of
the plan is explicitly unfinished — the spec deferred bulk-migrating the roughly 40 ad-hoc
inline banners onto the primitive, recorded no tracking issue for that tail, and it should be
assumed still open.

Come here when you are asking why notification layout is owned by a single component rather
than by each caller, why the toast's built-in icon is disabled, or where the `--fill-{status}`
tokens came from. If you instead want the design rules the spec answers to, that is the root
[Biorouter Design System](../../../design.md) (§4.2 and §4.3), and the colour tokens it
consumes are in [`docs/design/theming/`](../../design/theming/README.md). If you want to
reproduce or test notification behaviour in the running app, go to
[`docs/desktop-ui/`](../../desktop-ui/README.md) instead — nothing here is a checklist to
re-run.

> **Note.** Two references the spec treats as authoritative are unreachable from this
> repository: the 2026-07-12 audit catalogue of 18 defects that forms its evidence base, and
> the approved mockup `direction-B-full.html` that showed every content variant in light and
> dark. The spec records no path to either.

## Documents

| Document | What it covers |
|---|---|
| [Notification surface design](notification-surface-design.md) | The design spec for `NotificationSurface`: the two visible defects (text colliding with the × close mark, and a status icon floating in the middle of tall toasts), their single root cause — the toast owns none of its own layout — and the layout contract, toast wiring, grouped-extension toast, inline alerts, tokens and verification plan that fix them at the source. Direction B, the tinted icon chip, was the approved treatment. |

## Related documentation

- [Biorouter Design System](../../../design.md) — the design authority this spec answers to;
  §4.2 covers the close affordance and §4.3 covers toasts and inline alerts.
- [Alma Mater theme tokens](../../design/theming/alma-mater-theme-tokens.md) — the token layer
  (`--text-*`, `--border-subtle`, `--background-*`) the design consumes instead of the raw
  Tailwind it replaces.
- [Desktop reliability defects — July 2026](../subsystem-reviews-2026/desktop-reliability-defects.md)
  — the wider July 2026 desktop defect batch, which also covers notification defects and
  records the fixes that shipped alongside this one.
- [Debugging the dev GUI with agent-browser](../../desktop-ui/agent-browser-debugging.md) —
  how to drive the real dev GUI, which is what the spec's visual-sweep verification step
  requires.
