Mobile access lets you connect to biorouter remotely from an iOS mobile device using secure tunneling.

:::warning Experimental Feature
Mobile access is a preview feature in active development. Behavior and configuration may change in future releases.
:::

## How Mobile Access Works

Mobile access connects your iOS device to biorouter Desktop through a secure tunnel. After you install and configure the **BioRouter AI** app, you can access biorouter from anywhere.

**Key details:**
- Uses [Lapstone](https://github.com/michaelneale/lapstone-tunnel), a public HTTPS tunnel service provided by Mic Neale
- Easy setup using a QR code with a unique secret key to secure the connection
- Your tunnel URL remains the same across sessions, so you only need to configure your mobile app once
- The connection requires your computer to be awake with biorouter Desktop running
- Automatically reconnects if interrupted and restarts when you launch biorouter Desktop

## Setup

### Install the App
1. Install the **BioRouter AI** app on your iOS mobile device from the [App Store](https://apps.apple.com/app/biorouter-ai/id6752889295)

:::tip App Store QR Code
Follow the steps below to open the `Remote Access` section, then click "scan QR code" in the info box for quick access to the App Store.
:::

### Start the Tunnel
1. Open biorouter Desktop
2. Click the <PanelLeft className="inline" size={16} /> button in the top-left to open the sidebar
3. Click `Settings` in the sidebar
4. Click `App`
5. Scroll down to the `Remote Access` section and click `Start Tunnel`

Once the tunnel starts, you'll see a `Remote Access Connection` QR code for configuring the app.

:::info 
Click `Stop Tunnel` at any time to close the connection.
:::

### Connect the App
1. Open the **BioRouter AI** app on your iOS mobile device
2. Scan the `Remote Access Connection` QR code displayed in biorouter Desktop
3. The app will automatically configure the connection

You can now access biorouter Desktop from your mobile device.

## What You Can Do

The mobile app gives you full access to biorouter:
- Start new conversations or continue existing sessions
- Use all your biorouter extensions and configurations
- Work from anywhere while your computer handles the processing

## Additional Resources

For more information about mobile access features, check the [BioRouter GitHub repository](https://github.com/BaranziniLab/BioRouter) for updates and announcements.