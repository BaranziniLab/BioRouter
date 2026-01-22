<h1 className={styles.pageTitle}>Context Engineering</h1>
<p className={styles.pageDescription}>
  Context engineering is about building background knowledge, preferences, and workflows that help biorouter work more effectively. Instead of repeating instructions, you define them once and teach biorouter how you work.
</p>

<div className={styles.categorySection}>
  <h2 className={styles.categoryTitle}>📚 Documentation & Guides</h2>
  <div className={styles.cardGrid}>
    <Card 
      title="Using biorouterhints"
      description="Use AGENTS.md, .biorouterhints, and other files to provide project context, preferences, and instructions that biorouter loads automatically."
      link="/docs/guides/context-engineering/using-biorouterhints"
    />
    <Card 
      title="Using Skills"
      description="Create reusable instruction sets containing workflows, scripts, and other resources that biorouter can load on demand."
      link="/docs/guides/context-engineering/using-skills"
    />
    <Card 
      title="Custom Slash Commands"
      description="Create custom shortcuts to quickly run reusable instructions in any chat session with simple slash commands."
      link="/docs/guides/context-engineering/slash-commands"
    />
    <Card 
      title="Memory Extension"
      description="Teach biorouter persistent knowledge it can recall across sessions. Save commands, code snippets, and preferences for consistent assistance."
      link="/docs/mcp/memory-mcp"
    />
    <Card 
      title="Research → Plan → Implement Pattern"
      description="See how slash commands make it easy to integrate instructions into interactive RPI workflows."
      link="/docs/tutorials/rpi"
    />
  </div>
</div>

<div className={styles.categorySection}>
  <h2 className={styles.categoryTitle}>📝 Featured Blog Posts</h2>
  <div className={styles.cardGrid}>
    <Card
      title="Stop Your AI Agent From Making Unwanted Changes"
      description="Teach your AI agent how to commit early and often so you can control changes and roll back safely."
      link="/blog/2025/12/10/stop-ai-agent-unwanted-changes"
    />
    <Card
      title="The AI Skeptic's Guide to Context Windows"
      description="Why do AI agents forget? Learn how context windows, tokens, and biorouter help you manage memory and long conversations."
      link="/blog/2025/08/18/understanding-context-windows"
    />
  </div>
</div>