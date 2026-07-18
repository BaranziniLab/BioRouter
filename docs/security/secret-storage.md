# Secret storage

Biorouter stores provider API keys and other secrets in the operating
system's native credential store: the **macOS Keychain**, the **Windows
Credential Manager**, or the **Linux Secret Service** (GNOME Keyring /
KWallet). Secrets never touch disk in plaintext, custody stays with the OS,
and access is gated by the same mechanisms the rest of the platform uses.

## macOS: the Keychain password prompt, explained

When a Biorouter binary reads the `biorouter` Keychain item for the first
time, macOS itself shows a password dialog — the OS asking you to authorize
*that specific executable* to read the item. Two facts shape the whole
experience:

- **"Allow" authorizes a single access; "Always Allow" is permanent.**
  Always Allow adds the binary to the Keychain item's access control list,
  so it is never asked again. Always click **Always Allow**.
- **Authorization is per binary.** The desktop app's backend (`biorouterd`)
  and the CLI (`biorouter`) are separate executables, so macOS tracks a
  separate grant for each.

### How many times will I type my password?

Biorouter reads the credential store **once per process** and caches
secrets in memory for the rest of the run, so a single authorization covers
an entire session — you will never see back-to-back identical prompts.

- **Desktop-only usage: zero prompts.** The binary that *creates* a
  Keychain item is trusted for it automatically. If you enter your API keys
  through the desktop app, `biorouterd` created the item and never prompts.
- **Adding the CLI (or vice versa): one prompt, once.** The other binary
  needs its own grant: type your password once and click **Always Allow**.
  Release builds are Developer ID-signed, so the grant survives app
  updates.

So in the worst case (using both the GUI and the CLI, with secrets created
by the other one) your password is needed **twice, ever** — once per
binary. Usually it is once or not at all.

### Why can't one password cover both binaries?

This is a deliberate macOS security boundary, not a Biorouter limitation.
The Keychain identifies an app by its code signature (its *designated
requirement*) and records consent per app, per item. If authorizing
`biorouterd` also authorized `biorouter`, any program on the system could
ride along on another app's grant — so the OS simply does not offer a
supported way for one executable's authorization to extend to another.
Apple's sanctioned mechanism for sharing (keychain access groups) requires
moving to the iOS-style data-protection keychain with provisioning-profile
entitlements, which the Rust `keyring` ecosystem doesn't support today.

### Developers: rebuilds re-prompt unless you sign

`cargo build` produces ad-hoc-signed binaries whose identity changes on
every build, so the Keychain treats each rebuild as a brand-new app and
re-prompts even after Always Allow. `just copy-binary` (used by
`just run-ui` / `just run-dev`) now automatically re-signs the freshly
built `biorouter`/`biorouterd` with the Developer ID certificate when it is
present in your keychain, giving dev builds a stable identity — Always
Allow then sticks across rebuilds. Without the certificate, set
`BIOROUTER_DISABLE_KEYRING=true` during development to use plaintext
`secrets.yaml` instead.

## Other platforms

- **Windows:** the Credential Manager never shows a prompt (credentials
  are protected per-user by DPAPI). Biorouter transparently splits large
  secret sets across multiple credentials to stay under Windows'
  2560-byte per-credential limit.
- **Linux:** desktop sessions unlock the Secret Service with your login;
  no extra prompts. On headless machines, SSH sessions, WSL, or systems
  without a Secret Service daemon, Biorouter automatically falls back to
  file storage (`~/.config/biorouter/secrets.yaml`).

## Escape hatches

| Variable | Effect |
|---|---|
| `BIOROUTER_DISABLE_KEYRING=true` | Store secrets in plaintext `~/.config/biorouter/secrets.yaml` instead of the OS credential store |
| `<KEY_NAME>` (e.g. `OPENAI_API_KEY`) | Any secret can be supplied directly via the environment; this always wins over the credential store |
