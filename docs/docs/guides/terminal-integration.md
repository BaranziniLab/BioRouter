# Terminal Integration

Talk to biorouter directly from your shell prompt. Instead of switching to a separate REPL session, stay in your terminal and call biorouter when you need it.

## Setup

<Tabs groupId="shells">
<TabItem value="zsh" label="zsh" default>

Add to `~/.zshrc`:
```bash
eval "$(biorouter term init zsh)"
```

</TabItem>
<TabItem value="bash" label="bash">

Add to `~/.bashrc`:
```bash
eval "$(biorouter term init bash)"
```

</TabItem>
<TabItem value="fish" label="fish">

Add to `~/.config/fish/config.fish`:
```fish
biorouter term init fish | source
```

</TabItem>
<TabItem value="powershell" label="PowerShell">

Add to `$PROFILE`:
```powershell
Invoke-Expression (biorouter term init powershell)
```

</TabItem>
</Tabs>

Restart your terminal or source the config, and that's it!

## Usage

Just type `@biorouter` (or `@g` for short) followed by your question:

```bash
npm install express
    npm ERR! code EACCES
    npm ERR! permission denied

@biorouter "how do I fix this error?"
```

biorouter automatically sees the commands you've run since your last question, so you don't need to explain what you've been doing. Use quotes around your prompt if it contains special characters like `?`, `*`, or `'`:

```bash
@biorouter "what's in this directory?"
@g "analyze the error: 'permission denied'"
```

## Named Sessions
By default, each terminal gets its own biorouter session that lasts until you close it. Named sessions let you continue conversations across terminal restarts and share context between windows.

<Tabs groupId="shells">
<TabItem value="zsh" label="zsh" default>

```bash
eval "$(biorouter term init zsh --name my-project)"
```

</TabItem>
<TabItem value="bash" label="bash">

```bash
eval "$(biorouter term init bash --name my-project)"
```

</TabItem>
<TabItem value="fish" label="fish">

```fish
biorouter term init fish --name my-project | source
```

</TabItem>
<TabItem value="powershell" label="PowerShell">

```powershell
Invoke-Expression (biorouter term init powershell --name my-project)
```

</TabItem>
</Tabs>

Named sessions persist in biorouter's database, so they're available anytime, even after restarting your computer. Reopen later and run the same command to continue:

```bash
# Start debugging
eval "$(biorouter term init zsh --name auth-bug)"
@biorouter help me debug this login timeout

# Close terminal, come back later
eval "$(biorouter term init zsh --name auth-bug)"
@biorouter "what was the solution we discussed?"
# Continues the same conversation with context
```

## Show Context Status in Your Prompt

Add `biorouter term info` to your prompt to see how much context you've used and which model is active during a terminal biorouter session. 

<Tabs groupId="shells">
<TabItem value="zsh" label="zsh" default>

```bash
PROMPT='$(biorouter term info) %~ $ '
```

</TabItem>
<TabItem value="bash" label="bash">

```bash
PS1='$(biorouter term info) \w $ '
```

</TabItem>
<TabItem value="fish" label="fish">

```fish
function fish_prompt
    biorouter term info
    echo -n ' '(prompt_pwd)' $ '
end
```

</TabItem>
<TabItem value="powershell" label="PowerShell">

```powershell
function prompt {
    $gooseInfo = & biorouter term info
    "$gooseInfo $(Get-Location) PS> "
}
```

</TabItem>
</Tabs>

Your terminal prompt now shows the context usage and model name (shortened for readability) for the active biorouter session. For example:

```bash
●●○○○ sonnet ~/projects $
```
## Troubleshooting

**biorouter doesn't see recent commands:**
If you run commands but biorouter says it doesn't see any recent activity, check if terminal integration is properly [set up in your shell config](#setup).
You can also check the id of the biorouter session in your current terminal:
```bash
# Check if session ID exists
echo $BIOROUTER_SESSION_ID
# Should show something like: 20251209_151730
```
To share context across terminal windows, use a [named session](#named-sessions) instead.

**Session getting too full** (prompt shows `●●●●●`):
If biorouter's responses are getting slow or hitting context limits, start a fresh biorouter session in the terminal. The new biorouter session sees your command history, but not the conversation history from the previous session. 
```bash
# Start a new biorouter session in the same shell
eval "$(biorouter term init zsh)"
```