`.biorouterignore` is a text file that defines patterns for files and directories that biorouter will not access. This means biorouter cannot read, modify, delete, or run shell commands on these files when using the Developer extension's tools.

:::info Developer extension only
The .biorouterignore feature currently only affects tools in the [Developer](/docs/mcp/developer-mcp) extension. Other extensions are not restricted by these rules.
:::

This guide will show you how to use `.biorouterignore` files to prevent biorouter from changing specific files and directories.

## Creating your `.biorouterignore` file

biorouter supports two types of `.biorouterignore` files:
- **Global ignore file** - Create a `.biorouterignore` file in `~/.config/biorouter`. These restrictions will apply to all your sessions with biorouter, regardless of directory.
- **Local ignore file** - Create a `.biorouterignore` file at the root of the directory you'd like it applied to. These restrictions will only apply when working in a specific directory.

:::tip
You can use both global and local `.biorouterignore` files simultaneously. When both exist, biorouter will combine the restrictions from both files to determine which paths are restricted.
:::

## Example `.biorouterignore` file

In your `.biorouterignore` file, you can write patterns to match files you want biorouter to ignore. Here are some common patterns:

```plaintext
# Ignore specific files by name
settings.json         # Ignore only the file named "settings.json"

# Ignore files by extension
*.pdf                # Ignore all PDF files
*.config             # Ignore all files ending in .config

# Ignore directories and their contents
backup/              # Ignore everything in the "backup" directory
downloads/           # Ignore everything in the "downloads" directory

# Ignore all files with this name in any directory
**/credentials.json  # Ignore all files named "credentials.json" in any directory

# Complex patterns
*.log                # Ignore all .log files
!error.log           # Except for error.log file
```

## Ignore File Types and Priority
biorouter respects ignore rules from global `.biorouterignore` and local `.biorouterignore` files. It uses a priority system to determine which files should be ignored. 

### 1. Global `.biorouterignore`
- Highest priority and always applied first
- Located at `~/.config/BioRouter/.biorouterignore`
- Affects all projects on your machine

```
~/.config/BioRouter/
└── .biorouterignore      ← Applied to all projects
```

### 2. Local `.biorouterignore`
- Project-specific rules
- Located in your project root directory

```
~/.config/BioRouter/
└── .biorouterignore      ← Global rules applied first

Project/
├── .biorouterignore      ← Local rules applied second
└── src/
```

### 3. Default Patterns
By default, if you haven't created any .biorouterignore files, biorouter will not modify files matching these patterns:
```plaintext
**/.env
**/.env.*
**/secrets.*
```

## Common use cases

Here are some typical scenarios where `.biorouterignore` is helpful:

- **Generated Files**: Prevent biorouter from modifying auto-generated code or build outputs
- **Third-Party Code**: Keep biorouter from changing external libraries or dependencies
- **Important Configurations**: Protect critical configuration files from accidental modifications
- **Version Control**: Prevent changes to version control files like `.git` directory
- **Custom Restrictions**: Create `.biorouterignore` files to define which files biorouter should not access