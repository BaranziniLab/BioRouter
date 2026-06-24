//! Build pipeline for Agent Drafter apps.
//!
//! Apps are authored in TypeScript (`src/main.ts` importing `src/sdk.ts`) and
//! bundled into a single browser-runnable `dist/app.js` with **esbuild**. esbuild
//! is located via (in order): `$BIOROUTER_ESBUILD_BIN`, the desktop app's bundled
//! `node_modules/.bin/esbuild` (dev tree), `esbuild` on `PATH`, then `npx esbuild`.
//!
//! When no esbuild is available (e.g. a headless CLI install with no Node), a
//! best-effort vendored type-stripper concatenates the sources into a runnable
//! bundle so simple apps still work. The stripper is intentionally conservative;
//! complex TypeScript should be built where esbuild is present.

use std::path::{Path, PathBuf};
use std::process::Command;

// ---------------------------------------------------------------------------
// Build-time guardrail harness
// ---------------------------------------------------------------------------
//
// Rather than only handing the model UI templates, we wrap app authoring in a
// harness that *validates whatever the model generates* and feeds actionable
// findings back so it can self-correct. The harness enforces three things on
// the LLM's free-form output: (1) it reaches the BioRouter backend through the
// App SDK / agent protocol, (2) it is self-contained (no external/CDN assets
// that break portability), and (3) it stays aesthetically aligned with the
// BioRouter design system (uses `br-*` classes + tokens, not ad-hoc CSS).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LintLevel {
    Error,
    Warn,
}

#[derive(Debug, Clone)]
pub struct LintFinding {
    pub level: LintLevel,
    pub msg: String,
}

/// Validate a built app's authored files against the harness guardrails.
/// Returns findings (Errors block readiness; Warns are nudges). Pure string
/// analysis — safe to run on every build/preview.
pub fn lint_app(project_dir: &Path) -> Vec<LintFinding> {
    let mut out = Vec::new();
    let mut err = |m: &str| {
        out.push(LintFinding {
            level: LintLevel::Error,
            msg: m.to_string(),
        })
    };
    let index = std::fs::read_to_string(project_dir.join("index.html")).unwrap_or_default();
    let main = std::fs::read_to_string(project_dir.join("src/main.ts")).unwrap_or_default();
    let il = index.to_lowercase();

    // (2) Self-contained — no external/CDN assets.
    if il.contains("src=\"http") || il.contains("src='http") {
        err("index.html loads an external <script src=\"http…\">. Apps must be self-contained — remove it and use the BioRouter App SDK instead.");
    }
    if il.contains("<link") && (il.contains("href=\"http") || il.contains("href='http")) {
        err("index.html links an external stylesheet. Remove it; the BioRouter design system is injected automatically.");
    }
    for cdn in ["cdn.", "unpkg.com", "jsdelivr", "googleapis", "cdnjs"] {
        if il.contains(cdn) {
            err(&format!(
                "index.html references a CDN ('{cdn}'). Remove external assets — exported apps must run offline."
            ));
            break;
        }
    }

    // (1) Backend wiring through the App SDK / agent protocol.
    if !main.contains("./sdk") {
        err("src/main.ts must `import { createApp } from \"./sdk\"` — that's how the app reaches the BioRouter backend.");
    }
    for line in main.lines() {
        let t = line.trim_start();
        if t.starts_with("import ") && !t.contains("\"./") && !t.contains("'./") {
            err(&format!(
                "src/main.ts has a non-local import — only import from \"./sdk\": {}",
                t.trim()
            ));
        }
    }
    let calls_agent = main.contains("br.run")
        || main.contains(".prompt(")
        || main.contains(".ask(")
        || main.contains("autoChat");
    if !calls_agent {
        out.push(LintFinding {
            level: LintLevel::Warn,
            msg: "src/main.ts never calls the agent (br.run / br.prompt / br.ask) and doesn't enable autoChat. Wire a control to br.run(prompt, \"#out\").".into(),
        });
    }

    // (1c) Wiring: if the page has interactive controls but main.ts wires no
    // events (and isn't an auto-chat app), the UI is inert.
    let has_controls = il.contains("<button")
        || il.contains("<select")
        || il.contains("type=\"range\"")
        || il.contains("br-chip")
        || il.contains("br-region")
        || il.contains("br-dragitem")
        || il.contains("br-dropzone")
        || il.contains("br-tab");
    if has_controls
        && !main.contains("addEventListener")
        && !main.contains("autoChat")
        && !main.contains("onclick")
    {
        out.push(LintFinding {
            level: LintLevel::Warn,
            msg: "index.html has interactive controls but src/main.ts wires no events (addEventListener). The controls won't do anything — wire them to br.run(...).".into(),
        });
    }

    // (1b) Element-id consistency: every id main.ts looks up must exist in
    // index.html. A mismatch is a common LLM bug — getElementById returns null
    // and the app crashes on the first addEventListener.
    let referenced = referenced_ids(&main);
    for rid in &referenced {
        let present =
            index.contains(&format!("id=\"{rid}\"")) || index.contains(&format!("id='{rid}'"));
        if !present {
            out.push(LintFinding {
                level: LintLevel::Error,
                msg: format!(
                    "src/main.ts references element id '#{rid}' that is not in index.html — getElementById would return null and crash. Add the element or fix the id."
                ),
            });
        }
    }

    // (3) Aesthetic alignment with the design system.
    let mut warn = |m: &str| {
        out.push(LintFinding {
            level: LintLevel::Warn,
            msg: m.to_string(),
        })
    };
    if il.contains("<style") {
        warn("index.html contains a <style> block — prefer the design-system classes/CSS variables over custom CSS for a native look.");
    }
    if il.contains("color:#") || il.contains("color: #") || il.contains("background:#") {
        warn("index.html uses raw hex colors — use var(--br-text)/var(--br-accent)/… tokens so the app matches BioRouter's theme.");
    }
    if !il.contains("br-") {
        warn("index.html uses no BioRouter design-system classes (br-*). The UI will look off-theme; compose with br-card/br-btn/br-select/etc.");
    }
    if !il.contains("br-output") && !il.contains("data-br-chat") {
        warn("No result surface found. Add a <div class=\"br-output\" id=\"out\"></div> (target for br.run) or a [data-br-chat] panel.");
    }
    out
}

