The Chat Recall extension helps biorouter remember past conversations by searching across your session history. When you need context from previous work, biorouter can search for relevant conversations or load summaries of specific sessions.

biorouter automatically uses Chat Recall when you reference past work or ask questions that require historical context.

## Configuration

  
  
  
  

  1. Run the `configure` command:
  ```sh
  biorouter configure
  ```

  2. Choose to `Toggle Extensions`
  ```sh
  ┌   biorouter-configure 
  │
  ◇  What would you like to configure?
  │  Toggle Extensions 
  │
  ◆  Enable extensions: (use "space" to toggle and "enter" to submit)
  // highlight-start    
  │  ● chatrecall
  // highlight-end  
  └  Extension settings updated successfully
  ```
  

## Example Usage

Chat Recall can search across all sessions using keywords. Results are grouped by session and ordered by recency, and can be filtered by date. Alternatively, you can provide [session IDs](/docs/guides/biorouter-cli-commands#session-list-options) to load summaries of specific sessions.

Let's ask biorouter to find recent conversations about a topic:

### biorouter Prompt

```
What did we discuss about the cohort QC pipeline last week?
```

### biorouter Output

> **Note:** I found discussions about the cohort QC pipeline in your recent sessions. Here's what we covered:

**Cohort QC Pipeline Discussions - Last Week**

#### Session: Fix QC filtering scripts (Nov 26, 2024)
**Working Dir:** /projects/cohort-study

We debugged a quality-control step that was dropping more samples than expected. The key points were:

- The filter was excluding samples with missing values in an optional biomarker column
- We added conditional logic to skip the filter when the column is absent
- Re-ran the pipeline locally and committed the fix

#### Session: Cohort schema updates (Nov 27, 2024)
**Working Dir:** /projects/cohort-study

We discussed adding a derived eligibility flag:

- Planned to add a new `meets_inclusion_criteria` field to the cohort table
- Discussed whether to default it to false until criteria are evaluated
- Created the script to populate the new field

Both sessions were focused on safely refining the cohort dataset. Would you like me to load the full details from either of these sessions?

> **Info:** Compacted sessions from biorouter versions prior to v1.14.0 may not be searchable due to a change in how conversation history is stored.
