# BR-65 — Managed/enterprise policy tier for guardrails and hooks

**Lens:** R (robustness P-50). **Inspired by:** Gemini CLI tiered policy engine
(`Default < Extension < Workspace < User < Admin`, admin always wins,
ownership-verified — `docs/agent-loop-review/external/gemini-cli.md:141-144,248-250`)
and Claude Code managed settings (`/Library/Application Support/…`,
`/etc/claude-code/…` read last / highest precedence —
`docs/agent-loop-review/external/claude-code.md:40-42`, deny/ask always win —
`:160-162`).

Depends on / is the umbrella for: **BR-20** (always-on catastrophic denylist),
**BR-21** (auditable command policy engine). BR-65 supplies the *tier/precedence
machinery* those two rule sets plug into; it is designed to land useful on its
own (managed hooks + managed permission rules) before BR-20/BR-21 exist.

---

## Problem (grounded in code, with file:line)

BioRouter's two governance surfaces each have exactly **two** configuration
tiers — a user-global layer and an opt-in project layer — and **no
non-overridable admin layer**, so a lab/UCSF deployment cannot enforce a policy
that a user cannot turn off.

**Hooks (2 tiers, both user-controlled).**
`HooksManager` resolves only global + (opt-in) project groups:

- `crates/biorouter/src/hooks/mod.rs:159-169` — `resolved_groups()` = global
  groups `++` project groups; that is the entire precedence chain.
- `crates/biorouter/src/hooks/config.rs:111-120` — `load_global_config()` reads
  `~/.config/biorouter/config.yaml` `hooks:` (env override `BIOROUTER_HOOKS`).
- `crates/biorouter/src/hooks/config.rs:131-143` — `parse_project_hooks()` reads
  `.biorouter/hooks.yaml` from the session working dir.
- `crates/biorouter/src/hooks/mod.rs:74-79` — project hooks are gated by a
  **single global boolean** `allow_project_hooks` (or
  `BIOROUTER_ALLOW_PROJECT_HOOKS=1`). It is all-or-nothing and the user owns it,
  so an org cannot *force* a security hook nor *forbid* project hooks.
