This guide covers storing, organizing, and finding biorouter workflows when you need to access them again later. 

> **Info:** - **biorouter Desktop** has a visual Workflow Library for browsing and managing saved workflows
- **biorouter CLI** stores workflows as files that you find using file paths or environment variables

## Understanding Workflow Storage

Before saving workflows, it's important to understand where they can be stored and how this affects their availability.

### Workflow Storage Locations

| Type | Location | Availability | Best For |
|------|----------|-------------|----------|
| **Global** | `~/.config/biorouter/workflows/` | All projects and sessions | Personal workflows, general-purpose workflows |
| **Local** | `YOUR_WORKING_DIRECTORY/.biorouter/workflows/` | Only when working in that project | Project-specific workflows, team workflows |

**Choose Global Storage When:**
- You want the workflow available across all projects
- It's a personal workflow or general-purpose workflow
- You're the primary user of the workflow

**Choose Local Storage When:**
- The workflow is specific to a particular project
- You're working with a team and want to share the workflow
- The workflow depends on project-specific files or configurations

## Storing Workflows

  

**Save New Workflow:**

1. To create a workflow from your chat session, see [Create Workflow](/docs/guides/workflows/session-workflows#create-workflow)
2. Once in the Workflow Editor, click `Save Workflow` to save it to your Workflow Library

**Save Modified Workflow:**

If you're already using a workflow and want to save a modified version:
1. Click the  button at the bottom of the app, which appears after sending your first message
2. Make any desired edits to the instructions, prompt, or other fields
3. Click `Save Workflow`

> **Info:** When you modify and save a workflow with a new name, a new workflow and new link are generated. You can still run the original workflow from the workflow library, or using the original link. If you edit a workflow without changing its name, the version in the workflow library is updated, but you can still run the original workflow via link.

  
  

    When you [create a workflow](/docs/guides/workflows/reference), it gets saved to:

    * Your working directory by default: `./workflow.yaml`
    * Any path you specify: `/workflow /path/to/my-workflow.yaml`  
    * Local project workflows: `/workflow .biorouter/workflows/my-workflow.yaml`

    
> **Note:** The CLI saves workflows as `.yaml` files. While the CLI can run workflows in `.json` format, it does not provide an option to save workflows as JSON.

  

### Importing Workflows

  
    Import a workflow using its deeplink or workflow file:

    1. Click the  button in the top-left to open the sidebar
    2. Click `Workflows` in the sidebar
    3. Click `Import Workflow`
    4. Choose your import method:
       - To import via a link: Under `Workflow Deeplink`, paste in the [workflow link](/docs/guides/workflows/session-workflows#share-via-workflow-link)
       - To import via a file: Under `Workflow File`, click `Choose File`, select a workflow file, and click `Open`
    5. Click `Import Workflow` to save a copy of the workflow to your Workflow Library

  
> **Warning:** biorouter Desktop accepts `.yaml`, `.yml`, and `.json` files, but **the CLI only supports `.yaml` and `.json`**. For full compatibility across both interfaces, avoid `.yml` extensions.

  All workflow formats follow the same [schema structure](/docs/guides/workflows/reference#core-workflow-schema).

  
  
    Workflow import is only available in biorouter Desktop.
  

## Finding Available Workflows

  

**Access Workflow Library:**
1. Click the  button in the top-left to open the sidebar
2. Click `Workflows` to view your Workflow Library
3. Browse your available workflows, which show:
   - Workflow title and description
   - Last modified date
   - Whether they're stored globally or locally

> **Info:** The Desktop Workflow Library displays all workflows you've explicitly saved or imported. It doesn't automatically discover workflow files from your filesystem like the CLI does.

  
  

Use the `biorouter workflow list` command to find all available workflows from multiple sources:

**Basic Usage**

```bash
# List all available workflows
biorouter workflow list

# Show detailed information including titles and full paths
biorouter workflow list --verbose

# Output in JSON format for automation
biorouter workflow list --format json
```

**Workflow Discovery Process**

biorouter searches for workflows in the following locations (in order):

1. **Current directory**: `.` (looks for `*.yaml` and `*.json` files)
2. **Custom paths**: Directories specified in [`BIOROUTER_WORKFLOW_PATH`](/docs/guides/environment-variables#workflow-configuration) environment variable
3. **Global workflow library**: `~/.config/biorouter/workflows/` (or equivalent on your OS)
4. **Local project workflows**: `./.biorouter/workflows/`
5. **GitHub repository**: If [`BIOROUTER_WORKFLOW_GITHUB_REPO`](/docs/guides/environment-variables#workflow-configuration) environment variable is configured

**Example Output**

*Default text format:*
```bash
$ biorouter workflow list
Available workflows:
biorouter-self-test - A comprehensive meta-testing workflow - local: ./biorouter-self-test.yaml
hello-world - A sample workflow demonstrating basic usage - local: ~/.config/biorouter/workflows/hello-world.yaml
literature-scan - Summarize new PubMed papers on a topic - local: ~/.config/biorouter/workflows/literature-scan.yaml
```

*Verbose mode:*
```bash
$ biorouter workflow list --verbose
Available workflows:
  biorouter-self-test - A comprehensive meta-testing workflow - local: ./biorouter-self-test.yaml
    Title: biorouter Self-Testing Integration Suite
    Path: ./biorouter-self-test.yaml
  hello-world - A sample workflow demonstrating basic usage - local: ~/.config/biorouter/workflows/hello-world.yaml
    Title: Hello World Workflow
    Path: /Users/username/.config/biorouter/workflows/hello-world.yaml
```

*JSON format for automation:*
```json
[
  {
    "name": "biorouter-self-test",
    "source": "Local",
    "path": "./biorouter-self-test.yaml",
    "title": "biorouter Self-Testing Integration Suite",
    "description": "A comprehensive meta-testing workflow"
  },
  {
    "name": "hello-world",
    "source": "GitHub",
    "path": "workflows/hello-world.yaml",
    "title": "Hello World Workflow",
    "description": "A sample workflow demonstrating basic usage"
  }
]
```

**Configuring Workflow Sources**

Add custom workflow directories:
```bash
biorouter workflow list
```

Configure GitHub workflow repository:
```bash
biorouter workflow list
```

See the [Environment Variables Guide](/docs/guides/environment-variables#workflow-configuration) for more configuration options.

**Manual Directory Browsing (Advanced)**

If you need to browse workflow directories manually:

```bash
# List workflows in default global location
ls ~/.config/biorouter/workflows/

# List workflows in current project
ls .biorouter/workflows/

# Search for all workflow files
find . -name "*.yaml" -path "*/workflows/*" -o -name "*.json" -path "*/workflows/*"
```

> **Tip:** The `biorouter workflow list` command is the recommended way to find workflows as it automatically searches all configured sources and provides consistent formatting.

  

## Using Saved Workflows

  

1. Click the  button in the top-left to open the sidebar
2. Click `Workflows`
3. Find your workflow in the Workflow Library
4. Choose one of the following:
   - Click `Use` to run it immediately
   - Click `Preview` to see the workflow details first, then click **Load Workflow** to run it

  
  

Once you've located your workflow file, [run the workflow](/docs/guides/workflows/session-workflows#run-a-workflow) or [open it in biorouter Desktop](/docs/guides/biorouter-cli-commands#workflow).

> **Tip:** The CLI can run workflows saved from biorouter Desktop without any conversion. Both CLI-created and Desktop-saved workflows work with all workflow commands.