/// Element ids that `main.ts` looks up: `getElementById("x")` and CSS-id
/// selectors `"#x"` / `'#x'` (covers querySelector and `br.run(_, "#x")`).
fn referenced_ids(main: &str) -> Vec<String> {
    use std::collections::BTreeSet;
    let mut ids: BTreeSet<String> = BTreeSet::new();
    if let Ok(re) = regex::Regex::new(r#"getElementById\(\s*["']([A-Za-z0-9_-]+)["']"#) {
        for c in re.captures_iter(main) {
            ids.insert(c[1].to_string());
        }
    }
    if let Ok(re) = regex::Regex::new(r#"["']#([A-Za-z0-9_-]+)["']"#) {
        for c in re.captures_iter(main) {
            ids.insert(c[1].to_string());
        }
    }
    ids.into_iter().collect()
}

/// Format findings for inclusion in a tool result.
pub fn format_lint(findings: &[LintFinding]) -> String {
    if findings.is_empty() {
        return "Harness: ✓ passes all guardrails (SDK-wired, self-contained, on-theme).".into();
    }
    let mut s = String::from("Harness findings:\n");
    for f in findings {
        let tag = match f.level {
            LintLevel::Error => "✗ ERROR",
            LintLevel::Warn => "• warn",
        };
        s.push_str(&format!("  {tag}: {}\n", f.msg));
    }
    if findings.iter().any(|f| f.level == LintLevel::Error) {
        s.push_str("Fix the ERRORs (they break backend access or portability), then rebuild.");
    }
    s
}

/// Outcome of a build.
#[derive(Debug, Clone)]
pub struct BuildReport {
    pub ok: bool,
    /// "esbuild" or "fallback".
    pub used: String,
    /// Combined stdout/stderr (or fallback notes).
    pub log: String,
}

