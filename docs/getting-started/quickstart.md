# biorouter in 5 minutes

biorouter is an AI-powered integrated research environment for biomedical discovery, built by UCSF's Baranzini Lab. It unifies multiple LLM providers, AI agents, and MCP-based extensions into a single extensible tool.

This quick tutorial will guide you through:

- ✅ Installing biorouter
- ✅ Configuring your LLM
- ✅ Running your first biomedical research task
- ✅ Adding an MCP server

Let's begin 🚀

## Install biorouter

  
    Choose to install the Desktop and/or CLI version of biorouter:

    
      
        
        <div style={{ marginTop: '1rem' }}>
          1. Unzip the downloaded zip file.
          2. Run the executable file to launch the biorouter Desktop application.
        
      
      
        The `biorouter` CLI ships inside the desktop app. On macOS and Windows, install the app, then run `biorouter setup-path` to add the bundled `biorouter` binary to your `PATH`. On Linux, install the standalone CLI package from the [download page](http://biorouter.ucsf.edu/download) (`biorouter-cli_*_amd64.deb` or `biorouter-cli-*-1.x86_64.rpm`), which provides `biorouter` and `biorouterd` without the desktop app.
      
    
  

  
    
Linux builds are available. Install the desktop app with the `biorouter_*_amd64.deb` (Debian/Ubuntu) or `Biorouter-*-1.x86_64.rpm` (Fedora/RHEL) package, or install the standalone CLI with the `biorouter-cli_*_amd64.deb` / `biorouter-cli-*-1.x86_64.rpm` package. Grab them from the [download page](http://biorouter.ucsf.edu/download).

  

  
    
Windows builds are available. Download `Biorouter-win32-x64-*.zip` from the [download page](http://biorouter.ucsf.edu/download), unzip it, and run `Biorouter.exe`. The `biorouter` CLI is bundled inside the app — run `biorouter setup-path` to add it to your `PATH`.

  

## Configure Provider

biorouter works with [supported LLM providers](/docs/getting-started/providers) that give biorouter the AI intelligence it needs to understand your requests. On first use, you'll be prompted to configure your preferred provider.

  
  On the welcome screen, you have three options:
  - **Automatic setup with [Tetrate Agent Router](https://tetrate.io/products/tetrate-agent-router-service)**
  - **Automatic Setup with [OpenRouter](https://openrouter.ai/)**
  - **Other Providers**

  For this quickstart, choose `Automatic setup with Tetrate Agent Router`. Tetrate provides access to multiple AI models with built-in rate limiting and automatic failover. For more information about OpenRouter or other providers, see [Configure LLM Provider](/docs/getting-started/providers).
  
  biorouter will open a browser for you to authenticate with Tetrate, or create a new account if you don't have one already. When you return to the biorouter desktop app, you're ready to begin your first session.
      
    
  
  
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
    
> **Tip:** GitHub Copilot doesn't use an API key. Instead, an authentication code is generated during configuration. To generate the code, select `GitHub Copilot` as your provider. An auth code will be copied to your clipboard, and a browser window will open where you can paste it to complete authentication.

    For more details, see [GitHub Copilot Authentication](/docs/getting-started/providers#github-copilot-authentication).

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
  

## Start Session
Sessions are single, continuous conversations between you and biorouter. Let's start one.

    
        After choosing an LLM provider, click the `Home` button in the sidebar.

        Type your questions, tasks, or instructions directly into the input field, and biorouter will immediately get to work.
    
    
        1. Make an empty directory (e.g. `biorouter-demo`) and navigate to that directory from the terminal.
        2. To start a new session, run:
        ```sh
        biorouter session
        ```

        
> **Tip:** CLI users can also start a session in [biorouter Web](/docs/guides/biorouter-cli-commands#web), a web-based chat interface:
        ```sh
        biorouter web --open
        ```

    

## Write Prompt

From the prompt, you can interact with biorouter by typing your instructions exactly as you would describe a task to a research assistant.

Let's ask biorouter to do a quick literature analysis!

```
find three recent review papers on the role of microglia in multiple sclerosis, then write a one-page markdown summary comparing their main conclusions
```

biorouter will create a plan and then get right to work on it. Once done, your directory should contain a markdown summary of the papers it analyzed.

## Enable an Extension

While biorouter can already work with files in your directory, wouldn't it be better if it could fetch papers and data from the web for you? Let's give biorouter the ability to browse the web and scrape sources by enabling the [`Computer Controller` extension](/docs/mcp/computer-controller-mcp).

    
        1. Click the  button in the top-left to open the sidebar.
        2. Click `Extensions` in the sidebar menu.
        3. Toggle the `Computer Controller` extension to enable it. This extension enables webscraping, file caching, and automations.
        4. Return to your session to continue.
        5. Now that biorouter has browser capabilities, let's ask it to pull an abstract from the web:
    
    
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
        5. Ask biorouter to fetch a paper's abstract from the web:
    

```
fetch the abstract of the most-cited paper on PubMed for "BRCA1 breast cancer risk" and add it to my summary
```

Go ahead and review the results — your literature summary now pulls directly from the source.

## Next Steps
Congrats, you've successfully used biorouter for your first biomedical research task! 🎉

Here are some ideas for next steps:
* Continue your session with biorouter and deepen the analysis (add more papers, extract data tables, draw a figure, etc).
* Browse other available [extensions](/extensions) and install more to enhance biorouter's functionality even further.
* Provide biorouter with a [set of hints](/docs/guides/context-engineering/using-biorouterhints) to use within your sessions.
* See how you can set up [access controls](/docs/mcp/developer-mcp#configuring-access-controls) if you don't want biorouter to work autonomously.
