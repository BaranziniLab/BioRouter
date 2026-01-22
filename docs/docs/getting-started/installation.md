# Install biorouter

<Tabs>
  <TabItem value="mac" label="macOS" default>
    Choose to install the Desktop and/or CLI version of biorouter:

    <Tabs groupId="interface">
      <TabItem value="ui" label="biorouter Desktop" default>
        Install biorouter Desktop directly from the browser or with [Homebrew](https://brew.sh/).

        <h3 style={{ marginTop: '1rem' }}>Option 1: Install via Download</h3>
        <MacDesktopInstallButtons/>

        <div style={{ marginTop: '1rem' }}>
          1. Unzip the downloaded zip file.
          2. Run the executable file to launch the biorouter Desktop application.

          :::tip Updating biorouter
          It's best to periodically [update biorouter](/docs/guides/updating-biorouter).
          :::
        </div>
        <h3>Option 2: Install via Homebrew</h3>
        Homebrew downloads the [same app](https://github.com/Homebrew/homebrew-cask/blob/master/Casks/b/block-biorouter.rb) but can take care of updates too.
        ```bash
          brew install --cask block-biorouter
        ```
        ---
        <div style={{ marginTop: '1rem' }}>
          :::info Permissions
          If you're on an Apple Mac M3 and the biorouter Desktop app shows no window on launch, check and update the following:

          Ensure the `~/.config` directory has read and write access.

          biorouter needs this access to create the log directory and file. Once permissions are granted, the app should load correctly. For steps on how to do this, refer to the  [Known Issues Guide](/docs/troubleshooting/known-issues#macos-permission-issues)
          :::
        </div>
      </TabItem>
      <TabItem value="cli" label="biorouter CLI">
        Install biorouter directly from the browser or with [Homebrew](https://brew.sh/).

        <h3 style={{ marginTop: '1rem' }}>Option 1: Install via Download script</h3>
        Run the following command to install the latest version of biorouter on macOS:

        ```sh
        curl -fsSL https://github.com/BaranziniLab/BioRouter/releases/download/stable/download_cli.sh | bash
        ```
        This script will fetch the latest version of biorouter and set it up on your system.

        If you'd like to install without interactive configuration, disable `CONFIGURE`:

        ```sh
        curl -fsSL https://github.com/BaranziniLab/BioRouter/releases/download/stable/download_cli.sh | CONFIGURE=false bash
        ```

        :::tip Updating biorouter
        It's best to keep biorouter updated. To update biorouter, run:
        ```sh
        biorouter update
        ```
        :::

        <h3>Option 2: Install via Homebrew</h3>
        Homebrew downloads the [a precompiled CLI tool](https://github.com/Homebrew/homebrew-core/blob/master/Formula/b/block-biorouter-cli.rb) and can take care of updates.
        ```bash
        brew install block-biorouter-cli
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

## Set LLM Provider
biorouter works with [supported LLM providers][providers] that give biorouter the AI intelligence it needs to understand your requests. On first use, you'll be prompted to configure your preferred provider.

<Tabs groupId="interface">
  <TabItem value="ui" label="biorouter Desktop" default>
    On the welcome screen, choose how to configure a provider:
    <OnboardingProviderSetup />
  </TabItem>
  <TabItem value="cli" label="biorouter CLI">
    The CLI automatically enters configuration mode where you can choose how to configure a provider:

    <OnboardingProviderSetup />

    Example configuration flow:

    ```
    ┌   biorouter-configure
    │
    ◇ How would you like to set up your provider?
    │ Tetrate Agent Router Service Login
    │
    Opening browser for Tetrate Agent Router Service authentication...
    [biorouter opens the browser and prints details]

    Authentication complete!

    Configuring Tetrate Agent Router Service...
    ✓ Tetrate Agent Router Service configuration complete
    ✓ Models configured successfully

    Testing configuration...
    ✓ Configuration test passed!
    ✓ Developer extension enabled!
    └ Tetrate Agent Router Service setup complete! You can now use biorouter.
  ```

  :::info Windows Users
  If you choose to manually configure a provider, when prompted during configuration, choose to not store to keyring. If you encounter keyring errors when setting API keys, you can set environment variables manually instead:

  ```bash
  export OPENAI_API_KEY={your_api_key}
  ```

  Then run `biorouter configure` again. biorouter will detect the environment variable and display:

  ```
  ● OPENAI_API_KEY is set via environment variable
  ```

  To make API keys persist across sessions, add them to your shell profile:
  ```bash
  echo 'export OPENAI_API_KEY=your_api_key' >> ~/.bashrc
  source ~/.bashrc
  ```
  :::
  </TabItem>
</Tabs>

:::tip
<ModelSelectionTip />
:::

## Update Provider
You can change your LLM provider and/or model or update your API key at any time.

<Tabs groupId="interface">
  <TabItem value="ui" label="biorouter Desktop" default>
    1. Click the <PanelLeft className="inline" size={16} /> button in the top-left to open the sidebar.
    2. Click the `Settings` button on the sidebar.
    3. Click the `Models` tab.
    4. Choose to update your provider, switch models, or click `Reset Provider and Model` to clear your settings and return to the welcome screen. See details about these [configuration options](/docs/getting-started/providers#configure-provider-and-model).
  </TabItem>
  <TabItem value="cli" label="biorouter CLI">
    1. Run the following command:
    ```sh
    biorouter configure
    ```
    2. Select `Configure Providers` from the menu.
    3. Follow the prompts to choose your LLM provider and enter or update your API key.

    **Example:**

    To select an option during configuration, use the up and down arrows to highlight your choice then press Enter.

    ```
    ┌   biorouter-configure
    │
    ◇ What would you like to configure?
    │ Configure Providers
    │
    ◇ Which model provider should we use?
    │ Google Gemini
    │
    ◇ Provider Google Gemini requires GOOGLE_API_KEY, please enter a value
    │▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪▪
    │
    ◇ Enter a model from that provider:
    │ gemini-2.0-flash-exp
    │
    ◇  Hello there! You're all set to use me, so please ask away!
    │
    └  Configuration saved successfully
    ```
  </TabItem>
</Tabs>

<RateLimits />

## Running biorouter

<Tabs groupId="interface">
    <TabItem value="ui" label="biorouter Desktop" default>
        Starting a session in the biorouter Desktop is straightforward. After choosing your provider, you'll see the session interface ready for use.

        Type your questions, tasks, or instructions directly into the input field, and biorouter will get to work immediately.
    </TabItem>
    <TabItem value="cli" label="biorouter CLI">
        From your terminal, navigate to the directory you'd like to start from and run:
        ```sh
        biorouter session
        ```
    </TabItem>
</Tabs>

## Shared Configuration Settings

The biorouter CLI and Desktop UI share all core configurations, including LLM provider settings, model selection, and extension configurations. When you install or configure extensions in either interface, the settings are stored in a central location, making them available to both the Desktop application and CLI. This makes it convenient to switch between interfaces while maintaining consistent settings. For more information, visit the [Config Files][config-files] guide.

:::info
While core configurations are shared between interfaces, extensions have flexibility in how they store authentication credentials. Some extensions may use the shared config files while others implement their own storage methods.
:::

<Tabs groupId="interface">
    <TabItem value="ui" label="biorouter Desktop" default>
        Navigate to shared configurations through:
        1. Click the <PanelLeft className="inline" size={16} /> button in the top-left to open the sidebar.
        2. Click the `Settings` button on the sidebar.
    </TabItem>
    <TabItem value="cli" label="biorouter CLI">
        Use the following command to manage shared configurations:
        ```sh
        biorouter configure
        ```
    </TabItem>
</Tabs>

## Additional Resources

You can also configure Extensions to extend biorouter's functionality, including adding new ones or toggling them on and off. For detailed instructions, visit the [Using Extensions Guide][using-extensions].

[using-extensions]: /docs/getting-started/using-extensions
[providers]: /docs/getting-started/providers
[handling-rate-limits]: /docs/guides/handling-llm-rate-limits-with-biorouter
[mcp]: https://www.anthropic.com/news/model-context-protocol
[config-files]: /docs/guides/config-files.md