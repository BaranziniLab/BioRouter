Biorouter’s permissions determine how much autonomy it has when modifying files, using extensions, and performing automated actions. By selecting a permission mode, you have full control over how biorouter interacts with your development environment.

**Permission Modes Video Walkthrough**

## Permission Modes

| Mode | Description | Best For |
|------|-------------|----------|
| **Completely Autonomous** | biorouter can modify files, use extensions, and delete files **without requiring approval** | Users who want **full automation** and seamless integration into their workflow |
| **Manual Approval** | biorouter **asks for confirmation** before using any tools or extensions (supports granular [tool permissions](/docs/guides/managing-tools/tool-permissions)) | Users who want to **review and approve** every change and tool usage |
| **Smart Approval** | biorouter uses a risk-based approach to **automatically approve low-risk actions** and **flag others** for approval (supports granular [tool permissions](/docs/guides/managing-tools/tool-permissions))  | Users who want a **balanced mix of autonomy and oversight** based on the action’s impact |
| **Chat Only** | biorouter **only engages in chat**, with no extension use or file modifications | Users who prefer a **conversational AI experience** for analysis, writing, and reasoning tasks without automation |

> **Warning:** `Autonomous Mode` is applied by default.

## Configuring biorouter mode

Here's how to configure:

  

    You can change modes before or during a session and it will take effect immediately.

     
      

      Click the  mode button from the bottom menu. 
      
      
        1. Click the  button on the top-left to open the sidebar.
        2. Click the `Settings` button on the sidebar.
        3. Click `Chat`.
        4. Under `Mode`, choose the mode you'd like.
      
       
  
  

    
      
        To change modes mid-session, use the `/mode` command.

        * Autonomous: `/mode auto`
        * Smart Approve: `/mode smart_approve`
        * Approve: `/mode approve`
        * Chat: `/mode chat`     
      
      
        1. Run the following command:

        ```sh
        biorouter configure
        ```

        2. Select `biorouter settings` from the menu and press Enter.

        ```sh
        ┌ biorouter-configure
        │
        ◆ What would you like to configure?
        | ○ Configure Providers
        | ○ Add Extension
        | ○ Toggle Extensions
        | ○ Remove Extension
        // highlight-start
        | ● biorouter settings (Set the biorouter mode, Tool Output, Tool Permissions, Experiment, biorouter workflow github repo and more)
        // highlight-end
        └
        ```

        3. Choose `biorouter mode` from the menu and press Enter.

        ```sh
        ┌   biorouter-configure
        │
        ◇  What would you like to configure?
        │  biorouter settings 
        │
        ◆  What setting would you like to configure?
        // highlight-start
        │  ● biorouter mode (Configure biorouter mode)
        // highlight-end
        │  ○ Router Tool Selection Strategy 
        │  ○ Tool Permission 
        │  ○ Tool Output 
        │  ○ Max Turns 
        │  ○ Toggle Experiment 
        │  ○ biorouter workflow github repo 
        │  ○ Scheduler Type 
        └
        ```

        4.  Choose the biorouter mode you would like to configure.

        ```sh
        ┌   biorouter-configure
        │
        ◇  What would you like to configure?
        │  biorouter settings
        │
        ◇  What setting would you like to configure?
        │  biorouter mode
        │
        ◆  Which biorouter mode would you like to configure?
        // highlight-start
        │  ● Auto Mode (Full file modification, extension usage, edit, create and delete files freely)
        // highlight-end
        |  ○ Approve Mode
        |  ○ Smart Approve Mode    
        |  ○ Chat Mode
        |
        └  Set to Auto Mode - full file modification enabled
        ```     
      
    
  

  
> **Info:** In manual and smart approval modes, you will see "Allow" and "Deny" buttons in your session windows during tool calls. 
  biorouter will only ask for permission for tools that it deems are 'write' tools, e.g. any 'text editor write', 'text editor edit', 'bash - rm, cp, mv' commands. 
  
  Read/write approval makes best effort attempt at classifying read or write tools. This is interpreted by your LLM provider.
