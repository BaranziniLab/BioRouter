<div align="center">

# BioRouter

_An extensible, open-source AI agent for automating biomedical and engineering tasks_

<p align="center">
  <a href="https://opensource.org/licenses/Apache-2.0">
    <img src="https://img.shields.io/badge/License-Apache_2.0-blue.svg" alt="Apache 2.0 License">
  </a>
</p>

</div>

## Overview

BioRouter is a powerful AI agent capable of automating complex development and research tasks from start to finish. Built on a flexible architecture, BioRouter can execute code, debug failures, orchestrate workflows, and interact with external APIs autonomously.

This project is adapted from [Goose](https://github.com/block/goose) and extended for biomedical research applications while maintaining compatibility with general software engineering tasks.

## Key Features

- **Multi-Model Support**: Works with any LLM provider (OpenAI, Anthropic, Ollama, OpenRouter, etc.)
- **Extensible Architecture**: Plugin system via MCP (Model Context Protocol) servers
- **Multiple Interfaces**: Available as CLI, desktop app, and server
- **Local-First**: Runs entirely on your machine with full control over your data
- **Project Context**: Understands codebases through `.biorouterhints` configuration files
- **Recipe System**: Automated workflows for complex multi-step tasks

## Installation

### Prerequisites

- **Rust** (1.70 or later): [Install Rust](https://rustup.rs/)
- **Node.js** (18 or later): Required for desktop app
- **Cargo**: Comes with Rust installation

### Building from Source

#### 1. Clone the Repository

```bash
git clone https://github.com/BaranziniLab/BioRouter.git
cd BioRouter
```

#### 2. Build the Rust Components

```bash
# Build all components (CLI, server, and libraries)
cargo build --release

# Binaries will be available at:
# - target/release/biorouter (CLI)
# - target/release/biorouterd (Server)
```

#### 3. Build the Desktop App (Optional)

```bash
cd ui/desktop
npm install
npm run make

# The built app will be in ui/desktop/out/
```

## Running BioRouter

### CLI Usage

The BioRouter CLI provides direct command-line access to the AI agent:

```bash
# Run the CLI binary
./target/release/biorouter

# Or install it to your PATH
cargo install --path crates/biorouter-cli

# Start an interactive session
biorouter session start

# Run a single task
biorouter run -t "Analyze the main.rs file and suggest improvements"

# Use a recipe for complex workflows
biorouter run --recipe recipe.yaml
```

#### Configuration

Configure BioRouter through environment variables or a config file at `~/.config/biorouter/config.yaml`:

```yaml
providers:
  openai:
    api_key: YOUR_API_KEY
    model: gpt-4
  anthropic:
    api_key: YOUR_API_KEY
    model: claude-sonnet-4

default_provider: openai
```

Environment variables (highest priority):
```bash
export BIOROUTER_PROVIDER=openai
export BIOROUTER_MODEL=gpt-4
export OPENAI_API_KEY=your_key_here
```

### Server Usage

Run BioRouter as a background service with the server daemon:

```bash
# Start the server
./target/release/biorouterd

# Server runs on http://127.0.0.1:58622 by default

# Configure server settings
export BIOROUTER_API_HOST=0.0.0.0
export BIOROUTER_API_PORT=8080

# Run with custom config
./target/release/biorouterd --config server-config.yaml
```

The server provides a REST API for integrating BioRouter into other applications.

### Desktop App Usage

The desktop application provides a graphical interface for BioRouter:

```bash
cd ui/desktop

# Development mode
npm run start-gui

# Or run the built application from ui/desktop/out/
```

**First-time setup:**
1. Launch the desktop app
2. Select your AI provider (OpenAI, Anthropic, Ollama, etc.)
3. Configure API keys through the settings panel
4. Start chatting with BioRouter!

## Project Structure

```
BioRouter/
├── crates/
│   ├── biorouter/           # Core agent library
│   ├── biorouter-cli/       # CLI application
│   ├── biorouter-server/    # Server daemon
│   ├── biorouter-mcp/       # MCP protocol implementation
│   ├── biorouter-acp/       # ACP protocol support
│   ├── biorouter-bench/     # Benchmarking tools
│   └── biorouter-test/      # Integration tests
├── ui/desktop/              # Electron desktop application
│   └── src/images/          # App icons and assets
│       ├── icon.svg         # Main app icon (source)
│       ├── glyph.svg        # Icon glyph variant
│       └── prepare.sh       # Icon generation script
├── documentation/           # Documentation website
├── examples/                # Example recipes and configurations
├── scripts/                 # Build and utility scripts
└── recipe-scanner/          # Recipe security scanner
```

### App Icon

The BioRouter desktop app uses a router-inspired icon design located in [ui/desktop/src/images/](ui/desktop/src/images/):

- [icon.svg](ui/desktop/src/images/icon.svg) - Main application icon source file
- [glyph.svg](ui/desktop/src/images/glyph.svg) - Glyph variant of the icon

The icon features a network router design with interconnected circles and nodes in dark gray (#36393d) on a transparent background, representing BioRouter's role in routing requests between AI models and MCP servers.

To regenerate platform-specific icon formats (PNG, ICNS) after modifying the SVG:

```bash
cd ui/desktop/src/images
bash prepare.sh
```

## Configuration Files

### `.biorouterhints`

Add project-specific context to improve BioRouter's understanding:

```
# .biorouterhints
This is a Rust project using the Actix web framework.
The main API endpoints are in src/routes/
Database models are in src/models/
Use `cargo test` to run tests.
```

### Recipe Files

Define complex workflows with YAML recipe files:

```yaml
# recipe.yaml
name: Code Review Workflow
description: Comprehensive code review with testing

steps:
  - name: Analyze Code
    prompt: Review all changed files for code quality issues

  - name: Run Tests
    prompt: Execute the test suite and report any failures

  - name: Generate Report
    prompt: Create a markdown summary of findings
```

## Development

### Running Tests

```bash
# Run all Rust tests
cargo test

# Run specific crate tests
cargo test -p biorouter

# Run desktop app tests
cd ui/desktop
npm test
```

### Building Documentation

```bash
cd documentation
npm install
npm run build
```

### Code Style

The project uses:
- `rustfmt` for Rust code formatting
- `clippy` for Rust linting
- `prettier` for TypeScript/JavaScript formatting

```bash
# Format Rust code
cargo fmt --all

# Run Clippy
cargo clippy --all-targets --all-features

# Format UI code
cd ui/desktop
npm run format
```

## Extensions and MCP Servers

BioRouter supports extensions through the Model Context Protocol (MCP). Create custom tools and integrations:

```json
{
  "mcpServers": {
    "biomedical-tools": {
      "command": "python",
      "args": ["-m", "biomedical_mcp_server"],
      "env": {
        "NCBI_API_KEY": "your_key"
      }
    }
  }
}
```

## Security

### Recipe Security Scanner

BioRouter includes a security scanner for validating recipes:

```bash
cd recipe-scanner
./scan-recipe.sh /path/to/recipe.yaml
```

For security vulnerabilities, please email: wanjun.gu@ucsf.edu

## Troubleshooting

### Common Issues

**Issue**: `cargo build` fails with linking errors
- **Solution**: Ensure you have the latest Rust toolchain: `rustup update`

**Issue**: Desktop app won't start
- **Solution**: Clear the electron cache: `rm -rf ~/Library/Application\ Support/BioRouter`

**Issue**: API key not recognized
- **Solution**: Check environment variables: `echo $OPENAI_API_KEY` or verify `~/.config/biorouter/config.yaml`

**Issue**: Server won't bind to port
- **Solution**: Check if port 58622 is already in use: `lsof -i :58622`

## License

This project is licensed under the Apache License 2.0 - see the [LICENSE](LICENSE) file for details.

## Acknowledgments

BioRouter is adapted from [Goose](https://github.com/block/goose), an open-source AI agent framework. We are grateful to the Block team and the open-source community for their foundational work.

### Core Maintainer

**Wanjun Gu**
- Email: wanjun.gu@ucsf.edu
- GitHub: [@Broccolito](https://github.com/Broccolito)

### Institutional Affiliation

[Baranzini Lab](https://baranzinilab.ucsf.edu/)
Department of Neurology
UCSF Bakar Computational Health Sciences Institute
University of California, San Francisco

## Contributing

We welcome contributions! Please see [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

### Development Workflow

1. Fork the repository
2. Create a feature branch: `git checkout -b feature/your-feature`
3. Make your changes and add tests
4. Run tests: `cargo test && cd ui/desktop && npm test`
5. Commit your changes: `git commit -m "Add your feature"`
6. Push to your fork: `git push origin feature/your-feature`
7. Open a Pull Request

## Citation

If you use BioRouter in your research, please cite:

```bibtex
@software{biorouter2025,
  title = {BioRouter: An Extensible AI Agent for Biomedical Research},
  author = {Gu, Wanjun and Baranzini, Sergio E.},
  year = {2025},
  url = {https://github.com/BaranziniLab/BioRouter},
  note = {Adapted from Goose: https://github.com/block/goose}
}
```

## Contact

For questions, issues, or collaboration inquiries:
- **Email**: wanjun.gu@ucsf.edu
- **GitHub Issues**: [BaranziniLab/BioRouter/issues](https://github.com/BaranziniLab/BioRouter/issues)
- **Lab Website**: [baranzinilab.ucsf.edu](https://baranzinilab.ucsf.edu/)

</div>
