# Managed / enterprise policy (BR-65)

BioRouter's hooks and permissions are normally configured per user (global
`~/.config/biorouter/config.yaml`) and, opt-in, per project
(`.biorouter/hooks.yaml`). Both tiers are owned by the user, so a lab or
institutional deployment has no way to enforce a rule the user cannot turn off.

The **managed policy tier** adds a trusted, admin-owned layer that wins over user
and project config for two governance surfaces:

- **Permissions** — force certain tools to be denied, to require approval, or to
  be pre-approved, regardless of the user's own `permission.yaml` or the active
  permission mode (including `Auto`).
- **Hooks** — mandatory lifecycle hooks that always run and cannot be disabled,
  plus an override that forces or forbids project hooks org-wide.

Precedence is:

```
Default (built-in)  <  User (global config)  <  Project (opt-in)  <  Managed (admin)
```

Managed **deny** and **ask** always win. Managed **allow** is applied as the
permission baseline (ahead of the user's table), so it can pre-approve a tool the
user marked "Never Allow" — but a higher managed **deny** or a security inspector
can still escalate it. `deny` is checked before `ask` before `allow`.

## File location

The managed file is read from an admin-owned, per-OS location. There is **no**
environment-variable override in production (an env var is user-settable and
would defeat the tamper model):

| OS      | Path                                                            |
| ------- | -------------------------------------------------------------- |
| macOS   | `/Library/Application Support/BioRouter/managed-policy.yaml`    |
| Linux   | `/etc/biorouter/managed-policy.yaml`                           |
| Windows | `%ProgramData%\BioRouter\managed-policy.yaml`                  |

Deploy it via MDM/Jamf (macOS), a package postinstall or Ansible (Linux), or
Group Policy / an installer (Windows).

## Ownership verification

Before the file is parsed, BioRouter verifies that neither the file nor its
parent directory can be rewritten by a non-privileged user (mirroring Gemini
CLI's ownership verification against privilege escalation):

- **Unix** — the file and its parent directory must be owned by `root` (uid 0)
  **or** the current user, and must **not** be group- or world-writable
  (`mode & 0o022 == 0`). Install with, e.g.:

  ```bash
  sudo install -o root -g wheel -m 0755 -d /etc/biorouter
  sudo install -o root -g wheel -m 0644 managed-policy.yaml /etc/biorouter/
  ```

- **Windows** — `%ProgramData%` is admin-writable-only by default, so phase 1
  trusts the location; deep ACL owner verification is a planned follow-up.

A file that fails the check (untrusted owner, world-writable, unreadable, or
malformed YAML) is **ignored with a warning** and the agent runs normally. This
is a deliberate availability-over-strictness choice: a bad MDM push must not
brick every lab machine.

## Schema

```yaml
# Managed hooks: identical schema to the user `hooks:` map. They always run and
# resolve BEFORE user/project hooks. A managed Stop/PreToolUse block cannot be
# cleared by a user hook (merge takes the most-restrictive decision).
hooks:
  PreToolUse:
    - matcher: "developer__shell"
      hooks:
        - type: command
          command: "/usr/local/lib/biorouter/managed-shell-guard.sh"
  Stop:
    - hooks:
        - type: prompt
          prompt: "Block stopping until the audit log has been written."

# Override the user's `allow_project_hooks` opt-in.
#   false -> forbid project hooks org-wide (even if the user set true)
#   true  -> force project hooks on (even if the user did not opt in)
#   (omit) -> leave the user/env value untouched
allow_project_hooks: false

permissions:
  # Force auto-approve (applied as the permission baseline). Wins over a user
  # "Never Allow" for the same tool.
  allow:
    - "developer__text_editor"
  # Force an approval prompt (escalation; non-bypassable).
  ask:
    - "memory__.*"          # exact tool name or anchored regex (matcher.rs)
  # Force a hard denial (escalation; non-bypassable, works even in Auto mode).
  deny:
    - "developer__shell"

  # Reserved for future command-level policy (BR-20/BR-21). Parsed but inert in
  # this release, so a managed file can carry these forward without breaking
  # older binaries.
  command_rules: []
```

Tool names in `allow`/`ask`/`deny` are matched exactly or as an anchored regex,
the same matcher the hooks engine uses (so `a|b` alternation and `foo__.*` both
work).

## Behavior notes

- **No managed file → zero change.** If the file is absent or untrusted, the
  managed inspector is skipped and hook resolution is byte-for-byte the previous
  behavior. Solo users pay nothing.
- **Managed deny works in every mode**, including `Auto`. It rides the existing
  escalation-only inspection merge, so no later inspector can lower it. A tool
  the user "Always Allow"-ed silently becomes denied, surfaced as
  *"Blocked by your organization's managed policy."*
- **The managed file is read-only** to BioRouter; the agent never writes it. No
  session, `permission.yaml`, or `.biorouter/hooks.yaml` state changes.

## Roadmap (phase 2+)

- Windows ACL owner verification (`Administrators`/`SYSTEM`).
- A `biorouter policy show` subcommand to print the resolved managed layer and
  its trust status for deployment verification.
- BR-20 (catastrophic denylist) and BR-21 (argv/prefix command policy) evaluate
  `permissions.command_rules` inside the managed inspector.
