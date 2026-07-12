# Notification & Alert Redesign — Design Spec

**Date:** 2026-07-12 · **Status:** Draft for approval · **Owner:** UI hardening pass
**Design authority:** [`design.md`](../../../design.md) §4.2 (close affordance), §4.3 (toasts & inline alerts)
**Chosen direction:** **B — tinted icon chip** (approved 2026-07-12)

---

## 1. Problem

Notification pop-ups (toasts + inline alerts) have two visible defects and a layer of theme/token
drift beneath them:

1. **Text and action buttons collide with the × close mark.** The close-button gutter is reserved
   *once* by react-toastify's vendor CSS (`padding-inline-end: 38px`), but individual callers *also*
   add their own right padding (`ToastErrorContent` adds `pr-8`; `GroupedExtensionLoadingToast` adds
   `pr-8` on its summary yet only `pr-2` on its list rows). The reservation is therefore doubled in
   some places and **too small in others** — long extension names in a "Failed: …" line wrap *under*
   the ×, and the "Ask biorouter / Copy error" cluster is shoved into the × zone.
2. **The success / error / loading logo floats in the middle.** The status glyph is pinned
   `align-self: center` and lives *outside* the text, so it centres against the toast's **total
   height**. On a one-line toast this looks fine; on a title + wrapped body, or the expanded failure
   list (~256px tall), the icon drifts to the vertical midpoint and reads as belonging to nothing.

**Root cause (single):** the toast owns none of its own layout — not the close-gutter reservation, not
the icon's cross-axis alignment. Both defects fall out of that.

Underneath, geometry and status colours are hardcoded raw Tailwind rather than the `design.md` token
layer: `rounded-2xl` (16px vs the canonical 12px), `border-white/20` dividers (invisible in light
mode), `opacity-90/75` instead of muted-text tokens, and `toastSuccess`/`toastLoading` **silently drop
the message** when no title is passed. The same class of bug is re-implemented independently in the
inline `AlertBox` and ~40 ad-hoc inline banners.

Full evidence: audit catalogue produced 2026-07-12 (18 defects, root-cause synthesis).

## 2. Scope

**In scope (chosen: "Toasts + shared alert primitive"):**

- The transient toast system: `toasts.tsx` (`toastSuccess` / `toastError` / `toastLoading` /
  `extensionLoading`), the `<ToastContainer>` in `App.tsx`, and the `.Toastify__*` rules in `main.css`.
- The multi-state `GroupedExtensionLoadingToast`.
- **A new shared notification primitive** that the toast content, the grouped toast, and the inline
  alerts all render through, so the layout is owned in exactly one place.
- The inline `AlertBox` (context-window alert in the side popover) migrated onto the primitive.
- `SystemNotificationInline` given a real info-alert affordance.

**Follow-up (not blocking this pass):** bulk-migrating all ~40 ad-hoc inline-banner call sites onto the
new primitive. The primitive ships and is adopted by `AlertBox` + a few high-traffic sites now; the
long tail is mechanical and can land incrementally without re-litigating the design.

**Out of scope:** native OS notifications (`main.ts`) — they render through the OS, not our CSS.

## 3. The design — Direction B

### 3.1 The shared primitive

One component owns the notification/alert layout. Working name: `NotificationSurface` (in
`components/alerts/`), rendering a status-typed row:

```
┌───────────────────────────────────────────────┐
│ ▢  Title (text-default, 600)              (×)  │   ▢ = 28px tinted icon chip, TOP-aligned
│    Message (text-muted, 13/18)                 │   × = 20px ghost, in a gutter reserved ONCE
│    [ action ] [ action ]                       │
│    ── hairline ──                              │   (optional expanded region: grouped toast)
│    · detail rows (scroll, capped) ·            │
└───────────────────────────────────────────────┘
```

**Layout contract (fixes both defects at the source):**

| Property | Value |
|---|---|
| Container | `display:flex; gap:11px`; padding `12px 40px 12px 13px` — the **40px right gutter is the close reservation, owned here once**. Callers add **no** `pr-*`. |
| Icon chip | 28px square, `--radius-md` (8px), `--fill-{status}` @ ~10% (light) / ~15% (dark); glyph 16px `--text-{status}`, 1.5px stroke; **`align-self:flex-start`** so it anchors to the title's first line at any height. |
| Title | 13/18, weight 600, `--text-default`; rendered **only if provided**. |
| Message | 13/18, `--text-muted`; rendered **independently of the title** (no more silent drops). Wraps with `overflow-wrap:anywhere`. |
| Actions | 8px-gap row, `--motion`-free; buttons are `secondary`/ghost `sm`. |
| Close | 20px ghost, `--radius-sm`, `right:10px top:10px` (top-aligned, pairs with the title), 14px glyph, hover `--background-medium`. |
| Surface | `--background-default`, 1px `--border-subtle`, `--radius-lg` (12px). Toast adds `--elev-popover`; **inline banner variant drops elevation** (`flat`). |
| Status | Encoded by the **chip tint + icon colour only** — surface stays neutral (honours "colour is evidence; surfaces stay neutral"). No 3px left bar (that was Direction A). |

