# biorouter in 5 minutes

biorouter is an extensible open source AI agent that enhances your software development by automating coding tasks. 

This quick tutorial will guide you through:

- ✅ Installing biorouter
- ✅ Configuring your LLM
- ✅ Building a small app
- ✅ Adding an MCP server

Let's begin 🚀

## Install biorouter

<Tabs>
  <TabItem value="mac" label="macOS" default>
    Choose to install the Desktop and/or CLI version of biorouter:

    <Tabs groupId="interface">
      <TabItem value="ui" label="biorouter Desktop" default>
        <MacDesktopInstallButtons/>
        <div style={{ marginTop: '1rem' }}>
          1. Unzip the downloaded zip file.
          2. Run the executable file to launch the biorouter Desktop application.
        </div>
      </TabItem>
      <TabItem value="cli" label="biorouter CLI">
        Run the following command to install biorouter:

        ```sh
        curl -fsSL https://github.com/BaranziniLab/BioRouter/releases/download/stable/download_cli.sh | bash
        ```
      </TabItem>
    </Tabs>
  </TabItem>

  <TabItem value="linux" label="Linux">
    :::caution Coming Soon
    Linux installation for BioRouter is not yet available. We're working on bringing BioRouter to Linux and will release builds soon.

    Please check back later or [watch our GitHub repository](https://github.com/BaranziniLab/BioRouter) for updates.
    :::
  </TabItem>

  <TabItem value="windows" label="Windows">
    :::caution Coming Soon
    Windows installation for BioRouter is not yet available. We're working on bringing BioRouter to Windows and will release builds soon.

    Please check back later or [watch our GitHub repository](https://github.com/BaranziniLab/BioRouter) for updates.
    :::
  </TabItem>
</Tabs>

## Configure Provider

biorouter works with [supported LLM providers](/docs/getting-started/providers) that give biorouter the AI intelligence it needs to understand your requests. On first use, you'll be prompted to configure your preferred provider.

<Tabs groupId="interface">
  <TabItem value="ui" label="biorouter Desktop" default>
  On the welcome screen, you have three options:
  - **Automatic setup with [Tetrate Agent Router](https://tetrate.io/products/tetrate-agent-router-service)**
  - **Automatic Setup with [OpenRouter](https://openrouter.ai/)**
  - **Other Providers**

  For this quickstart, choose `Automatic setup with Tetrate Agent Router`. Tetrate provides access to multiple AI models with built-in rate limiting and automatic failover. For more information about OpenRouter or other providers, see [Configure LLM Provider](/docs/getting-started/providers).
  
  biorouter will open a browser for you to authenticate with Tetrate, or create a new account if you don't have one already. When you return to the biorouter desktop app, you're ready to begin your first session.
      
  :::info Free Credits Offer
  You'll receive $10 in free credits the first time you automatically authenticate with Tetrate through biorouter. This offer is available to both new and existing Tetrate users.
  :::
    
  </TabItem>
  <TabItem value="cli" label="biorouter CLI">
  1. In your terminal, run the following command: 

    ```sh
    biorouter configure
    ```

  2. Select `Configure Providers` from the menu and press Enter.

    ```
   ┌   biorouter-configure 
   │
   ◆  What would you like to configure?
   │  ● Configure Providers (Change provider or update credentials)
   │  ○ Add Extension 
   │  ○ Toggle Extensions 
   │  ○ Remove Extension 
   │  ○ biorouter settings 
   └  
   ```
   3. Choose a model provider. For this quickstart, select `Tetrate Agent Router Service` and press Enter. Tetrate provides access to multiple AI models with built-in rate limiting and automatic failover. For information about other providers, see [Configure LLM Provider](/docs/getting-started/providers).

   ```
   ┌   biorouter-configure 
   │
   ◇  What would you like to configure?
   │  Configure Providers 
   │
   ◆  Which model provider should we use?
   │  ○ Anthropic
   │  ○ Azure OpenAI 
   │  ○ Amazon Bedrock 
   │  ○ Claude Code
   │  ○ Codex CLI
   │  ○ Databricks 
   │  ○ Gemini CLI
   |  ● Tetrate Agent Router Service (Enterprise router for AI models)
   │  ○ ...
   └  
   ```
    :::info Free Credits Offer
    You'll receive $10 in free credits the first time you automatically authenticate with Tetrate through biorouter. This offer is available to both new and existing Tetrate users.
    :::

   4. Enter your API key (and any other configuration details) when prompted.

   ```
   ┌   biorouter-configure 
   │
   ◇  What would you like to configure?
   │  Configure Providers 
   │
   ◇  Which model provider should we use?
   │  Tetrate Agent Router Service 
   │
   ◆  Provider Tetrate Agent Router Service requires TETRATE_API_KEY, please enter a value
   │  ▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪
   └  
   ```
    :::tip GitHub Copilot Authentication
    GitHub Copilot doesn't use an API key. Instead, an authentication code is generated during configuration. To generate the code, select `GitHub Copilot` as your provider. An auth code will be copied to your clipboard, and a browser window will open where you can paste it to complete authentication.

    For more details, see [GitHub Copilot Authentication](/docs/getting-started/providers#github-copilot-authentication).
    :::

   5. Select or search for the model you want to use.
   ```
   │
   ◇  Model fetch complete
   │
   ◆  Select a model:
   │  ○ Search all models...
   │  ○ gemini-2.5-pro
   │  ○ gemini-2.0-flash
   |  ○ gemini-2.0-flash-lite
   │  ● gpt-5 (Recommended)
   |  ○ gpt-5-mini
   |  ○ gpt-5-nano
   |  ○ gpt-4.1
   │
   ◓  Checking your configuration...
   └  Configuration saved successfully
   ```
  </TabItem>
</Tabs>

## Start Session
Sessions are single, continuous conversations between you and biorouter. Let's start one.

<Tabs groupId="interface">
    <TabItem value="ui" label="biorouter Desktop" default>
        After choosing an LLM provider, click the `Home` button in the sidebar.

        Type your questions, tasks, or instructions directly into the input field, and biorouter will immediately get to work.
    </TabItem>
    <TabItem value="cli" label="biorouter CLI">
        1. Make an empty directory (e.g. `biorouter-demo`) and navigate to that directory from the terminal.
        2. To start a new session, run:
        ```sh
        biorouter session
        ```

        :::tip biorouter Web
        CLI users can also start a session in [biorouter Web](/docs/guides/biorouter-cli-commands#web), a web-based chat interface:
        ```sh
        biorouter web --open
        ```
        :::

    </TabItem>
</Tabs>

## Write Prompt

From the prompt, you can interact with biorouter by typing your instructions exactly as you would speak to a developer.

Let's ask biorouter to make a tic-tac-toe game!

```
create an interactive browser-based tic-tac-toe game in javascript where a player competes against a bot
```

biorouter will create a plan and then get right to work on implementing it. Once done, your directory should contain a JavaScript file as well as an HTML page for playing.

## Enable an Extension

While you're able to manually navigate to your working directory and open the HTML file in a browser, wouldn't it be better if biorouter did that for you? Let's give biorouter the ability to open a web browser by enabling the [`Computer Controller` extension](/docs/mcp/computer-controller-mcp).

<Tabs groupId="interface">

    <TabItem value="ui" label="biorouter Desktop" default>
        1. Click the <PanelLeft className="inline" size={16} /> button in the top-left to open the sidebar.
        2. Click `Extensions` in the sidebar menu.
        3. Toggle the `Computer Controller` extension to enable it. This extension enables webscraping, file caching, and automations.
        4. Return to your session to continue.
        5. Now that biorouter has browser capabilities, let's ask it to launch your game in a browser:
    </TabItem>
    <TabItem value="cli" label="biorouter CLI">
        1. End the current session by entering `Ctrl+C` so that you can return to the terminal's command prompt.
        2. Run the configuration command
        ```sh
        biorouter configure
        ```
        3. Choose `Add Extension` > `Built-in Extension` > `Computer Controller`, and set the timeout to 300s. This extension enables webscraping, file caching, and automations.
        ```
        ┌   biorouter-configure
        │
        ◇  What would you like to configure?
        │  Add Extension
        │
        ◇  What type of extension would you like to add?
        │  Built-in Extension
        │
        ◇  Which built-in extension would you like to enable?
        │  Computer Controller
        │
        ◇  Please set the timeout for this tool (in secs):
        │  300
        │
        └  Enabled computercontroller extension
        ```
        4. Now that biorouter has browser capabilities, let's resume your last session:
        ```sh
         biorouter session -r
        ```
        5. Ask biorouter to launch your game in a browser:
    </TabItem>
</Tabs>

```
open the tic-tac-toe game in a browser
```

Go ahead and play your game, I know you want to 😂 ... good luck!

## Next Steps
Congrats, you've successfully used biorouter to develop a web app! 🎉

Here are some ideas for next steps:
* Continue your session with biorouter and improve your game (styling, functionality, etc).
* Browse other available [extensions](/extensions) and install more to enhance biorouter's functionality even further.
* Provide biorouter with a [set of hints](/docs/guides/context-engineering/using-biorouterhints) to use within your sessions.
* See how you can set up [access controls](/docs/mcp/developer-mcp#configuring-access-controls) if you don't want biorouter to work autonomously.