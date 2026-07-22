# Biorouter persona for Jupyter AI

## Implementation

The integration uses a thin `BaseAcpPersona` subclass registered through the
`jupyter_ai.personas` entry-point group. It launches `biorouter acp` over stdio
and leaves model, extension, skill, workflow, and permission ownership inside
Biorouter.

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
and uses Jupyter AI's ACP base class for message processing, session recovery,
streaming, tool-call display, permission requests, cancellation, and MCP server
forwarding.

## Compatibility

- Jupyter AI 3.0.1
- JupyterLab 4.5.9
- `jupyter-ai-acp-client` 0.1.5
- `jupyter-ai-persona-manager` 0.0.12

JupyterLab 4.5.9 is intentionally pinned because Jupyter AI 3.0.1's frontend
extensions are incompatible with JupyterLab 4.6's `@jupyter/ydoc` major version.

## Acceptance tests

1. Build and install the persona package into JupyterLab's Python environment.
2. Verify the `biorouter-acp` entry point loads.
3. Restart JupyterLab and confirm `@Biorouter` appears in the mention menu.
4. Create an ACP session and load the `about-biorouter` skill.
5. Call the Developer extension and verify a deterministic marker.
6. Verify Jupyter's notebook MCP tools are visible to Biorouter.
7. Confirm tool calls in both Jupyter AI and the persisted Biorouter session.

All acceptance tests passed on July 17, 2026. Jupyter AI loaded the
`biorouter-acp` entry point, initialized a Biorouter ACP session, displayed
`@Biorouter`, and received an agent reply. A second ACP turn loaded the
`about-biorouter` skill, ran `developer/shell` with the deterministic
`BIOROUTER_JUPYTER_ACP_EXTENSION_OK` marker, and read `Untitled1.ipynb` through
`jupytermcpserver/read_notebook`. The completed tool graph and successful tool
responses were verified in Biorouter session `20260717_27`.

Re-verified on July 21, 2026 after the folder was renamed to
`integrations/jupyter-ai`, the avatar was swapped to the current Biorouter mark,
and the docs were refreshed. Testing ran against the packaged release binary
(`BIOROUTER_EXECUTABLE=target/release/biorouter`) using the default provider —
UCSF Versa `gpt-5.5-2026-04-24` (`versa_azure`) — in a Python 3.12 venv with the
pinned `jupyterlab==4.5.9` / `jupyter-ai==3.0.1` stack, driving JupyterLab over
CDP. Four capability suites passed against Biorouter session `20260722_2`:

- **Extension execution.** A Code Execution tool call ran a shell command and the
  reply contained the exact single-use marker it emitted, proving real execution
  rather than a hallucinated result.
- **Knowledge bases.** The agent enumerated both installed bases (`Soul`,
  `brainstorm`), correctly reported `brainstorm` as active, and summarized its
  actual contents (curated biomedical/genomics notes on GWAS, pleiotropy, spatial
  transcriptomics, etc.) — i.e. it queried the KB, not just its names.
- **Skills.** The agent listed the full real skill catalog via its skills tool,
  including extension-bundled skills (`bioroffice-*`, `spoke-knowledge-graph`,
  `omop-phenotype-query`, the `tcr-bcr-analysis-*` family, `update-soul`, …) it
  could not have produced without invoking the tool.
- **Concurrent messages.** Three `@Biorouter` prompts submitted in rapid
  succession were queued and answered serially in order, each returning its exact
  requested token with no dropped, interleaved, or errored turn.

No integration bugs were found; the persona is robust across these usage
patterns.

## Repository checks

- `cargo fmt --all -- --check`: passed.
- `cargo build -p biorouter-cli --bin biorouter`: passed.
- `cargo test -p biorouter-acp`: 26 tests passed and one unrelated current-HEAD
  fixture failed. `test_acp_with_builtin_and_mcp` still expects the older
  `lookup/get_code: Get the code` discovery text, while the current tool search
  response emits a complete import and signature. No Rust source was changed by
  this integration.