/// Locate an esbuild executable. Returns `(program, leading_args)` so the caller
/// can support both a direct binary and `npx esbuild`.
fn find_esbuild() -> Option<(String, Vec<String>)> {
    if let Ok(bin) = std::env::var("BIOROUTER_ESBUILD_BIN") {
        if !bin.trim().is_empty() && Path::new(&bin).exists() {
            return Some((bin, vec![]));
        }
    }
    // Dev tree: ui/desktop/node_modules/.bin/esbuild, discovered relative to CWD
    // and a couple of ancestors (tests/CLI may run from a subdir).
    if let Ok(cwd) = std::env::current_dir() {
        let mut dir: Option<&Path> = Some(cwd.as_path());
        for _ in 0..6 {
            let Some(d) = dir else { break };
            let cand = d.join("ui/desktop/node_modules/.bin/esbuild");
            if cand.exists() {
                return Some((cand.to_string_lossy().to_string(), vec![]));
            }
            dir = d.parent();
        }
    }
    if which("esbuild") {
        return Some(("esbuild".to_string(), vec![]));
    }
    if which("npx") {
        return Some(("npx".to_string(), vec!["--yes".into(), "esbuild".into()]));
    }
    None
}

fn which(prog: &str) -> bool {
    let Ok(path) = std::env::var("PATH") else {
        return false;
    };
    for dir in std::env::split_paths(&path) {
        let cand = dir.join(prog);
        if cand.is_file() {
            return true;
        }
        #[cfg(windows)]
        {
            if dir.join(format!("{prog}.cmd")).is_file()
                || dir.join(format!("{prog}.exe")).is_file()
            {
                return true;
            }
        }
    }
    false
}

/// Build an app project rooted at `project_dir`. Expects `src/main.ts`; writes
/// `dist/app.js`. Returns a report (never errors on a *build* failure — inspect
/// `report.ok`); only returns `Err` for filesystem problems.
pub fn build_app(project_dir: &Path) -> std::io::Result<BuildReport> {
    let entry = project_dir.join("src/main.ts");
    if !entry.exists() {
        return Ok(BuildReport {
            ok: false,
            used: "none".into(),
            log: "src/main.ts not found — nothing to build".into(),
        });
    }
    let dist = project_dir.join("dist");
    std::fs::create_dir_all(&dist)?;
    let out = dist.join("app.js");

    if let Some((program, lead)) = find_esbuild() {
        match run_esbuild(&program, &lead, &entry, &out) {
            Ok(report) if report.ok => return Ok(report),
            Ok(report) => {
                // esbuild ran but failed (syntax error etc.) — surface it rather
                // than silently masking with the weaker fallback.
                return Ok(report);
            }
            Err(e) => {
                // esbuild could not be spawned; fall through to the stripper.
                let mut report = fallback_bundle(project_dir, &out)?;
                report.log = format!("esbuild spawn failed ({e}); used fallback.\n{}", report.log);
                return Ok(report);
            }
        }
    }

    fallback_bundle(project_dir, &out)
}

fn run_esbuild(
    program: &str,
    lead: &[String],
    entry: &Path,
    out: &Path,
) -> std::io::Result<BuildReport> {
    let mut cmd = Command::new(program);
    cmd.args(lead);
    cmd.arg(entry);
    cmd.arg("--bundle");
    cmd.arg("--format=iife");
    cmd.arg("--target=es2018");
    cmd.arg("--loader:.ts=ts");
    cmd.arg(format!("--outfile={}", out.display()));
    cmd.arg("--log-level=warning");
    let output = cmd.output()?;
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(BuildReport {
        ok: output.status.success() && out.exists(),
        used: "esbuild".into(),
        log,
    })
}

