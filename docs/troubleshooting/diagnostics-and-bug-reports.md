# Diagnostics and bug reports

> **What this is.** How to produce a biorouter diagnostics bundle from the desktop app or the CLI, what the bundle contains, and how to turn it into a bug report or feature request on GitHub.
> **Status:** Current.
> **Audience:** end users

biorouter provides several built-in features to help you get support, report issues, and request new functionality. This page covers the diagnostics system, bug reporting, and feature request tools. When something is going wrong, start with [Common problems and fixes](common-problems-and-fixes.md); come here once you need to hand a maintainer the details of your setup.

| Feature | Purpose | Location | Output |
|---------|---------|----------|---------|
| **Diagnostics** | Generate troubleshooting data | Chat input toolbar | ZIP file with system info, logs, and session data |
| **Report a Bug** | Submit bug reports | Settings → Help & feedback | Opens GitHub issue template |
| **Request a Feature** | Suggest new features | Settings → Help & feedback | Opens GitHub issue template |

## Diagnostics bundle

The diagnostics feature creates a comprehensive troubleshooting bundle that includes system information, session data, configuration files, and recent logs. This is invaluable for debugging issues or getting technical support.

Generate one when you are:

- Experiencing crashes or unexpected behavior.
- Getting error messages you don't understand.
- Hitting performance issues or slow responses.
- About to report a bug and want to include technical details.

### Generating a bundle from the desktop app

1. In an active chat session, look for the diagnostics icon in the bottom toolbar.
2. Click the diagnostics button.
3. Review the information in the modal about what data will be collected.
4. Click `Download` to generate and save the diagnostics bundle.
5. The ZIP file will be saved as `diagnostics_{session_id}.zip`.

> **Note.** The diagnostics button is only available when you have an active session, as it needs a session ID to generate the bundle.

### Generating a bundle from the CLI

Use the session diagnostics command to generate a troubleshooting bundle. For complete details and all available options, see the [CLI command reference](../cli/command-reference.md#session-diagnostics-options).

```sh
# Generate diagnostics for a specific session
biorouter session diagnostics --session-id <session_id>

# Interactive selection (prompts you to choose a session)
biorouter session diagnostics

# Save to a custom location
biorouter session diagnostics --session-id <session_id> --output /path/to/diagnostics.zip
```

To find your session ID, first list available sessions:

```sh
biorouter session list
```

Example output:

```text
Available sessions:
abc123def - My coding session - 2024-01-15 14:30:22
xyz789ghi - Documentation work - 2024-01-15 10:15:45
```

### What the bundle contains

The diagnostics ZIP file contains several folders:

```text
diagnostics_abc123def.zip
├── logs/
│   ├── biorouter-2024-01-15.jsonl
│   ├── biorouter-2024-01-14.jsonl
│   └── ...
├── session.json          # Your session messages
├── config.yaml          # Configuration files (if they exist)
└── system.txt           # System information
```

Broken out by kind:

- **System information**: app version, operating system, architecture, and timestamp.
- **Session data**: your current conversation messages and history.
- **Configuration files**: your [configuration files](../configuration/config-file-reference.md) (if they exist).
- **Log files**: recent application logs for debugging.

> **Warning.** Diagnostics bundles contain your session messages and system information. If your session includes sensitive data (API keys, personal information, proprietary code), review the contents before sharing publicly.

## Reporting bugs and requesting features

Both flows open a structured GitHub issue template, so your report arrives with the information maintainers need. The desktop steps are the same for each; only the final button differs.

From the desktop app:

1. Open the sidebar using the button in the top-left.
2. Click `Settings` in the sidebar.
3. Scroll down to the `Help & feedback` section.
4. Click `Report a Bug` to file a bug, or `Request a Feature` to suggest new functionality.
5. This opens GitHub in your browser with the matching pre-filled template.

From the CLI, navigate directly to the GitHub repository:

| Report type | URL |
|---|---|
| Bug report | `https://github.com/BaranziniLab/biorouter/issues/new?template=bug_report.md` |
| Feature request | `https://github.com/BaranziniLab/biorouter/issues/new?template=feature_request.md` |

## Error recovery with "Ask biorouter"

When certain types of error occur in biorouter Desktop (such as failures to activate extensions), you'll see an `Ask biorouter` button in the error notification. This feature lets you quickly troubleshoot the issue with biorouter's help:

1. When the error occurs, an `Ask biorouter` button appears in the error notification.
2. Click the button to send the error details to biorouter in a chat prompt.
3. biorouter provides diagnostic suggestions and potential solutions.

## Further debugging

For issues not resolved by diagnostics:

- **Session and system logs**: view detailed logs for debugging individual sessions. The bundle above collects the recent ones under `logs/`.
- **[Telemetry export](../configuration/environment-variables.md#observability)**: configure telemetry for performance analysis and production monitoring.

## Related documentation

- [Common problems and fixes](common-problems-and-fixes.md) — the symptom-by-symptom reference to check before filing an issue; several of its entries end by asking for the bundle described here.
- [Troubleshooting index](README.md) — the entry point for this folder and the recommended order of steps.
- [CLI command reference](../cli/command-reference.md#session-diagnostics-options) — every flag on `biorouter session diagnostics`, plus the rest of the session subcommands.
- [Configuration file reference](../configuration/config-file-reference.md) — what lives in the config files the bundle collects.
- [Environment variables](../configuration/environment-variables.md#observability) — the observability settings behind telemetry export.