- Internal review calls this out directly: `internal/hooks.md:204` ("Only 2
  config tiers"), `:278-280` (gap #12 "Two config tiers only, no
  managed/enterprise layer — no way for an org to enforce a non-overridable
  security hook, and project hooks are all-or-nothing").

**Permissions / command policy (also user-controlled, and mostly off).**

- `crates/biorouter/src/permission/permission_inspector.rs:106-188` — the live
  gate. It consults only `PermissionManager.get_user_permission` (a per-user
  `permission.yaml`) plus the always-empty `readonly_tools`/`regular_tools`
  sets. In `Auto` mode it returns `Allow` for everything (`:121-122`) with **no**
  screening.
- `crates/biorouter/src/config/permission.rs:11-12,42-62` — `PermissionManager`
  is a singleton backed by one user file `~/.config/biorouter/permission.yaml`;
  there is no higher-precedence source.
- `crates/biorouter/src/security/mod.rs` — the regex scanner is
  `SECURITY_PROMPT_ENABLED=false` by default and, when on, only *asks*
  (`internal/guardrails-permissions.md:114`, gap #3). So `Auto` mode = zero
  command governance and nothing an admin can pin.

**Enforcement seam already exists (good).** Non-`permission` inspector results
are merged as **escalation-only overrides** — any inspector can move a request
`approved → needs_approval → denied` but an `Allow` override is a no-op
(`crates/biorouter/src/tool_inspection.rs:217-257`). This means a new
managed-policy inspector's `Deny`/`RequireApproval` will win **even over `Auto`
mode's `Allow`** and cannot be lowered by a later inspector — exactly the
non-bypassable property a managed tier needs. The gap is purely that no such
tier is loaded, and that it must sit at a trusted, admin-owned location the user
cannot rewrite.

Net: the machinery to *enforce* a stricter decision exists; what's missing is
(1) a trusted config source with **ownership verification**, (2) a **precedence
model** that puts it above user/project for both hooks and permissions, and (3)
the wiring to load it.

---

## Design (data model, module layout, key APIs/signatures, control flow)

### Precedence model

Adopt Gemini's ordering, collapsed to BioRouter's surfaces:

```
Default(builtin)  <  User(global config)  <  Project(opt-in)  <  Managed(admin)
```

Managed **wins** for both hooks and permissions. Two rule kinds with different
merge semantics (this asymmetry is the crux and must be explicit):

- **Managed DENY / ASK** — enforced through the *existing escalation-only*
  override merge (`tool_inspection.rs`). Non-bypassable by construction; works in
  every mode including `Auto`. No change to merge code.
- **Managed ALLOW** (force-approve, reduce prompts) — escalation-only merge
  *cannot* lower a user `Deny`, so a managed ALLOW must be applied as the
  **baseline**, ahead of the user permission lookup, inside `PermissionInspector`.
  Managed ALLOW is a convenience, not a security control, so putting it in the
  baseline (where a *higher* managed DENY or a security inspector can still
  escalate) is correct.
- **Managed hooks** — resolved **first** in `resolved_groups`, always run,
  cannot be disabled; managed may also **force** `allow_project_hooks` on/off,
  overriding the user's opt-in.

### Trusted config location + ownership verification

New paths (production, no env override for tamper-resistance — only the existing
test seam `BIOROUTER_PATH_ROOT` is honored):

- macOS: `/Library/Application Support/BioRouter/managed-policy.yaml`
- Linux: `/etc/biorouter/managed-policy.yaml`
- Windows: `%ProgramData%\BioRouter\managed-policy.yaml`

**Files to change:** `crates/biorouter/src/config/paths.rs` — add

```rust
pub fn managed_policy_path() -> Option<PathBuf>  // None on unsupported/env-test override handling
```

returning the per-OS constant (honoring `BIOROUTER_PATH_ROOT` → `<root>/managed/managed-policy.yaml`
so tests can point at it).

**Ownership verification** (new `managed/trust.rs`): before parsing, `stat` the
file and its parent dir and require they are owned by a privileged principal and
not world-writable — mirroring Gemini's "ownership verification against
privilege escalation" (`external/gemini-cli.md:143-144`):

- Unix: `std::os::unix::fs::MetadataExt` — require `uid == 0` (root) **or**
  `uid == <current euid>` for a user-mode dev/test install, and reject if
  `mode & 0o022 != 0` (group/other writable). A managed file that fails the
  check is **ignored with a `warn!`** (fail-open on *presence*, because a
  corrupt/unsafe managed file must not brick the agent), but its failure is
  surfaced as a startup diagnostic (see BR-67 observability tie-in).
- Windows (phase 2): verify the ACL owner is `Administrators`/`SYSTEM` via
  `windows` crate; until then, restrict to `%ProgramData%` (already
  admin-writable-only by default) and skip deep ACL checks.

### Module layout (new)

```
crates/biorouter/src/managed/
  mod.rs        // ManagedPolicy, load(), resolved accessors
  settings.rs   // on-disk schema (serde) + parse
  trust.rs      // ownership/permission verification
```

Re-exported as `crate::managed`.

### Data model (`managed/settings.rs`)

```rust
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
pub struct ManagedPolicyFile {
    /// Managed hooks: same schema as HooksConfig event map. Always run,
    /// resolved before user/project groups.
    #[serde(default)]
    pub hooks: crate::hooks::HooksConfig,

    /// If set, overrides the user's allow_project_hooks opt-in (Some(false)
    /// forbids project hooks org-wide; Some(true) forces them on).
    #[serde(default)]
    pub allow_project_hooks: Option<bool>,

    /// Managed permission rules over tool names (exact or the same
    /// anchored-regex matcher hooks already use, matcher.rs).
    #[serde(default)]
    pub permissions: ManagedPermissions,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
pub struct ManagedPermissions {
    #[serde(default)] pub allow: Vec<String>, // force auto-approve (baseline)
    #[serde(default)] pub ask:   Vec<String>, // force RequireApproval (escalation)
    #[serde(default)] pub deny:  Vec<String>, // force Deny (escalation)
    /// Reserved for BR-21: argv/prefix command rules keyed by tool (e.g. shell).
    #[serde(default)] pub command_rules: Vec<ManagedCommandRule>,
}
```

`deny` is checked before `ask` before `allow` (deny/ask always win —
`external/claude-code.md:160-162`). The `command_rules` field is the extension
point BR-20's catastrophic list and BR-21's policy engine deserialize into; BR-65
ships it as an inert, forward-compatible field (unknown → skipped, matching the
`HooksConfig` forward-compat pattern at `config.rs:78-104`).

### Runtime type (`managed/mod.rs`)

```rust
pub struct ManagedPolicy {
    file: ManagedPolicyFile,
    source: Option<PathBuf>,   // None when absent/untrusted → all queries inert
}

impl ManagedPolicy {
    /// Load + verify once at startup. Absent or untrusted → empty (inert).
    pub fn load() -> Arc<ManagedPolicy>;

    pub fn is_active(&self) -> bool;                 // a trusted file was loaded
    pub fn hooks(&self) -> &HooksConfig;
    pub fn project_hooks_override(&self) -> Option<bool>;

    /// Managed verdict for a tool name, or None if no managed rule applies.
    /// Deny > Ask > Allow.
    pub fn permission_for(&self, tool_name: &str) -> Option<ManagedVerdict>;
}

pub enum ManagedVerdict { Allow, Ask, Deny }
```

### Wiring

**1. Hooks (`crates/biorouter/src/hooks/mod.rs`).**
Add `managed: Arc<ManagedPolicy>` field to `HooksManager`. In `new()` load it;
compute effective `allow_project_hooks` = `managed.project_hooks_override()`
if `Some`, else the current user/env value (`mod.rs:74-79`). In
`resolved_groups` (`:159-169`) prepend managed groups:

```rust
let mut groups = self.managed.hooks().events.get(&event).cloned().unwrap_or_default();
groups.extend(self.global.events.get(&event).cloned().unwrap_or_default());
if effective_allow_project_hooks { groups.extend(project_groups); }
```

Merge semantics are unchanged (most-restrictive decision wins,
`outcome.rs`), so a managed `Stop`/`PreToolUse` block already cannot be undone by
a user hook. `HooksConfig`/`HookMatcherGroup` are reused verbatim — no new hook
schema.

**2. Permissions — new `ManagedPolicyInspector`
(`crates/biorouter/src/permission/managed_inspector.rs`).**
A `ToolInspector` named `"managed"` holding `Arc<ManagedPolicy>`. Registered
**first**, before `security`, in `Agent::build_tool_inspection_manager`
(`agents/agent.rs:342-361`):

```rust
tool_inspection_manager.add_inspector(Box::new(ManagedPolicyInspector::new(managed.clone())));
tool_inspection_manager.add_inspector(Box::new(SecurityInspector::new()));
// ...permission, repetition, hooks as today
```

- `is_enabled()` returns `managed.is_active()`, so a machine with no managed file
  pays nothing (mirrors `SecurityInspector::is_enabled`,
  `security_inspector.rs:89-92`).
- `inspect()` emits `Deny` / `RequireApproval` for tools hit by managed
  `deny`/`ask`. Because it is a non-`permission` inspector, these ride the
  escalation-only merge (`tool_inspection.rs:217-257`) and win over everything,
  including `Auto` `Allow`. It emits **no** result for `allow` tools (allow is
  handled in the baseline, below).

**3. Managed ALLOW baseline (`permission/permission_inspector.rs:123-150`).**
Give `PermissionInspector` an `Arc<ManagedPolicy>` and check it *first* inside
the `Approve | SmartApprove` arm (before the user-permission lookup at `:125`):

```rust
if let Some(v) = self.managed.permission_for(tool_name) {
    match v {
        ManagedVerdict::Deny  => InspectionAction::Deny,          // belt-and-suspenders
        ManagedVerdict::Ask   => InspectionAction::RequireApproval(Some("Managed policy".into())),
        ManagedVerdict::Allow => InspectionAction::Allow,          // managed force-allow
    }
} else { /* existing user-permission / readonly / default logic */ }
```

This makes managed ALLOW win over a user `NeverAllow` (which escalation could not
achieve) while managed DENY is enforced both here and in the dedicated inspector.

### Control flow (one tool call, managed file present)

```
model emits tool requests
 └─ inspect_tools() runs inspectors in order:
     managed  → Deny/Ask for governed tools (escalation, non-bypassable)
     security → (off by default)
     permission → managed Allow baseline first, else user/mode logic
     repetition, hooks (managed hook groups resolved first, always run)
 └─ process_inspection_results_with_permission_inspector()
     → escalation-only merge: managed Deny/Ask cannot be lowered
 └─ dispatch / human-approval / declined
```

---

## Alternatives considered (and why rejected)

1. **Add a `managed:` section inside the existing user `config.yaml`.** Rejected:
   the user owns and can edit that file, so it provides zero enforcement. A
   managed tier must live at an admin-owned path the user cannot write
   (`external/claude-code.md:40-42`).
2. **One combined "settings tiers" rewrite (managed/user/project/local) for all
   config, à la Claude Code's four-file `settings.json` cascade.** Rejected for
   this BR: far larger blast radius (every config key gains precedence
   semantics). BR-65 is scoped to the two *governance* surfaces the review flags;
   a general settings cascade can reuse `ManagedPolicy::load` + `trust.rs` later.
3. **Enforce managed rules purely via the escalation-only merge (no baseline
   change).** Rejected: escalation cannot express managed **ALLOW** (can't lower
   a user Deny), so org-wide "these tools are pre-approved, stop prompting" —
   half the value for a trusted lab deployment — would be impossible. Hence the
   small `PermissionInspector` baseline hook.
4. **Reuse `PermissionManager` with a second "managed" category key** (like the
   existing `user`/`smart_approve` keys, `permission.rs:39-40`). Rejected: that
   store is a single user-writable file loaded from `Paths::config_dir()`; it has
   no trust boundary and no ownership check. Managed policy needs its own trusted
   source.
5. **Honor an env override (`BIOROUTER_MANAGED_POLICY`) for the managed path.**
   Rejected in production: an env var is trivially user-settable and defeats the
   tamper model. Only the pre-existing test seam `BIOROUTER_PATH_ROOT` is honored,
   and only to relocate the whole config root under test.

---

## Migration & compatibility (config, persisted state, rollout)

- **Backward compatible / opt-in by absence.** No managed file → `is_active()`
  false → the managed inspector is skipped and `resolved_groups`/baseline are
  byte-for-byte today's behavior. Zero change for solo users.
- **No persisted-state change.** Sessions, `permission.yaml`, `.biorouter/hooks.yaml`
  are untouched. `ManagedPolicy` is read-only and never written by the agent.
- **Config schema is additive and forward-compatible.** `ManagedPolicyFile`
  reuses `HooksConfig` (unknown events skipped, `config.rs:78-104`) and skips
  unknown top-level keys, so `command_rules` (BR-20/BR-21) can be added later
  without breaking older binaries that ignore it.
- **Rollout.** Ship the loader + hooks tier + permission deny/ask/allow first
  (phase 1). Admins deploy the file via MDM/Jamf (macOS), a package
  postinstall/Ansible (Linux). Document the path + ownership requirement in a new
  `docs/guides/managed-policy.md`. A `biorouter policy show` CLI subcommand
  (phase 2) prints the resolved managed layer + trust status so an admin can
  verify deployment.
- **Failure mode.** Untrusted or malformed managed file → ignored with a loud
  `warn!` + (BR-67) observability event; the agent still runs. This is a
  deliberate availability-over-strictness choice for a research tool; the human
  Q below asks whether an org should be able to opt into fail-closed.

---

## Test plan (unit/integration; what proves no regression)

Unit (`crates/biorouter/src/managed/`):
- `settings.rs`: parse a full managed file; unknown keys skipped; deny>ask>allow
  ordering in `permission_for`.
- `trust.rs`: a fixture file under `BIOROUTER_PATH_ROOT`; assert a
  world-writable (`0o666`) file is rejected and a `0o644` root-ish file accepted;
  absent file → inert.
- `mod.rs`: `is_active()` false when no file; `project_hooks_override` plumbing.

Inspector integration (`crates/biorouter/src/permission/`, `tool_inspection` tests):
- Managed `deny` on `developer__shell` → request ends in `denied` **even in
  `Auto` mode** (proves non-bypassable via escalation merge).
- Managed `ask` → `needs_approval` regardless of mode.
- Managed `allow` on a tool the user set `NeverAllow` → `approved` (proves
  baseline precedence; the one case escalation can't do).
- No managed file → inspector `is_enabled()` false, decisions identical to
  pre-change (regression guard).

Hooks integration (`crates/biorouter/src/hooks/` tests, extend existing
`config.rs`/`mod.rs` tests):
- Managed `Stop` hook block cannot be cleared by a user hook; managed groups
  resolve before global (`resolved_groups`).
- `allow_project_hooks: Some(false)` in managed suppresses a present
  `.biorouter/hooks.yaml` even when the user set `allow_project_hooks: true`.

No-regression proof: full `cargo test -p biorouter` (hooks + permission +
tool_inspection suites) unchanged when no managed file exists; the added tests
all key off the `BIOROUTER_PATH_ROOT` fixture so CI never touches real
`/etc` or `/Library`.

---

## Effort & phasing (first mergeable slice)

**Phase 1 (first PR, ~M):** `managed/{mod,settings,trust}.rs`; `paths.rs`
`managed_policy_path()`; `ManagedPolicyInspector` (deny/ask) + `PermissionInspector`
managed-allow baseline; `HooksManager` managed-hooks tier + `allow_project_hooks`
override; Unix ownership check; unit + inspector tests; `docs/guides/managed-policy.md`.
This alone delivers "org can force a non-overridable deny/ask + a mandatory Stop
hook, and forbid project hooks" — the review's core ask (`hooks.md:278-280`).

**Phase 2:** Windows ACL verification; `biorouter policy show` CLI; BR-67
observability events for load/trust-failure/managed-denial.

**Phase 3 (separate BRs, plug into `command_rules`):** BR-20 catastrophic
denylist and BR-21 argv/policy engine deserialize into `ManagedPermissions.command_rules`
and evaluate inside `ManagedPolicyInspector`.

---

## Open questions for the human (only genuine product decisions)

1. **Fail-open vs fail-closed on an untrusted/corrupt managed file.** Default
   proposed = ignore + warn (availability). Should an org be able to set
   `enforcement: strict` so a present-but-unverifiable managed file *halts* the
   agent instead? (Security posture vs. a bad MDM push bricking every lab
   machine.)
2. **Should managed ALLOW exist at all?** Force-allowing tools reduces prompts
   but weakens the "admin only tightens" mental model. Options: (a) allow it
   (proposed), (b) managed may only `deny`/`ask` (pure governance), (c) allow but
   only for tools with `read_only_hint`.
3. **Precedence of a managed DENY vs. a user AlwaysAllow — expected, but confirm
   the UX:** a tool the user "Always Allow"-ed silently becomes denied under a
   new managed policy. Do we surface a distinct "blocked by your organization's
   policy" message (vs. a generic decline) so users aren't confused? (Proposed:
   yes, dedicated reason string.)
4. **Non-Unix ownership story for phase 1.** Is macOS + Linux (`/etc`,
   `/Library`) sufficient for the initial UCSF deployment, deferring Windows ACL
   verification to phase 2 (Windows restricted to `%ProgramData%` until then)?
