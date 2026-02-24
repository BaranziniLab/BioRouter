# UCSF BioRouter — Schedulers

BioRouter includes a built-in scheduler that lets you run recipes on a schedule — automatically, without manual intervention. This is useful for recurring analysis jobs, nightly data processing, periodic report generation, or any task you want to run at regular intervals.

---

## Overview

The scheduler lets you:

- Schedule any recipe to run at a specified time or on a recurring cron schedule
- Run BioRouter in headless (non-interactive) mode as a background job
- Persist scheduled jobs across sessions
- Manage (list, pause, delete) scheduled jobs through the Desktop UI or CLI

Scheduled jobs are stored persistently in an SQLite database, so they survive application restarts.

---

## Creating a Scheduled Job

### Desktop UI

1. Open the sidebar.
2. Navigate to the **Schedule** section.
3. Click **New Schedule**.
4. Select the recipe you want to run.
5. Set the schedule using a cron expression or a simple time picker.
6. Optionally fill in recipe parameters.
7. Save the schedule.

### CLI

Use the `schedule` command or ask BioRouter directly in a session:

```sh
# Start a session and instruct BioRouter to schedule a recipe
biorouter session

# In the session:
> Schedule the "nightly-analysis" recipe to run every day at 2am
```

BioRouter will create the scheduled job and confirm the cron expression.

---

## Cron Expression Format

Scheduled jobs use standard cron syntax:

```
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

---

## Managing Scheduled Jobs

### Desktop UI

The Schedule panel shows all active and paused jobs with:
- Recipe name and description
- Next run time
- Last run status
- Controls to pause, resume, edit, or delete

### CLI

```sh
# List all scheduled jobs
biorouter schedule list

# Delete a scheduled job by ID
biorouter schedule delete <job-id>
```

---

## Headless (Non-Interactive) Mode

Scheduled jobs always run in headless mode — BioRouter executes the recipe without any user interaction. For this to work correctly, your recipe must:

1. Include a `prompt` field (not just `instructions`) — the `prompt` is the initial message sent automatically.
2. Not depend on user input during execution (no `user_prompt` parameters that need interactive input at runtime).
3. Pre-fill all required parameters, either in the recipe's `parameters` defaults or in the schedule configuration.

**Example recipe suitable for scheduling:**
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

---

## Environment and Credentials

Scheduled jobs run in the same environment as the BioRouter server process. Make sure:

- API keys for the required LLM provider are available in the environment or keyring.
- Any extensions the recipe uses are installed and configured.
- File paths referenced in the recipe are accessible at the time the job runs.

---

## Output and Logging

- Scheduled job output is logged and visible in the session history.
- You can review past runs in the Desktop's session list or via `biorouter sessions`.
- Errors during a scheduled run are captured and stored — the job will attempt to run again at the next scheduled time unless deleted.

---

## Use Cases

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

**Automated data reports:**
```yaml
# weekly-cohort-stats.yaml
title: "Weekly Cohort Statistics"
description: "Computes summary statistics for the study cohort"
prompt: "Generate a weekly statistics report from the cohort database and save to /reports/week-{{ week_number }}.md"
```

**Pipeline monitoring:**
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
