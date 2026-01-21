---
sidebar_position: 105
title: Using biorouter in ACP Clients
sidebar_label: biorouter in ACP Clients
---

Client applications that support the [Agent Client Protocol (ACP)](https://agentclientprotocol.com/) can connect natively to biorouter. This integration allows you to seamlessly interact with biorouter directly from the client.

:::warning Experimental Feature
ACP is an emerging specification that enables clients to communicate with AI agents like biorouter. This feature has limited adoption and may evolve as the protocol develops.
:::

## How It Works
After you configure biorouter as an agent in the ACP client, you gain access to biorouter's core agent functionality, including its extensions and tools. biorouter also automatically loads any [configured MCP servers](#using-mcp-servers-from-acp-clients) from your ACP client alongside its own extensions, making their tools available without additional configuration.

The client manages the biorouter lifecycle automatically, including:

- **Initialization**: The client runs the `biorouter acp` command to initialize the connection
- **Communication**: The client communicates with biorouter over stdio using JSON-RPC
- **Multiple Sessions**: The client manages multiple concurrent biorouter conversations simultaneously

:::info Session Persistence
ACP sessions are saved to biorouter's session history where you can access and manage them using biorouter. Access to session history in ACP clients might vary.
:::

:::tip Reference Implementation
The [biorouter for VS Code](/docs/experimental/vs-code-extension) extension uses ACP to communicate with biorouter. See the [vscode-biorouter](https://github.com/block/vscode-biorouter) repository for implementation details.
:::

## Setup in ACP Clients
Any editor or IDE that supports ACP can connect to biorouter as an agent server. Check the [official ACP clients list](https://agentclientprotocol.com/overview/clients) for available clients with links to their documentation.

### Example: Zed Editor Setup

ACP was originally developed by [Zed](https://zed.dev/). Here's how to configure biorouter in Zed:

#### 1. Prerequisites

Ensure you have both Zed and biorouter CLI installed:

- **Zed**: Download from [zed.dev](https://zed.dev/)
- **biorouter CLI**: Follow the [installation guide](/docs/getting-started/installation)

  - ACP support works best with version 1.16.0 or later - check with `biorouter --version`.

  - Temporarily run `biorouter acp` to test that ACP support is working:

    ```
    ~ biorouter acp
    BioRouter ACP agent started. Listening on stdio...
    ```

    Press `Ctrl+C` to exit the test.

#### 2. Configure biorouter as a Custom Agent

Add biorouter to your Zed settings:

1. Open Zed
2. Press `Cmd+Option+,` (macOS) or `Ctrl+Alt+,` (Linux/Windows) to open the settings file
3. Add the following configuration:

```json
{
  "agent_servers": {
    "biorouter": {
      "command": "biorouter",
      "args": ["acp"],
      "env": {}
    }
  },
  // more settings
}
```

You should now be able to interact with biorouter directly in Zed. Your ACP sessions use the same extensions that are enabled in your biorouter configuration, and your tools (Developer, Computer Controller, etc.) work the same way as in regular biorouter sessions.

#### 3. Start Using biorouter in Zed

1. **Open the Agent Panel**: Click the sparkles agent icon in Zed's status bar
2. **Create New Thread**: Click the `+` button to show thread options
3. **Select biorouter**: Choose `New biorouter` to start a new conversation with biorouter
4. **Start Chatting**: Interact with biorouter directly from the agent panel

#### Advanced Configuration

##### Overriding Provider and Model

By default, biorouter will use the provider and model defined in your [configuration file](/docs/guides/config-files). You can override this for specific ACP configurations using the `BIOROUTER_PROVIDER` and `BIOROUTER_MODEL` environment variables.

The following Zed settings example configures two biorouter agent instances. This is useful for:
- Comparing model performance on the same task
- Using cost-effective models for simple tasks and powerful models for complex ones

```json
{
  "agent_servers": {
    "biorouter": {
      "command": "biorouter",
      "args": ["acp"],
      "env": {}
    },
    "biorouter (GPT-4o)": {
      "command": "biorouter",
      "args": ["acp"],
      "env": {
        "BIOROUTER_PROVIDER": "openai",
        "BIOROUTER_MODEL": "gpt-4o"
      }
    }
  },
  // more settings
}
```

## Using MCP Servers from ACP Clients

MCP servers configured in the ACP client's `context_servers` are automatically available to biorouter. This allows you to use those MCP servers when using both native client features and the biorouter agent integration.

**Example (Zed):**

```json
{
  "context_servers": {
    "filesystem": {
      "command": "npx",
      "args": [
        "-y",
        "@modelcontextprotocol/server-filesystem",
        "/path/to/allowed/dir"
      ]
    }
  },
  "agent_servers": {
    "biorouter": {
      "command": "biorouter",
      "args": ["acp"],
      "env": {}
    }
  },
  // more settings
}
```

To find out what tools are available, just ask biorouter while it's running in the client.

:::info
All MCP servers in `context_servers` are automatically available to biorouter, provided that they use stdio (command-based) or HTTP transports. biorouter doesn't support servers that use the deprecated SSE transport.

If a server in `context_servers` has the same name as a biorouter extension, biorouter uses its own [configuration](/docs/guides/config-files).
:::
## Additional Resources

import ContentCardCarousel from '@site/src/components/ContentCardCarousel';
import chooseYourIde from '@site/blog/2025-10-24-intro-to-agent-client-protocol-acp/choose-your-ide.png';

<ContentCardCarousel
  items={[
    {
      type: 'video',
      title: 'Intro to Agent Client Protocol (ACP) | Vibe Code with biorouter',
      description: 'Watch how ACP lets you seamlessly integrate biorouter into your code editor to streamline fragmented workflows.',
      thumbnailUrl: 'https://img.youtube.com/vi/Hvu5KDTb6JE/maxresdefault.jpg',
      linkUrl: 'https://www.youtube.com/watch?v=Hvu5KDTb6JE',
      date: '2025-10-16',
      duration: '50:23'
    },
   {
      type: 'blog',
      title: 'Intro to Agent Client Protocol (ACP): The Standard for AI Agent-Editor Integration',
      description: 'Learn how to integrate AI agents like biorouter directly into your code editor via ACP, eliminating window-switching and vendor lock-in.',
      thumbnailUrl: chooseYourIde,
      linkUrl: '/BioRouter/blog/2025/10/24/intro-to-agent-client-protocol-acp',
      date: '2025-10-24',
      duration: '7 min read'
    }
  ]}
/>
