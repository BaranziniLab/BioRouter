The Skills extension loads *skills* &mdash; reusable sets of instructions that teach biorouter how to perform specific tasks or follow particular workflows.

biorouter automatically discovers skills at startup and uses them when relevant to your request. biorouter loads skills from `.agents/skills/` in your project directory and `~/.config/agents/skills/` globally, making skills portable across different AI coding agents. To learn about creating skills and how biorouter uses them, see [Using Skills](/docs/guides/context-engineering/using-skills).

## Configuration

<PlatformExtensionNote/>

<Tabs groupId="interface">
  <TabItem value="ui" label="biorouter Desktop" default>
  <GooseBuiltinInstaller
    extensionName="Skills"
    description="Load and use skills from the .agents/skills directory"
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
  │  ● skills
  // highlight-end  
  └  Extension settings updated successfully
  ```
  </TabItem>
</Tabs>

## Example Usage

Let's say you have a skill that biorouter discovers on startup in `~/.config/agents/skills/deploy/SKILL.md`:

```markdown
---
name: production-deploy
description: Safe deployment procedure for production environment
---

# Production Deployment

## Pre-deployment Checklist
1. Ensure all tests pass
2. Check staging environment is stable
3. Notify #deployments channel

## Deployment Steps
1. Create release branch from main
2. Run `npm run build:prod`
3. Deploy to staging first, then production
4. Monitor error rates for 30 minutes
```

### biorouter Prompt

```
Deploy the current branch to production
```

### biorouter Output

:::note Desktop
I'll help you deploy the current branch to production. Let me first load the production deployment skill to follow the safe deployment procedure.

I can see you're on the **`my-feature-branch`** branch with some untracked files. Before proceeding with production deployment, I need to follow the safe deployment procedure.

#### ⚠️ Pre-deployment Confirmation Required

According to the production deployment skill, I need to complete the following checklist:

**Pre-deployment Checklist**
1. **Ensure all tests pass** - Have tests been run?
2. **Check staging environment is stable** - Has this been deployed to staging first?
3. **Notify #deployments channel** - Should I notify the team?

**Deployment Steps (once confirmed)**
1. Create release branch from main
2. Run `npm run build:prod`
3. Deploy to staging first, then production
4. Monitor error rates for 30 minutes

---

**Before I proceed, please confirm:**

1. ✅ Have all tests passed for the `my-feature-branch` branch?
2. ✅ Has this been tested on staging?
3. ✅ Should I notify the #deployments channel?
4. ✅ Are you sure you want to deploy the `my-feature-branch` branch (not `main`) to production?

Please confirm these items and I'll proceed with the deployment.
:::