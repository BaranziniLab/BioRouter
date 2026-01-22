---
title: Chat Recall Extension
description: Search conversation history and load session summaries across all your biorouter sessions
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import { PlatformExtensionNote } from '@site/src/components/PlatformExtensionNote';
import GooseBuiltinInstaller from '@site/src/components/BioRouterBuiltinInstaller';

The Chat Recall extension helps biorouter remember past conversations by searching across your session history. When you need context from previous work, biorouter can search for relevant conversations or load summaries of specific sessions.

biorouter automatically uses Chat Recall when you reference past work or ask questions that require historical context.

## Configuration

<PlatformExtensionNote defaultEnabled={false} />

<Tabs groupId="interface">
  <TabItem value="ui" label="biorouter Desktop" default>
  <GooseBuiltinInstaller
    extensionName="Chatrecall"
    description="Search conversation history across all your sessions"
  />
  </TabItem>
  <TabItem value="cli" label="biorouter CLI">

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
  </TabItem>
</Tabs>

## Example Usage

Chat Recall can search across all sessions using keywords. Results are grouped by session and ordered by recency, and can be filtered by date. Alternatively, you can provide [session IDs](/docs/guides/biorouter-cli-commands#session-list-options) to load summaries of specific sessions.

Let's ask biorouter to find recent conversations about a topic:

### biorouter Prompt

```
What did we discuss about database migrations last week?
```

### biorouter Output

:::note Desktop
I found discussions about database migrations in your recent sessions. Here's what we covered:

**Database Migration Discussions - Last Week**

#### Session: Fix migration scripts (Nov 26, 2024)
**Working Dir:** /projects/myapp

We debugged a production database migration that was failing with a "column already exists" error. The key points were:

- The migration was trying to add a column that already existed in some environments
- We added conditional logic to check if the column exists before adding it
- Tested the fix locally and committed the changes

#### Session: Database schema updates (Nov 27, 2024)
**Working Dir:** /projects/myapp

We discussed adding email verification functionality:

- Planned to add a new `email_verified` column to the users table
- Discussed whether to make it nullable or default to false
- Created the migration script for the new column

Both sessions were focused on safely updating the database schema in production. Would you like me to load the full details from either of these sessions?
:::

:::info
Compacted sessions from biorouter versions prior to v1.14.0 may not be searchable due to a change in how conversation history is stored.
:::
