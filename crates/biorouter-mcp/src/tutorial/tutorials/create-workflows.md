# Creating Workflows: turn a conversation into a reusable, parameterized automation

## Purpose

Teach the user to author, validate, and run Biorouter workflows — declarative
YAML files that capture instructions, parameters, extensions, and settings so
a task can be re-run consistently, shared, or scheduled.

## Concepts to convey first (briefly)

A workflow YAML has:

- **Required:** `version`, `title`, `description`, and at least one of
  `instructions` (becomes the session's operating instructions) or `prompt`
  (the initial message — required for headless and scheduled runs).
- **Optional:** `parameters` (typed inputs: string/number/boolean/date/
  file/select, each `required`, `optional`, or `user_prompt`), `extensions`
  (MCP servers this workflow needs), `skills`, `knowledge_bases`, `settings`
  (per-workflow `biorouter_provider` / `biorouter_model` / `temperature`
  overrides), `activities` (clickable starter buttons in the desktop UI),
  `response` (a JSON schema forcing structured final output), `sub_workflows`,
  and `retry`.
- Parameters substitute into `instructions`/`prompt`/`activities` with Jinja
  syntax: `{{ parameter_name }}`.

## Phase 1: Start from something real

The easiest workflow is extracted from a conversation that already worked:

- **Desktop:** from a chat, use "make workflow from this session" — Biorouter
  drafts the title, description, instructions, and activities for you.
- **From scratch:** ask the user what task they repeat, then draft the YAML
  together. Keep instructions generic ("summarize the given paper") and push
  specifics into parameters.

Example to adapt:

```yaml
version: "1.0.0"
title: Paper summarizer
description: Summarize a paper and file it into a knowledge base
parameters:
  - key: paper_url
    input_type: string
    requirement: required
    description: URL or path of the paper to summarize
instructions: |
  Fetch {{ paper_url }}, produce a structured summary (objective, methods,
  findings, limitations), and add the source to the active knowledge base.
prompt: Summarize {{ paper_url }} now.
activities:
  - Summarize a new paper
settings:
  temperature: 0.2
```

## Phase 2: Validate and run

- Validate: `biorouter workflow validate <file>` catches missing fields and
  orphaned parameters.
- Run from the CLI: `biorouter run --workflow my.yaml --param paper_url=...`
- Run from the desktop: the **Workflows** page lists saved workflows; opening
  one prompts for parameters and starts a session with the workflow's
  instructions applied.

Have the user run their new workflow once end-to-end and refine the
instructions based on what the agent actually did.

## Phase 3: Going further

- **Structured output:** add a `response` JSON schema when downstream tooling
  consumes the result — the agent is then required to emit schema-valid JSON.
- **Scheduling:** any workflow with a `prompt` can run unattended on a cron
  schedule (offer the schedule-automations tutorial).
- **Composition:** `sub_workflows` lets a workflow delegate phases to other
  workflows; `retry` adds automatic retry logic for flaky steps.
- **Sharing:** workflow files are plain YAML — commit them to a repo or share
  the file; `BIOROUTER_WORKFLOW_GITHUB_REPO` can point a team at a shared
  workflow repository.

## Notes for the agent

- Iterate on `instructions` like code: run, observe, tighten. Vague
  instructions produce vague sessions.
- If a workflow will ever be scheduled, make sure it has a `prompt`, not just
  `instructions` — scheduled runs are headless and need an opening message.
- Declare the extensions the workflow depends on rather than assuming the
  user's defaults; workflows should be self-contained.
