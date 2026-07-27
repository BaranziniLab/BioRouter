# Biorouter persona for Jupyter AI

> **What this is.** The reference for the Jupyter AI integration: how the `@Biorouter` persona
> connects a Jupyter AI chat to a fully configured Biorouter agent over ACP, which versions the
> stack is pinned to and why, and the record of the acceptance runs that verified it.
> **Status:** Current. The persona ships in [`integrations/jupyter-ai/`](../../integrations/jupyter-ai/README.md)
> and landed on `main` in commit `708390d8`. The verification sections are dated records of
> specific runs; everything above them describes how the integration behaves now.
> **Audience:** developers working on the integration, and maintainers deciding whether to move
> the pinned JupyterLab / Jupyter AI versions.

Jupyter AI is JupyterLab's chat extension. Its *personas* are the chat participants a user
`@`-mentions, and its **ACP** (Agent Client Protocol) support lets a persona delegate a whole turn
to an external agent process instead of calling a model itself. This integration registers Biorouter
as one such persona: `@Biorouter` in a Jupyter AI chat runs the same agent as the desktop app and
the CLI — same model, providers, extensions, skills, workflows, knowledge bases, permissions, and
biomedical agents (SPOKE, OMOP, CDW) — inside the notebook.

## How it works

The integration is a thin `BaseAcpPersona` subclass registered through the `jupyter_ai.personas`
entry-point group. It launches `biorouter acp` over stdio and leaves model, extension, skill,
workflow, and permission ownership inside Biorouter:

```text
Jupyter AI chat
  -> Biorouter persona (`BaseAcpPersona` subclass)
  -> `biorouter acp` over stdio
  -> Biorouter Agent
      -> configured platform extensions (`skills`, `extensionmanager`, etc.)
      -> configured built-in and external MCP extensions
      -> Jupyter's notebook MCP server forwarded during ACP session creation
```

