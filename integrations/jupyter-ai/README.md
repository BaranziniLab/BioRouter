# Biorouter for Jupyter AI

This package registers **Biorouter** as an ACP (Agent Client Protocol) persona in
Jupyter AI. When you talk to `@Biorouter` in a Jupyter AI chat, it launches
`biorouter acp` over stdio and runs your fully configured Biorouter agent — the
same model, providers, extensions, skills, workflows, knowledge bases, and
permissions you use in the desktop app or the CLI, including the biomedical
agents (SPOKE, OMOP, CDW, …).

The package is a thin `BaseAcpPersona` subclass registered through the
`jupyter_ai.personas` entry-point group. All model, extension, skill, workflow,
and permission ownership stays inside Biorouter; Jupyter AI handles the chat UI,
streaming, tool-call display, permission prompts, cancellation, and forwarding
its own notebook MCP tools into the session.

## Requirements

- **Python 3.9–3.12** for the JupyterLab environment (Jupyter AI 3.0.1 does not
  yet support 3.13+; if your system Python is newer, make a dedicated venv — see
  below).
- **JupyterLab 4.5.9** and **Jupyter AI 3.0.1**. JupyterLab is pinned to 4.5.9 on
  purpose: Jupyter AI 3.0.1's frontend is incompatible with JupyterLab 4.6's
  `@jupyter/ydoc` major version.
- The **`biorouter` CLI on `PATH`**, or `BIOROUTER_EXECUTABLE` set to its
  executable.
- A **configured Biorouter provider and model** (e.g. `~/.config/biorouter/config.yaml`).

## Install

Create an isolated environment on a supported Python, install JupyterLab +
Jupyter AI, then install this package into the *same* environment:

```bash
# from the biorouter repo root
uv venv ~/biorouter-jupyter/.venv --python 3.12
uv pip install --python ~/biorouter-jupyter/.venv/bin/python \
  "jupyterlab==4.5.9" \
  "jupyter-ai==3.0.1" \
  "jupyter-ai-acp-client>=0.1.5,<0.2.0" \
  "jupyter-ai-persona-manager>=0.0.11,<0.1.0" \
  -e integrations/jupyter-ai
```

(`pip` works too if you prefer: activate the venv and `pip install -e integrations/jupyter-ai`.)

## Run

Point the persona at the Biorouter binary you want (a release install on `PATH`,
or an explicit dev build) and start JupyterLab from the same environment:

```bash
BIOROUTER_EXECUTABLE=/absolute/path/to/biorouter \
  ~/biorouter-jupyter/.venv/bin/jupyter-lab
```

If `biorouter` is already on your `PATH`, you can omit `BIOROUTER_EXECUTABLE`.

Open the Jupyter AI chat panel and type `@` — **Biorouter** appears in the
mention menu. Select it and chat; each turn runs through `biorouter acp` and uses
your configured model (by default the one in `~/.config/biorouter/config.yaml`).

## Verify

Confirm the entry point is discoverable:

```bash
python -c "from importlib.metadata import entry_points; \
  print([e.value for e in entry_points(group='jupyter_ai.personas') if e.name=='biorouter-acp'])"
# -> ['jupyter_ai_biorouter.persona:BioRouterAcpPersona']
```

Run the package's own tests:

```bash
python -m unittest discover -s integrations/jupyter-ai/tests
```

## Troubleshooting

- **`@Biorouter` doesn't appear** — the package must be installed into the *same*
  Python environment as JupyterLab; restart JupyterLab after installing.
- **"requires the `biorouter` CLI on PATH"** — install Biorouter or set
  `BIOROUTER_EXECUTABLE` to its absolute path, then restart JupyterLab.
- **No model replies / provider errors** — confirm Biorouter itself is configured
  (run `biorouter` once, or check `~/.config/biorouter/config.yaml`); the persona
  uses whatever Biorouter is set up to use.
