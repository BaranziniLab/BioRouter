# Scheduling Automations: run workflows on a cron schedule, unattended

## Purpose

Teach the user to put a workflow on a schedule, understand cron syntax, test
headless runs before scheduling, and monitor the results.

## Prerequisite

A working workflow **with a `prompt` field**. Scheduled runs are headless
(no human in the loop), so the workflow must open with its own message. If
the user doesn't have one yet, do the create-workflows tutorial first or
draft a minimal one together.

## Phase 1: Dry-run headless

Before scheduling, prove the workflow works unattended:

```bash
biorouter run --workflow my.yaml --param key=value
```

Watch the output with the user. If the run stalls waiting for input, the
instructions need tightening ("do not ask questions; make reasonable
assumptions and state them").

## Phase 2: Understand cron

Standard 5-field cron: `minute hour day-of-month month day-of-week`.

| Expression | Meaning |
|---|---|
| `0 2 * * *` | every day at 02:00 |
| `0 9 * * 1` | Mondays at 09:00 |
| `0 */6 * * *` | every 6 hours |
| `*/30 * * * *` | every 30 minutes |

`biorouter schedule cron-help` prints a reference in the terminal.

## Phase 3: Create the schedule

- **Desktop:** **Scheduler** page in the sidebar → New Schedule → pick the
  workflow, set the cron expression, fill any required parameters.
- **In chat:** ask the agent — the platform schedule tool can `create` a job
  from a workflow file, plus `run_now`, `pause`, `unpause`, `delete`,
  `inspect`, and list past `sessions`.
- **CLI:** `biorouter schedule list` shows jobs; jobs persist in
  `~/.config/biorouter/schedule.json`.

Use `run_now` immediately after creating the job to verify it executes the
same way the dry-run did.

## Phase 4: Monitor and maintain

- The Scheduler page shows each job's next run, last run, and whether it's
  currently running; jobs can be paused, resumed, edited, or deleted there.
- Each scheduled run produces a normal session — open it from the job's
  history (or History in the sidebar) to read exactly what the agent did.
- A misbehaving job: pause it, fix the workflow, dry-run again, unpause.

## Good first automations to suggest

- A weekly literature scan that searches for new papers on a topic and files
  summaries into a knowledge base.
- A daily data refresh: pull a dataset, recompute a report, save the output.
- A morning digest summarizing changes in a project directory or feed.

## Notes for the agent

- Always dry-run headless before scheduling; the most common failure is a
  workflow that asks a question nobody is there to answer.
- Mind cost: an hourly LLM job adds up. Suggest the least frequent cadence
  that satisfies the need.
- The machine must be on for jobs to fire; schedules don't run while the
  computer sleeps.