/// Best-effort transpile without esbuild: concatenate `sdk.ts` + `main.ts`,
/// drop module syntax, and strip the common TypeScript type constructs. Wrapped
/// in an IIFE. Good enough for the generated starter apps; not a real compiler.
fn fallback_bundle(project_dir: &Path, out: &Path) -> std::io::Result<BuildReport> {
    let src = project_dir.join("src");
    let sdk = std::fs::read_to_string(src.join("sdk.ts")).unwrap_or_default();
    let main = std::fs::read_to_string(src.join("main.ts"))?;
    let mut body = String::new();
    body.push_str(&strip_module_syntax(&sdk));
    body.push('\n');
    body.push_str(&strip_module_syntax(&main));
    let js = format!(
        "/* Agent Drafter fallback bundle (no esbuild present). */\n(function(){{\n{body}\n}})();\n"
    );
    std::fs::write(out, js)?;
    Ok(BuildReport {
        ok: true,
        used: "fallback".into(),
        log: "Bundled with the vendored type-stripper (esbuild not found). \
              For full TypeScript support install esbuild or build in the desktop app."
            .into(),
    })
}

/// Remove `import`/`export` statements, `interface`/`type` declarations, and the
/// most common inline type annotations. Conservative and line-oriented.
fn strip_module_syntax(ts: &str) -> String {
    let mut out = Vec::new();
    let mut in_type_block = false;
    let mut depth = 0i32;
    for line in ts.lines() {
        let trimmed = line.trim_start();
        // Drop multi-line interface / `type X = { … }` blocks.
        if in_type_block {
            depth += count(line, '{') - count(line, '}');
            if depth <= 0 {
                in_type_block = false;
            }
            continue;
        }
        if trimmed.starts_with("interface ")
            || trimmed.starts_with("export interface ")
            || trimmed.starts_with("declare ")
            || trimmed.starts_with("export type ")
            || trimmed.starts_with("type ")
        {
            if line.contains('{') && count(line, '{') > count(line, '}') {
                in_type_block = true;
                depth = count(line, '{') - count(line, '}');
            }
            continue;
        }
        // Drop import lines entirely (single-file bundle).
        if trimmed.starts_with("import ") {
            continue;
        }
        // Strip a leading `export ` keyword but keep the declaration.
        let mut l = line.to_string();
        if let Some(rest) = l.trim_start().strip_prefix("export ") {
            let indent = &l[..l.len() - l.trim_start().len()];
            l = format!("{indent}{rest}");
        }
        out.push(strip_inline_types(&l));
    }
    out.join("\n")
}

fn count(s: &str, c: char) -> i32 {
    s.chars().filter(|&x| x == c).count() as i32
}

/// Strip the common inline annotations: `: Type` after params/vars and generic
/// `<…>` after identifiers. Heuristic; leaves the runtime semantics intact.
fn strip_inline_types(line: &str) -> String {
    // Remove ` as Type` casts.
    let mut s = regex_lite_replace_as(line);
    // Remove parameter / variable annotations `name: Type` → `name`.
    s = strip_colon_types(&s);
    s
}

fn regex_lite_replace_as(line: &str) -> String {
    // Replace " as Something" up to a delimiter. Simple scan.
    let mut result = String::with_capacity(line.len());
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if line[i..].starts_with(" as ") {
            // skip " as " and the following type token(s)
            i += 4;
            while i < bytes.len() {
                let c = bytes[i] as char;
                if c == ')' || c == ';' || c == ',' || c == '}' || c == ']' {
                    break;
                }
                i += 1;
            }
        } else {
            result.push(bytes[i] as char);
            i += 1;
        }
    }
    result
}

