# Tutorial extension

> **What this is.** User guide to the built-in Tutorial extension, which loads interactive, step-by-step walkthroughs of BioRouter features.
> **Status:** Current. The extension is live in `crates/biorouter-mcp/src/tutorial/`, and the seven tutorials listed below are drawn from `crates/biorouter-mcp/src/tutorial/tutorials/`, which remains the authoritative source.
> **Audience:** end users, especially new ones.

The Tutorial extension gives you hands-on guidance for learning BioRouter. Rather than reading a page, you ask BioRouter to walk you through something and it loads a structured tutorial, guides you a step at a time, and answers questions as you go. It is aimed at getting new users comfortable with BioRouter's features through practice.

The extension provides:

- Structured, step-by-step tutorials
- Hands-on practice with BioRouter features
- Immediate feedback and guidance

## Configuration

1. Run the `configure` command:

   ```bash
   biorouter configure
   ```

2. Choose `Toggle Extensions`, then enable `tutorial`:

   ```text
   ┌   biorouter-configure
   │
   ◇  What would you like to configure?
   │  Toggle Extensions
   │
   ◆  Enable extensions: (use "space" to toggle and "enter" to submit)
   │  ● tutorial
   └  Extension settings updated successfully
   ```

## Available tutorials

Seven tutorials ship with BioRouter:

| Tutorial | What it covers |
|----------|----------------|
| `getting-started` | Your first 15 minutes with BioRouter: connecting an LLM provider, learning the desktop app and CLI, and having a productive first conversation. |
| `knowledge-bases` | Building a personal, citable library from papers and documents: creating a knowledge base, ingesting sources, querying it in chat, exploring the graph, and export/import. |
| `create-workflows` | Turning a conversation into a reusable, parameterized automation: authoring, validating and running declarative YAML workflows. |
| `schedule-automations` | Running workflows on a cron schedule, unattended: cron syntax, testing headless runs before scheduling, and monitoring results. |
| `create-skills` | Packaging your preferred methods as reusable instruction sets: creating, testing and managing skills — folders containing a `SKILL.md` the agent loads on demand. |
| `build-mcp-extension` | Building a custom MCP tool server and registering it in BioRouter, using the Python, TypeScript or Kotlin SDK. |
| `first-game` | A hands-on coding introduction, building a simple game together (the default suggestion is a Flappy Bird clone in Python and Pygame). This one is not biomedical — it exists as a low-stakes way to experience the full build-run-debug loop before you point BioRouter at real research code. |

> **Note.** If a tutorial you expect is missing here, check `crates/biorouter-mcp/src/tutorial/tutorials/` in the repository — that directory is the definitive list.

## Starting a tutorial

There are two ways to begin.

**Ask BioRouter directly.** Describe a topic you're interested in and BioRouter suggests a relevant tutorial:

```text
I'd like to learn how to build an extension for biorouter
```

**Request a specific tutorial by name:**

```text
Can you walk me through the first-game tutorial?
```

## During the tutorial

While going through a tutorial:

1. BioRouter guides you step-by-step through the process
2. You receive clear instructions before any actions are taken
3. You can ask questions at any time for clarification
4. You can take breaks and resume later

## Best practices

- **Take your time**: don't rush. Make sure you understand each step before moving on.
- **Ask questions**: if something isn't clear, ask BioRouter to explain further.
- **Experiment**: try variations of what you're learning.
- **Take notes**: document any interesting findings or techniques.

## Example usage

Here's how a session starts when you want to learn about building extensions.

### BioRouter prompt

```text
I'd like to learn how to build an extension for biorouter
```

### BioRouter output

```text
I'll help you learn about building extensions! The "build-mcp-extension" tutorial would be perfect for this. Let me load it and guide you through it.

─── load_tutorial | tutorial ──────────────────────────
name: build-mcp-extension

Before we dive into building your extension, I need to understand a few things to better guide you:

1. Which SDK would you prefer to use? We have three options:
   - Python (beginner-friendly, great for prototyping)
   - TypeScript (great for web-based tools)
   - Kotlin (robust, type-safe, good for complex applications)

2. What type of extension would you like to build? For example:
   - A tool that performs specific actions
   - A resource that provides data to the LLM
   - Or do you have a specific idea in mind?

Let me know your preferences, and I'll help you get started with the appropriate setup and implementation!

For reference, here's what we'll cover:
1. Setting up your development environment
2. Creating the basic MCP server
3. Implementing your specific functionality
4. Testing and debugging

Which SDK would you like to use to get started?
```

## Next steps

After completing tutorials, you can:

- Apply what you learned to your own projects
- Share your experience or suggest new tutorial topics by [opening an issue on the BioRouter repository](https://github.com/BaranziniLab/biorouter/issues)

## Getting further help

If you have questions, run into issues, or just want to brainstorm ideas, [open an issue on GitHub](https://github.com/BaranziniLab/biorouter/issues).

## Related documentation

- [Quickstart](../../getting-started/quickstart.md) — the written path through your first session, which the `getting-started` tutorial covers interactively.
- [Skills extension](skills.md) — the skills the `create-skills` tutorial teaches you to author.
- [Workflows](../../workflows/README.md) — reference material behind the `create-workflows` tutorial.
- [Scheduled jobs](../../workflows/scheduled-jobs.md) — reference material behind the `schedule-automations` tutorial.
- [Extensions and skills guide](../extensions-and-skills-guide.md) — how to enable, disable and configure extensions generally.
