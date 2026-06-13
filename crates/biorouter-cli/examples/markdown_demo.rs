//! Render a kitchen-sink markdown document with the CLI's terminal renderer.
//!
//! Used to eyeball rendering in a real PTY (e.g. under tmux):
//! `cargo run -p biorouter-cli --example markdown_demo`

use biorouter_cli::session::markdown::render_markdown;
use biorouter_cli::session::output::Theme;

const DOC: &str = r#"# Biorouter Render Test

Intro paragraph with **bold**, *italic*, ~~strikethrough~~, `inline code`,
and a [link to the docs](https://biorouter.ucsf.edu/docs).

## Table

| Provider | Models | Streaming |
|:---------|:------:|----------:|
| Anthropic | 12 | yes |
| OpenAI | 38 | yes |
| Ollama (local) | 5 | no |

## Lists

1. First ordered item
2. Second with `code`
   - nested bullet
     - deeper nested bullet
3. Third

- [x] install the CLI
- [ ] configure a provider

## Quotes

> A plain quote that should carry the bar prefix even when the
> line wraps around the terminal width gracefully.

> [!WARNING]
> Callouts render with a label.

## Code

```rust
fn main() {
    println!("hello from biorouter");
}
```

```python
def greet(name: str) -> str:
    return f"hi {name}"
```

Math inline: $e^{i\pi} + 1 = 0$

---

Done. Final paragraph after a horizontal rule.
"#;

fn main() {
    let width = console::Term::stdout()
        .size_checked()
        .map(|(_h, w)| (w as usize).clamp(40, 100))
        .unwrap_or(80);
    print!("{}", render_markdown(DOC, Theme::Dark, width));
}
