# UCSF BioRouter — Workflows

Workflows are reusable, shareable BioRouter configurations. A workflow packages instructions, prompts, extension requirements, parameters, and model settings into a single file that anyone can load to launch a reproducible, pre-configured BioRouter session.

Common use cases: automated analysis pipelines, code review workflows, data processing routines, multi-step research tasks, scheduled jobs.

---

## Workflow File Format

Workflows are written in YAML (recommended) or JSON.

```
my-workflow.yaml     ← recommended
my-workflow.yml      ← supported by Desktop only
my-workflow.json     ← supported
```

---

## Workflow Locations

BioRouter discovers workflows from:

1. The current working directory
2. Paths listed in the `BIOROUTER_WORKFLOW_PATH` environment variable
3. A configured GitHub repository (`BIOROUTER_WORKFLOW_GITHUB_REPO`) — requires the `gh` CLI to be installed and authenticated

---

## Core Schema

| Field | Required | Description |
|---|---|---|
| `title` | Yes | Short title for the workflow |
| `description` | Yes | What the workflow does |
| `instructions` | Yes* | System-level instructions for the agent (Jinja template supported) |
| `prompt` | Yes* | The initial prompt sent to the agent (required for headless/automated mode) |
| `parameters` | No | Dynamic parameters users fill in at launch |
| `extensions` | No | MCP servers and extensions the workflow needs |
| `settings` | No | Provider/model overrides for this workflow |
| `activities` | No | Clickable prompt buttons shown in Desktop (Desktop only) |
| `sub_workflows` | No | Subworkflows this workflow calls |
| `response` | No | Enforce structured JSON output |
| `retry` | No | Auto-retry logic with success validation |
| `version` | No | Workflow format version (defaults to `"1.0.0"`) |

*At least one of `instructions` or `prompt` is required.

---

## Minimal Example

```yaml
version: "1.0.0"
title: "Summarize Research Paper"
description: "Reads a PDF and produces a structured summary"
instructions: "You are a biomedical research assistant."
prompt: "Please summarize the key findings, methods, and conclusions of the attached paper."
```

---

## Parameters

Parameters make workflows dynamic. Users fill in values at launch time; values are substituted into `instructions`, `prompt`, and `activities` using `{{ parameter_name }}` syntax.

```yaml
parameters:
  - key: gene_name
    input_type: string
    requirement: required
    description: "Gene symbol to analyze (e.g. BRCA1)"

  - key: output_format
    input_type: select
    requirement: required
    description: "Output format"
    options:
      - markdown
      - json
      - csv

  - key: max_results
    input_type: number
    requirement: optional
    default: "10"
    description: "Maximum number of results to return"

  - key: data_file
    input_type: file
    requirement: required
    description: "Path to the input data file (contents will be included)"
```

**Input types:** `string`, `number`, `boolean`, `date`, `file`, `select`

**Requirement values:**
- `required` — must be provided at launch
- `optional` — can be omitted if a `default` is specified
- `user_prompt` — interactively prompts the user if not pre-filled

**Template syntax:**
```yaml
prompt: "Analyze {{ gene_name }} and return {{ max_results }} results in {{ output_format }} format."
```

---

## Extensions in Workflows

Workflows can declare which MCP extensions they need:

```yaml
extensions:
  - type: stdio
    name: github-mcp
    cmd: github-mcp-server
    args: []
    env_keys:
      - GITHUB_PERSONAL_ACCESS_TOKEN
    timeout: 60
    description: "GitHub operations"

  - type: builtin
    name: developer
    bundled: true

  - type: inline_python
    name: data_processor
    code: |
      import pandas as pd
      # ... processing logic
    dependencies:
      - pandas
    timeout: 120
```

If an extension requires a secret (listed in `env_keys`) that is not in the system keyring, BioRouter will prompt the user to enter it at launch and store it securely.

---

## Model/Provider Settings

Override the default provider and model for a specific workflow:

```yaml
settings:
  biorouter_provider: "anthropic"
  biorouter_model: "claude-sonnet-4-20250514"
  temperature: 0.7
```

---

## Activities (Desktop only)

Activities create clickable prompt bubbles in the Desktop UI when a workflow is opened:

```yaml
activities:
  - "message: **Welcome!** Select an analysis to begin."
  - "Run differential expression analysis for {{ gene_name }}"
  - "Generate pathway enrichment report"
  - "Export results as {{ output_format }}"
```

The `message:` prefix creates an info box. All other activities become clickable buttons that send their text as the first user message.

---

## Structured Output

Use `response` to enforce a specific JSON output structure — useful for automation pipelines:

```yaml
response:
  json_schema:
    type: object
    properties:
      summary:
        type: string
        description: "Brief summary of findings"
      genes_identified:
        type: array
        items:
          type: string
        description: "List of gene symbols identified"
      confidence_score:
        type: number
        description: "Confidence score 0-1"
    required:
      - summary
      - genes_identified
```

---

## Retry Logic

Workflows can retry automatically if success criteria are not met:

```yaml
retry:
  max_retries: 3
  timeout_seconds: 30
  checks:
    - type: shell
      command: "test -f /tmp/output.json"
    - type: shell
      command: "python -c \"import json; json.load(open('/tmp/output.json'))\""
  on_failure: "rm -f /tmp/output.json"
```

Retry flow:
1. Workflow runs.
2. All `checks` shell commands are executed.
3. If any check fails and retries remain, `on_failure` runs, message history resets, and the workflow restarts.
4. Stops when all checks pass or `max_retries` is exhausted.

---

## Subworkflows

A workflow can delegate sub-tasks to other workflows:

```yaml
sub_workflows:
  - name: "data_cleaning"
    path: "./subworkflows/clean-data.yaml"
    values:
      input_format: "csv"

  - name: "statistical_analysis"
    path: "./subworkflows/stats.yaml"
    sequential_when_repeated: false   # allow parallel execution
```

Set `sequential_when_repeated: false` to allow multiple instances of a subworkflow to run in parallel.

---

## Template Inheritance

Workflows support Jinja-style template inheritance so you can share common structure across related workflows:

**base.yaml:**
```yaml
version: "1.0.0"
title: "Base Analysis"
instructions: |
  {% block instructions %}
  You are a biomedical data analyst.
  {% endblock %}
prompt: |
  {% block prompt %}
  Analyze the provided data.
  {% endblock %}
```

**specialized.yaml:**
```yaml
{% extends "base.yaml" %}
{% block prompt %}
Perform a survival analysis on the provided clinical dataset.
{% endblock %}
```

---

## Saving and Running Workflows

**Desktop:**
- Open a workflow file via File > Open Workflow, or load from the Workflow Library in the sidebar.
- Saved workflows appear in the Workflow Library panel.

**CLI:**
```sh
biorouter run --workflow my-workflow.yaml
biorouter run --workflow my-workflow.yaml --param gene_name=BRCA1
```

---

## Workflow Validation

BioRouter validates workflows on load. Common errors:

- Missing required `title` or `description`
- Optional parameters without a `default` value
- Template variables (`{{ name }}`) with no matching parameter definition
- Unused parameter definitions
- Invalid JSON schema in the `response` field

Validate a workflow explicitly:
```sh
biorouter workflow validate my-workflow.yaml
```
