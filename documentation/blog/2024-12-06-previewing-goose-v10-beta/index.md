---
draft: false
title: "Previewing biorouter v1.0 Beta"
description: "biorouter v1.0 Beta is here! Learn about the latest features and improvements."
date: 2024-12-06
authors:
  - adewale
---

![goose v1.0 Beta](goose-v1.0-beta.png)
We are excited to share a preview of the new updates coming to biorouter with biorouter v1.0 Beta!

This major update comes with a bunch of new features and improvements that make biorouter more powerful and user-friendly. Here are some of the key highlights.

<!-- truncate -->


## Exciting Features of biorouter 1.0 Beta

### 1. Transition to Rust

The core of biorouter has been rewritten in Rust. Why does this matter? Rust allows for a more portable and stable experience. This change means that biorouter can run smoothly on different systems without the need for Python to be installed, making it easier for anyone to start using it.

### 2. Contextual Memory

biorouter will remember previous interactions to better understand ongoing projects. This means you won’t have to keep repeating yourself. Imagine having a conversation with someone who remembers every detail—this is the kind of support biorouter aims to offer.

### 3. Improved Plugin System

In biorouter v1.0, the biorouter toolkit system is being replaced with Extensions. Extensions are modular daemons that biorouter can interact with dynamically. As a result, biorouter will be able to support more complex plugins and integrations. This will make it easier to extend biorouter with new features and functionality.

### 4. Headless mode

You can now run biorouter in headless mode - this is useful for running biorouter on servers or in environments where a graphical interface is not available.

```sh
cargo run --bin biorouter -- run -i instructions.md
```

### 5. biorouter now has a GUI

biorouter now has an electron-based GUI macOS application that provides and alternative to the CLI to interact with biorouter and manage your projects.

![goose GUI](goose-gui.png)

### 6. biorouter alignment with open protocols

biorouter v1.0 Beta now uses a custom protocol, that is designed in parallel with [Anthropic’s Model Context Protocol](https://www.anthropic.com/news/model-context-protocol) (MCP) to communicate with Systems. This makes it possible for developers to create their own systems (e.g Jira, ) that BioRouter can integrate with. 

Excited for many more feature updates and improvements? Stay tuned for more updates on Goose! Check out the [BioRouter repo](https://github.com/BaranziniLab/BioRouter) and join our [Discord community](https://discord.gg/biorouter-oss).


<head>
  <meta property="og:title" content="Previewing BioRouter v1.0 Beta" />
  <meta property="og:type" content="article" />
  <meta property="og:url" content="https://baranzinilab.github.io/BioRouter/blog/2024/12/06/previewing-goose-v10-beta" />
  <meta property="og:description" content="AI Agent uses screenshots to assist in styling." />
  <meta property="og:image" content="https://baranzinilab.github.io/BioRouter/assets/images/goose-v1.0-beta-5d469fa73edea37cfccfe8a8ca0b47e2.png" />
  <meta name="twitter:card" content="summary_large_image" />
  <meta property="twitter:domain" content="baranzinilab.github.io/BioRouter" />
  <meta name="twitter:title" content="Screenshot-Driven Development" />
  <meta name="twitter:description" content="AI Agent uses screenshots to assist in styling." />
  <meta name="twitter:image" content="https://baranzinilab.github.io/BioRouter/assets/images/goose-v1.0-beta-5d469fa73edea37cfccfe8a8ca0b47e2.png" />
</head>