The implementation follows the
[`jupyter_ai.personas` contract](https://jupyter-ai.readthedocs.io/en/stable/developers/entry_points_api/personas_group.html)
and uses Jupyter AI's ACP base class for message processing, session recovery, streaming, tool-call
display, permission requests, cancellation, and MCP server forwarding. Because the base class does
that work, the package itself is two files: `persona.py` resolves the executable
(`BIOROUTER_EXECUTABLE`, else `biorouter` on `PATH`, else `PersonaRequirementsUnmet`) and declares
the persona's name, description and avatar; `pyproject.toml` registers the entry point
`biorouter-acp = jupyter_ai_biorouter.persona:BioRouterAcpPersona`.

Nothing in the Rust workspace is specific to this integration — it consumes the existing
`biorouter acp` subcommand, so the same ACP surface serves any other ACP-speaking host.

## Compatibility

| Component | Pinned version |
|---|---|
| Jupyter AI | 3.0.1 |
| JupyterLab | 4.5.9 |
| `jupyter-ai-acp-client` | 0.1.5 |
| `jupyter-ai-persona-manager` | 0.0.12 |
| Python (JupyterLab environment) | 3.9–3.12 |

JupyterLab 4.5.9 is intentionally pinned because Jupyter AI 3.0.1's frontend extensions are
incompatible with JupyterLab 4.6's `@jupyter/ydoc` major version. Python is capped at 3.12 because
Jupyter AI 3.0.1 does not yet support 3.13+.

## Installing and running

The install, run, verify and troubleshooting steps live with the package, in
[`integrations/jupyter-ai/README.md`](../../integrations/jupyter-ai/README.md), so that the commands
sit next to the `pyproject.toml` they install. In outline: create a Python 3.12 virtual environment,
install the pinned JupyterLab and Jupyter AI into it, install this package into the *same*
environment, then start JupyterLab from that environment with `BIOROUTER_EXECUTABLE` pointing at the
`biorouter` binary you want (or with `biorouter` already on `PATH`).

## Acceptance tests

The suite the integration is verified against:

1. Build and install the persona package into JupyterLab's Python environment.
2. Verify the `biorouter-acp` entry point loads.
3. Restart JupyterLab and confirm `@Biorouter` appears in the mention menu.
4. Create an ACP session and load the `about-biorouter` skill.
5. Call the Developer extension and verify a deterministic marker.
6. Verify Jupyter's notebook MCP tools are visible to Biorouter.
7. Confirm tool calls in both Jupyter AI and the persisted Biorouter session.

### 2026-07-17 — first full run

All acceptance tests passed. Jupyter AI loaded the `biorouter-acp` entry point, initialized a
Biorouter ACP session, displayed `@Biorouter`, and received an agent reply. A second ACP turn loaded
the `about-biorouter` skill, ran `developer/shell` with the deterministic
`BIOROUTER_JUPYTER_ACP_EXTENSION_OK` marker, and read `Untitled1.ipynb` through
`jupytermcpserver/read_notebook`. The completed tool graph and successful tool responses were
verified in Biorouter session `20260717_27`.

### 2026-07-21 — re-verification after the rename

Re-verified after the folder was renamed to `integrations/jupyter-ai`, the avatar was swapped to the
current Biorouter mark, and the docs were refreshed. Testing ran against the packaged release binary
(`BIOROUTER_EXECUTABLE=target/release/biorouter`) using the default provider — UCSF Versa
`gpt-5.5-2026-04-24` (`versa_azure`) — in a Python 3.12 venv with the pinned `jupyterlab==4.5.9` /
`jupyter-ai==3.0.1` stack, driving JupyterLab over CDP. Four capability suites passed against
Biorouter session `20260722_2`:

- **Extension execution.** A Code Execution tool call ran a shell command and the reply contained the
  exact single-use marker it emitted, proving real execution rather than a hallucinated result.
- **Knowledge bases.** The agent enumerated both installed bases (`Soul`, `brainstorm`), correctly
  reported `brainstorm` as active, and summarized its actual contents (curated biomedical/genomics
  notes on GWAS, pleiotropy, spatial transcriptomics, etc.) — i.e. it queried the KB, not just its
  names.
- **Skills.** The agent listed the full real skill catalog via its skills tool, including
  extension-bundled skills (`bioroffice-*`, `spoke-knowledge-graph`, `omop-phenotype-query`, the
  `tcr-bcr-analysis-*` family, `update-soul`, …) it could not have produced without invoking the
  tool.
- **Concurrent messages.** Three `@Biorouter` prompts submitted in rapid succession were queued and
  answered serially in order, each returning its exact requested token with no dropped, interleaved,
  or errored turn.

No integration bugs were found; the persona is robust across these usage patterns.

## Repository checks

Run these from the repository root when changing the integration or the ACP surface beneath it.

```bash
# the package's own tests (from the JupyterLab environment you installed it into)
python -m unittest discover -s integrations/jupyter-ai/tests

# the entry point is discoverable
python -c "from importlib.metadata import entry_points; \
  print([e.value for e in entry_points(group='jupyter_ai.personas') if e.name=='biorouter-acp'])"

# the ACP surface the persona drives
cargo fmt --all -- --check
cargo build -p biorouter-cli --bin biorouter
cargo test -p biorouter-acp
```

At the 2026-07-21 run, `cargo fmt --all -- --check` and `cargo build -p biorouter-cli --bin
biorouter` passed, and `cargo test -p biorouter-acp` ran 26 passing tests plus one unrelated
current-`HEAD` fixture failure: `test_acp_with_builtin_and_mcp` still expected the older
`lookup/get_code: Get the code` discovery text while the tool-search response had moved on to a
complete import and signature. No Rust source was changed by this integration.

Re-measured on 2026-07-26: the package's 5 unit tests pass, the entry point resolves to
`jupyter_ai_biorouter.persona:BioRouterAcpPersona`, `cargo fmt --all -- --check` is clean, and
`cargo test -p biorouter-acp` is now fully green at 28 passing tests — the stale fixture above has
since been fixed on `main`.

## Related documentation

- [`integrations/jupyter-ai/README.md`](../../integrations/jupyter-ai/README.md) — install, run, verify and troubleshoot the package itself
- [biorouter CLI command reference](../cli/command-reference.md) — the `biorouter acp` subcommand this persona launches
- [Extensions, skills, and MCP agents](../extensions/extensions-and-skills-guide.md) — what the agent brings with it into the notebook
- [Permission modes](../security/permission-modes.md) — how tool approval behaves when the prompts surface in a Jupyter AI chat
