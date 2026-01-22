The biorouter CLI and desktop apps are under active and continuous development. To get the newest features and fixes, you should periodically update your biorouter client using the following instructions.

<Tabs>
  <TabItem value="mac" label="macOS" default>
    <Tabs groupId="interface">
      <TabItem value="ui" label="biorouter Desktop" default>
        Update biorouter to the latest stable version.

        <DesktopAutoUpdateSteps />
        
        **To manually download and install updates:**
        1. <MacDesktopInstallButtons/>
        2. Unzip the downloaded zip file
        3. Drag the extracted `Goose.app` file to the `Applications` folder to overwrite your current version
        4. Launch biorouter Desktop

      </TabItem>
      <TabItem value="cli" label="biorouter CLI">
        You can update biorouter by running:

        ```sh
        biorouter update
        ```

        Additional [options](/docs/guides/biorouter-cli-commands#update-options):
        
        ```sh
        # Update to latest canary (development) version
        biorouter update --canary

        # Update and reconfigure settings
        biorouter update --reconfigure
        ```

        Or you can run the [installation](/docs/getting-started/installation) script again:

        ```sh
        curl -fsSL https://github.com/BaranziniLab/BioRouter/releases/download/stable/download_cli.sh | CONFIGURE=false bash
        ```

        To check your current biorouter version, use the following command:

        ```sh
        biorouter --version
        ```
      </TabItem>
    </Tabs>
  </TabItem>

  <TabItem value="linux" label="Linux">
    <Tabs groupId="interface">
      <TabItem value="ui" label="biorouter Desktop" default>
        Update biorouter to the latest stable version.

        <DesktopAutoUpdateSteps />
        
        **To manually download and install updates:**
        1. <LinuxDesktopInstallButtons/>

        #### For Debian/Ubuntu-based distributions
        2. In a terminal, navigate to the downloaded DEB file
        3. Run `sudo dpkg -i (filename).deb`
        4. Launch biorouter from the app menu
      </TabItem>
      <TabItem value="cli" label="biorouter CLI">
        You can update biorouter by running:

        ```sh
        biorouter update
        ```

        Additional [options](/docs/guides/biorouter-cli-commands#update-options):
        
        ```sh
        # Update to latest canary (development) version
        biorouter update --canary

        # Update and reconfigure settings
        biorouter update --reconfigure
        ```

        Or you can run the [installation](/docs/getting-started/installation) script again:

        ```sh
        curl -fsSL https://github.com/BaranziniLab/BioRouter/releases/download/stable/download_cli.sh | CONFIGURE=false bash
        ```

        To check your current biorouter version, use the following command:

        ```sh
        biorouter --version
        ```
      </TabItem>
    </Tabs>
  </TabItem>

  <TabItem value="windows" label="Windows">
    <Tabs groupId="interface">
      <TabItem value="ui" label="biorouter Desktop" default>
        Update biorouter to the latest stable version.

        <DesktopAutoUpdateSteps />
        
        **To manually download and install updates:**
        1. <WindowsDesktopInstallButtons/>
        2. Unzip the downloaded zip file
        3. Run the executable file to launch the biorouter Desktop app
      </TabItem>
      <TabItem value="cli" label="biorouter CLI">
        You can update biorouter by running:

        ```sh
        biorouter update
        ```

        Additional [options](/docs/guides/biorouter-cli-commands#update-options):
        
        ```sh
        # Update to latest canary (development) version
        biorouter update --canary

        # Update and reconfigure settings
        biorouter update --reconfigure
        ```

        Or you can run the [installation](/docs/getting-started/installation) script again in **Git Bash**, **MSYS2**, or **PowerShell** to update the biorouter CLI natively on Windows:

        ```bash
        curl -fsSL https://github.com/BaranziniLab/BioRouter/releases/download/stable/download_cli.sh | CONFIGURE=false bash
        ```
        
        To check your current biorouter version, use the following command:

        ```sh
        biorouter --version
        ```        

        <details>
        <summary>Update via Windows Subsystem for Linux (WSL)</summary>

        To update your WSL installation, use `biorouter update` or run the installation script again via WSL:

        ```sh
        curl -fsSL https://github.com/BaranziniLab/BioRouter/releases/download/stable/download_cli.sh | CONFIGURE=false bash
        ```

       </details>
      </TabItem>
    </Tabs>
  </TabItem>
</Tabs>