Statuses: `success`, `error`(danger), `warning`, `info`, `loading` (neutral chip + spinner).

### 3.2 Toast wiring

React-toastify draws its **own** status icon in a fixed slot and centres it — that is the floating
icon. We **disable it** (`icon: false`) and render the chip *inside* our content component instead, so
the icon lives in the same flex context as the text and top-aligns naturally. The `<ToastContainer>`
`toastClassName` becomes a thin token-based shell (surface + radius + elevation + the 40px gutter); all
per-caller `pr-*` is removed. Behaviour preserved: `autoClose` 5s (errors sticky), top-right,
`pauseOnHover`, `closeOnClick` (except the grouped toast, which stays `closeOnClick:false`).

### 3.3 Grouped-extension toast

Re-rendered through the primitive: summary = title + "Failed: …" message; the per-extension list moves
into the primitive's **expanded region** below a `--border-subtle` hairline (not `border-white/20`),
with `space-y` from the scale, a capped `max-height` scroll area with `scroll-padding` breathing room,
and one icon system (8px status dots for rows; the chip for the toast-level status). Secondary text
uses `--text-muted` / `--text-subtle`, not `opacity-*`.

### 3.4 Inline alerts

`AlertBox` and `SystemNotificationInline` render the primitive with `flat` (no elevation) and
status-tinted chip. `AlertBox`'s progress/threshold interior is preserved; only its shell + icon
alignment + status tokens change (drop `bg-background-danger text-white`, `text-neutral-900`,
`dark:bg-white dark:text-black` for `--fill/--text-{status}`).

## 4. Content variants the design must handle

Loading spinner · one-line success · **message-only, no title** · title + single-line body · title +
multi-line wrapped body · long error, no buttons · error + one button · error + two buttons ·
grouped "all loaded" · grouped "N failed" expanded with a **scrolling** list · inline single-line ·
inline multi-line · inline with an action link · warning banner in a form. All shown in the approved
mockup (`.superpowers/brainstorm/…/direction-B-full.html`), light + dark.

## 5. Tokens

Reuse existing tokens; **add `--fill-{status}`** (success/danger/warning/info) at the agreed opacity as
real tokens (today only `--background-{status}` exists and callers use ad-hoc `/10`). Light ≈ 10%,
dark ≈ 15% over `--background-default`. Radius `--radius-lg`/`-md`/`-sm`, elevation `--elev-popover`,
text `--text-default`/`-muted`/`-subtle`, hairline `--border-subtle` — all already defined.

## 6. Testing & verification

- **Unit (Vitest):** the primitive renders title-only, message-only, both, and neither gracefully;
  icon is `flex-start`-aligned; the close gutter class is present exactly once (no caller `pr-*`);
  status → chip-tint/icon-colour mapping; `toastSuccess`/`toastLoading` no longer drop the message.
- **Contrast:** `--fill-{status}` chip + `--text-{status}` glyph pairs added to
  `scripts/check-contrast.mjs`.
- **Visual sweep:** the browser mockup is the reference; after wiring, drive the real dev GUI
  (`debug-app`) to fire each toast variant + the grouped toast in both themes and confirm no overlap /
  no floating icon.
- **Regression:** `npm run test:run`, `npm run lint:check`, `npx tsc --noEmit` clean.

## 7. Files

`ui/desktop/src/toasts.tsx` · `App.tsx` · `styles/main.css` (`.Toastify__*`, add `--fill-{status}`) ·
`components/GroupedExtensionLoadingToast.tsx` · new `components/alerts/NotificationSurface.tsx` ·
`components/alerts/AlertBox.tsx` · `components/context_management/SystemNotificationInline.tsx` ·
tests alongside.

## 8. Risks

- **react-toastify icon slot:** disabling the built-in icon must not break `isLoading`/spinner
  semantics — verify the loading state still animates.
- **Bulk inline-alert migration** is deferred to avoid a large unverifiable diff in one pass.
- **Dark-mode `--fill-{status}` opacity** needs an eyes-on check on the real dark surface, not just the
  mockup ground.
