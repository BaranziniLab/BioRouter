# Scheduled jobs

> **What this is.** A guide to Biorouter's built-in cron scheduler: how to register a workflow to run unattended, the cron syntax it accepts, and what a workflow must contain to survive running headless.
> **Status:** Current.
> **Audience:** end users running recurring analysis or reporting jobs.

Biorouter includes a built-in scheduler that lets you run workflows on a schedule — automatically, without manual intervention. This is useful for recurring analysis jobs, nightly data processing, periodic report generation, or any task you want to run at regular intervals. Everything the scheduler runs is a workflow file, so read [the workflows index](README.md) first if you have not written one yet.

## What the scheduler does

The scheduler lets you:

- Schedule any workflow to run at a specified time or on a recurring cron schedule
- Run Biorouter in headless (non-interactive) mode as a background job
- Persist scheduled jobs across sessions
- Manage (list, pause, delete) scheduled jobs through the Desktop UI or CLI

Scheduled jobs are stored persistently in an SQLite database, so they survive application restarts.

## Creating a scheduled job

### From the Desktop UI

1. Open the sidebar.
2. Navigate to the **Schedule** section.
3. Click **New Schedule**.
4. Select the workflow you want to run.
5. Set the schedule using a cron expression or a simple time picker.
6. Optionally fill in workflow parameters.
7. Save the schedule.

### From the CLI

Use the `schedule` command or ask Biorouter directly in a session:

```sh
# Start a session and instruct Biorouter to schedule a workflow
biorouter session

# In the session:
> Schedule the "nightly-analysis" workflow to run every day at 2am
```

Biorouter will create the scheduled job and confirm the cron expression.

> **Note.** To register a job non-interactively, use `biorouter schedule add`. Its flags and a worked example are in [Creating and sharing workflows](creating-and-sharing-workflows.md#schedule-a-workflow) and the [`schedule` command reference](../cli/command-reference.md#schedule).

## Cron expression format

Scheduled jobs use standard cron syntax:

```text
┌─────── minute (0–59)
│ ┌───── hour (0–23)
│ │ ┌─── day of month (1–31)
│ │ │ ┌─ month (1–12)
│ │ │ │ ┌ day of week (0–7, 0 and 7 = Sunday)
│ │ │ │ │
* * * * *
```

Common examples:

| Expression | Meaning |
|---|---|
| `0 2 * * *` | Every day at 2:00 AM |
| `0 9 * * 1` | Every Monday at 9:00 AM |
| `0 */6 * * *` | Every 6 hours |
| `*/30 * * * *` | Every 30 minutes |
| `0 0 1 * *` | First day of every month at midnight |

## Managing scheduled jobs

### From the Desktop UI

The Schedule panel shows all active and paused jobs with:
- Workflow name and description
- Next run time
- Last run status
- Controls to pause, resume, edit, or delete

### From the CLI

```sh
# List all scheduled jobs
biorouter schedule list

# Delete a scheduled job by ID
biorouter schedule delete <job-id>
```

## Headless (non-interactive) mode

Scheduled jobs always run in headless mode — Biorouter executes the workflow without any user interaction. For this to work correctly, your workflow must:

1. Include a `prompt` field (not just `instructions`) — the `prompt` is the initial message sent automatically.
2. Not depend on user input during execution (no `user_prompt` parameters that need interactive input at runtime).
3. Pre-fill all required parameters, either in the workflow's `parameters` defaults or in the schedule configuration.

**Example workflow suitable for scheduling:**
```yaml
version: "1.0.0"
title: "Nightly Gene Expression Report"
description: "Runs a differential expression analysis and saves results"
instructions: "You are a bioinformatics assistant. Generate a concise summary report."
prompt: "Run the differential expression pipeline on today's data and save the results to /reports/{{ today }}/summary.md"
settings:
  biorouter_provider: "anthropic"
  biorouter_model: "claude-sonnet-4-20250514"
```

## Environment and credentials

Scheduled jobs run in the same environment as the Biorouter server process. Make sure:

- API keys for the required LLM provider are available in the environment or keyring.
- Any extensions the workflow uses are installed and configured.
- File paths referenced in the workflow are accessible at the time the job runs.

## Output and logging

- Scheduled job output is logged and visible in the session history.
- You can review past runs in the Desktop's session list or via `biorouter sessions`.
- Errors during a scheduled run are captured and stored — the job will attempt to run again at the next scheduled time unless deleted.

## Example workflows worth scheduling

Each block below is a **workflow file**, not a schedule. Save it, then register it with the scheduler using the Desktop or CLI steps above; the cron expression lives on the schedule, not in the file. Each one includes a `prompt`, as headless mode requires.

**Recurring research workflows:**
```yaml
# daily-literature-scan.yaml
title: "Daily Literature Scan"
description: "Fetches and summarizes new PubMed papers on a topic"
prompt: "Search PubMed for papers published today about CRISPR gene therapy. Summarize the top 5 results."
extensions:
  - type: stdio
    name: fetch
    cmd: uvx
    args: [mcp-server-fetch]
    timeout: 60
```

**Automated data reports** — no extensions or settings, so it runs against your configured defaults:
```yaml
# weekly-cohort-stats.yaml
title: "Weekly Cohort Statistics"
description: "Computes summary statistics for the study cohort"
prompt: "Generate a weekly statistics report from the cohort database and save to /reports/week-{{ week_number }}.md"
```

**Pipeline monitoring** — adds a `retry` block so a failed run is re-attempted before the next scheduled slot:
```yaml
# pipeline-health-check.yaml
title: "Pipeline Health Check"
description: "Checks status of all running analysis pipelines"
prompt: "Check the status of all jobs in the analysis queue and summarize any failures or delays."
retry:
  max_retries: 2
  checks:
    - type: shell
      command: "test -f /tmp/health-check-complete"
```

## Related documentation

- [Workflows](README.md) — the file format everything the scheduler runs is written in.
- [Creating and sharing workflows](creating-and-sharing-workflows.md#schedule-a-workflow) — the Desktop Scheduler view and the `biorouter schedule add` CLI path in detail.
- [Workflow schema reference](workflow-schema-reference.md#retry) — the full `retry` and `response` schemas the examples above use.
- [biorouter CLI command reference](../cli/command-reference.md#schedule) — every `schedule` subcommand and flag.
- [Headless Linux deployment](../deployment/headless-linux.md) — running Biorouter on a server where scheduled jobs have no GUI to fall back on.