fn strip_colon_types(line: &str) -> String {
    // Only strip when a `:` is followed by a space + capitalized/primitive type
    // and not inside a string. Very conservative to avoid mangling object/CSS.
    // This handles `(x: Type)`, `let y: Type =`, `): RetType {`.
    let mut out = String::with_capacity(line.len());
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    let mut in_str: Option<char> = None;
    while i < chars.len() {
        let c = chars[i];
        if let Some(q) = in_str {
            out.push(c);
            if c == q && (i == 0 || chars[i - 1] != '\\') {
                in_str = None;
            }
            i += 1;
            continue;
        }
        if c == '"' || c == '\'' || c == '`' {
            in_str = Some(c);
            out.push(c);
            i += 1;
            continue;
        }
        if c == ':' {
            // look ahead: optional space, then a type-ish token
            let mut j = i + 1;
            while j < chars.len() && chars[j] == ' ' {
                j += 1;
            }
            let starts_type = j < chars.len()
                && (chars[j].is_ascii_uppercase()
                    || matches!(
                        peek_word(&chars, j).as_str(),
                        "string" | "number" | "boolean" | "void" | "any" | "null" | "undefined"
                    ));
            // Only strip in a clearly typed position (preceded by an identifier
            // char or `)`), not an object literal `{ key: value }`.
            let prev = if i > 0 { chars[i - 1] } else { ' ' };
            let typed_position = prev.is_alphanumeric() || prev == '_' || prev == ')';
            if starts_type && typed_position {
                // consume the type expression until a stopper
                i = j;
                let mut ang = 0i32;
                while i < chars.len() {
                    let d = chars[i];
                    if d == '<' {
                        ang += 1;
                    } else if d == '>' {
                        ang -= 1;
                    } else if ang == 0
                        && matches!(d, '=' | ',' | ')' | ';' | '{' | '|' | '&' | '\n')
                    {
                        break;
                    }
                    i += 1;
                }
                out.push(' ');
                continue;
            }
        }
        out.push(c);
        i += 1;
    }
    out
}

fn peek_word(chars: &[char], start: usize) -> String {
    let mut s = String::new();
    let mut i = start;
    while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
        s.push(chars[i]);
        i += 1;
    }
    s
}

/// Default project source files written for a fresh agentic app.
pub fn default_sources() -> Vec<(PathBuf, String)> {
    vec![
        (
            PathBuf::from("src/sdk.ts"),
            include_str!("templates/sdk.ts").to_string(),
        ),
        (
            PathBuf::from("src/main.ts"),
            include_str!("templates/main.ts").to_string(),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn strip_module_syntax_removes_imports_and_exports() {
        let ts = "import { x } from \"./sdk\";\nexport function f() { return 1; }\ninterface I { a: number }\nconst y = 2;";
        let out = strip_module_syntax(ts);
        assert!(!out.contains("import"));
        assert!(!out.contains("interface"));
        assert!(out.contains("function f()"));
        assert!(out.contains("const y = 2"));
    }

    #[test]
    fn strip_colon_types_keeps_object_literals() {
        let s = strip_colon_types("const o = { key: value, n: 2 };");
        // object literal colons must be preserved
        assert!(s.contains("key: value") || s.contains("key:value"));
    }

    #[test]
    fn strip_colon_types_removes_param_annotations() {
        let s = strip_colon_types("function f(a: string, b: number) {");
        assert!(!s.contains(": string"));
        assert!(!s.contains(": number"));
        assert!(s.contains("function f(a"));
    }

    #[test]
    fn fallback_bundle_produces_runnable_iife() {
        let dir = TempDir::new().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("sdk.ts"), "export function hi() { return 1; }").unwrap();
        std::fs::write(src.join("main.ts"), "import { hi } from \"./sdk\";\nhi();").unwrap();
        let out = dir.path().join("dist/app.js");
        std::fs::create_dir_all(dir.path().join("dist")).unwrap();
        let report = fallback_bundle(dir.path(), &out).unwrap();
        assert!(report.ok);
        let js = std::fs::read_to_string(&out).unwrap();
        assert!(js.contains("function hi()"));
        assert!(js.trim_start().starts_with("/*"));
        assert!(js.contains("(function()"));
    }

    #[test]
    fn build_app_without_entry_is_noop() {
        let dir = TempDir::new().unwrap();
        let report = build_app(dir.path()).unwrap();
        assert!(!report.ok);
    }

    #[test]
    fn build_app_bundles_with_available_toolchain() {
        // Uses esbuild if present, else the fallback — either way dist/app.js
        // must exist and contain the SDK code.
        let dir = TempDir::new().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("sdk.ts"), include_str!("templates/sdk.ts")).unwrap();
        std::fs::write(src.join("main.ts"), include_str!("templates/main.ts")).unwrap();
        let report = build_app(dir.path()).unwrap();
        assert!(report.ok, "build failed: {}", report.log);
        let js = std::fs::read_to_string(dir.path().join("dist/app.js")).unwrap();
        assert!(js.contains("BioRouter") || js.contains("createApp"));
    }
}
