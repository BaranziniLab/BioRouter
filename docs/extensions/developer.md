The Developer extension allows biorouter to automate developer-centric tasks such as file editing, shell command execution, and project setup. It also provides tools for [enhanced code editing](/docs/guides/enhanced-code-editing) and [codebase analysis](/docs/guides/codebase-analysis).

This tutorial will cover enabling and using the Developer MCP Server, which is a built-in biorouter extension. 

## Configuration

> **Info:** The Developer extension is already enabled by default when biorouter is installed.

  
  
  
  

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
  │  ● developer
  // highlight-end  
  └  Extension settings updated successfully
  ```
  

## Example Usage

In this example, I'm going to have biorouter automate setting up my Python bioinformatics analysis environment with pandas, scikit-learn, scanpy, and python-dotenv, and initialize Git.

> **Info:** Anthropic's Claude 4 Sonnet was used for this task.

  
   1. Open a new session in biorouter Desktop
  
  

  1. Open a terminal and start a new biorouter session:

  ```sh
  biorouter session
  ```

  

### biorouter Prompt
```
set up a new Python bioinformatics project with pandas, scikit-learn, scanpy, and python-dotenv? Fetch the latest package versions, generate a README with those versions, and initialize Git
```

After 1 minute, biorouter was done setting up my entire analysis environment! 🎉

### biorouter Output

```md
I'll help you set up a new Python bioinformatics project with those dependencies. I'll break this down into steps:

1. First, let's create a new directory and initialize the project
2. Then install the dependencies
3. Create a detailed README
4. Initialize git
5. Create some starter files
6. Let's begin:

[... shell commands, files created ...]

Great! I've set up a complete Python analysis project with pandas, scikit-learn, and scanpy integrations:

1. Created a new project directory and initialized a virtual environment
2. Installed pandas, scikit-learn, scanpy, and python-dotenv dependencies
3. set up the basic project structure with directories for:
    - config/
    - data/
    - notebooks/ 
    - pipelines/
4. Created essential files:
    - run_analysis.py (main analysis entry point)
    - .env (environment variables, e.g. data paths)
    - .gitignore (with common Python ignores)
5. Added a basic cohort loader with sample_id and diagnosis fields
6. Created an analysis module with two functions:
    - load_cohort (read and validate the cohort table)
    - run_qc (apply quality-control filters)
7. Added pipeline scripts under `pipelines/`
8. Updated requirements.txt with pinned versions
9. Created a comprehensive README.md with:
    - Installation steps
    - Project structure
    - Available scripts
    - Environment variables 
    - Latest package versions

The project structure is:

cohort-analysis/
├── config/         # Configuration files
├── data/           # Input datasets (gitignored)
├── notebooks/      # Exploratory analysis notebooks
│   └── eda.ipynb
├── pipelines/      # Analysis pipelines
│   └── qc_pipeline.py
├── .env            # Environment variables
├── .gitignore      # Git ignore file
├── run_analysis.py # Entry point
└── requirements.txt # Project dependencies

To start developing you can:

1. Run `pip install -r requirements.txt` to install dependencies
2. run the analysis with `python run_analysis.py` 

For example, to load a cohort and run quality control:

python run_analysis.py --cohort data/cohort.csv --step qc

Would you like me to add any additional features or make other improvements to the setup?
```

## Configuring Access Controls

By default, biorouter can run system commands with your user privileges and edit any accessible file **without your approval**. This is because biorouter runs in Autonomous permission mode by default and has access to the Developer extension's shell and file editing tools. While this configuration allows biorouter to work quickly and independently, there's potential for unexpected outcomes. Understanding the available access control features can help you configure biorouter to match your comfort level and specific needs.

> **Tip:** See the [Quick Setup Example](#quick-setup-example) below for some ways to configure more control over biorouter's behavior.

### Developer Extension Tools

The Developer extension provides these tools:

| Tool | Description | Use Cases | Risk Level |
|------|-------------|-----------|------------|
| `shell` | Execute shell commands | Running tests, installing packages, git operations | ⚠️ High<br />Can run any system command with your user privileges |
| `text_editor` | Read, write, and edit files | Code refactoring, creating files, updating configs | ⚠️ High<br />Can modify any accessible file |
| `analyze` | Analyze code structure | Understanding codebase, finding dependencies | ✅ Low<br />Read-only code analysis |
| `screen_capture` | Take screenshots | Debugging UI issues, documenting state | ✅ Low<br />Visual information only |
| `image_processor` | Process and resize images | Optimizing assets, format conversion | ✅ Low<br />Image manipulation only |

### Access Control Features

You can layer multiple controls to match your risk tolerance and workflow:

- **[biorouter Permission Modes](/docs/guides/biorouter-permissions)** control when biorouter asks for approval:

  | Mode | Description | Use Cases |
  |------|-------------|-----------|
  | Autonomous<br />CLI: `auto` | No approval required | Best for experienced users in safe environments |
  | Manual Approval<br />CLI: `approve` | Review every action | Recommended for sensitive work or when you want maximum control |
  | Smart Approval<br />CLI: `smart_approve` | AI decides what needs review | Balanced approach |
  | Chat Only<br />CLI: `chat` | Disable all tools | For maximum security and models that don't support tool-calling |

- **[Tool Permissions](/docs/guides/managing-tools/tool-permissions)** let you set `Always allow`, `Ask before`, and `Never allow` permissions for individual extension tools when in Manual Approval or Smart Approval modes

- **[.biorouterignore files](/docs/guides/using-biorouterignore)** restrict which files and directories biorouter can access (`.gitignore` files are fallback)

> **Tip:** You can change biorouter permission modes during a session without restarting:
- **CLI**: Use the `/mode` command (e.g. `/mode approve`)
- **Desktop**: Use the  mode selector button in the bottom menu

#### Quick Setup Example

You might want more control over biorouter's operations when working with sensitive systems, exploring unfamiliar codebases, using untrusted models, or simply preferring to review actions before execution.

Here's an example configuration that enables oversight:

1. **Set the [permission mode](/docs/guides/biorouter-permissions)** to Smart Approval or Manual Approval:
   ```yaml
   # ~/.config/biorouter/config.yaml
   BIOROUTER_MODE: smart_approve  # or approve
   ```

2. **Create a [`.biorouterignore` file](/docs/guides/using-biorouterignore)** in your project to protect sensitive files:
   ```
   .env*
   secrets.*
   *.key
   *.pem
   .git/
   ```

3. **Configure [tool permissions](/docs/guides/managing-tools/tool-permissions)** based on your needs

As you become more comfortable with biorouter's behavior, you can adjust these settings to reduce friction while maintaining appropriate safeguards for your environment.

> **Info:** Also see the [Security Guide](/docs/guides/security/) for information about using biorouter safely.
