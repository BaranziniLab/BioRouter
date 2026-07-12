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

use crate::agent_drafter::store::Manifest;

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
#[allow(clippy::too_many_lines)]
pub fn lint_app(project_dir: &Path) -> Vec<LintFinding> {
    let mut out: Vec<LintFinding> = Vec::new();
    // Free functions rather than closures: two capturing closures over `out`
    // would be a double mutable borrow.
    fn error(out: &mut Vec<LintFinding>, m: &str) {
        out.push(LintFinding {
            level: LintLevel::Error,
            msg: m.to_string(),
        });
    }
    fn warning(out: &mut Vec<LintFinding>, m: &str) {
        out.push(LintFinding {
            level: LintLevel::Warn,
            msg: m.to_string(),
        });
    }
    let index = std::fs::read_to_string(project_dir.join("index.html")).unwrap_or_default();
    let main = std::fs::read_to_string(project_dir.join("src/main.ts")).unwrap_or_default();
    let il = index.to_lowercase();
    // The manifest carries the agent's system prompt, which is where an app says
    // *how* the agent should drive the UI — so the visual/UI checks below have to
    // see it, not just the markup.
    let manifest: Option<Manifest> = std::fs::read_to_string(project_dir.join("manifest.json"))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok());
    let system_prompt = manifest
        .as_ref()
        .and_then(|m| m.agent.as_ref())
        .map(|a| a.system_prompt.to_lowercase())
        .unwrap_or_default();
    let ui_enabled = manifest
        .as_ref()
        .and_then(|m| m.agent.as_ref())
        .map(|a| a.capabilities.ui.enabled)
        .unwrap_or(true);

    // (2) Self-contained — no external/CDN assets.
    if il.contains("src=\"http") || il.contains("src='http") {
        error(&mut out, "index.html loads an external <script src=\"http…\">. Apps must be self-contained — remove it and use the BioRouter App SDK instead.");
    }
    if il.contains("<link") && (il.contains("href=\"http") || il.contains("href='http")) {
        error(&mut out, "index.html links an external stylesheet. Remove it; the BioRouter design system is injected automatically.");
    }
    for cdn in ["cdn.", "unpkg.com", "jsdelivr", "googleapis", "cdnjs"] {
        if il.contains(cdn) {
            error(&mut out, &format!(
                "index.html references a CDN ('{cdn}'). Remove external assets — exported apps must run offline."
            ));
            break;
        }
    }

    // (3) Theme-adaptive text: a hardcoded text color (a hex/rgb literal in a
    // `color:` that isn't a `var(--br-*)`) won't adapt when the app is themed,
    // so it goes invisible (dark-on-dark / light-on-light). Warn so the model
    // switches to the design tokens. Heuristic: look for `color:#…`/`color:rgb`
    // that isn't immediately a `var(`.
    let hardcoded_text_color = {
        let hay = index.replace(' ', "");
        let hay_l = hay.to_lowercase();
        let mut hit = false;
        for pat in ["color:#", "color:rgb", "color:hsl"] {
            for (at, _) in hay_l.match_indices(pat) {
                // skip `background-color`/`border-color` — only bare `color:`
                let preceded_by_dash = at > 0 && hay_l.as_bytes()[at - 1] == b'-';
                if !preceded_by_dash {
                    hit = true;
                    break;
                }
            }
            if hit {
                break;
            }
        }
        hit
    };
    if hardcoded_text_color {
        warning(&mut out, "index.html hardcodes a text color (color:#… / color:rgb(…)). It won't adapt to the app theme and can go invisible (dark-on-dark). Use the design tokens: color: var(--br-text) for body text, var(--br-text-muted) for secondary text.");
    }
    // A *surface* token used as a text color (color:var(--br-muted|bg|surface|
    // medium|strong)) is near-background in both themes → invisible. The intended
    // text tokens are --br-text / --br-text-muted.
    let surface_as_text = {
        let hay = index.replace(' ', "").to_lowercase();
        ["color:var(--br-muted)", "color:var(--br-bg)", "color:var(--br-surface)",
         "color:var(--br-medium)", "color:var(--br-strong)"]
            .iter()
            .any(|p| hay.contains(p))
    };
    if surface_as_text {
        warning(&mut out, "index.html uses a SURFACE token as a text color (e.g. color: var(--br-muted)). Surface tokens are near-background in both themes, so the text is invisible. Use color: var(--br-text) or var(--br-text-muted).");
    }

    // (1) Backend wiring through the App SDK / agent protocol.
    if !main.contains("./sdk") {
        error(&mut out, "src/main.ts must `import { createApp } from \"./sdk\"` — that's how the app reaches the BioRouter backend.");
    }
    for line in main.lines() {
        let t = line.trim_start();
        if t.starts_with("import ") && !t.contains("\"./") && !t.contains("'./") {
            error(
                &mut out,
                &format!(
                    "src/main.ts has a non-local import — only import from \"./sdk\": {}",
                    t.trim()
                ),
            );
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
    let has_progress_surface = main.contains("br.run")
        || main.contains("autoChat: true")
        || main.contains("createApp();")
        || main.contains("mountTimeline")
        || il.contains("data-br-progress")
        || il.contains("br-run-status")
        || il.contains("data-br-chat");
    if calls_agent && !has_progress_surface {
        out.push(LintFinding {
            level: LintLevel::Error,
            msg: "Long-running agent work must expose visible step progress. Use br.run(...), the default [data-br-chat] panel, mountTimeline(br, \"#progress\"), or an equivalent br-run-status/debug surface.".into(),
        });
    }

    // (1c) Wiring: if the page has interactive controls but main.ts wires no
    // events (and isn't an auto-chat app), the UI is inert.
    //
    // `br-region` is the clickable map cell. `data-br-region` is an agent render
    // target and is NOT a control — match the class without swallowing the
    // attribute, or every app that declares a render target gets a false warning.
    let has_map_region = il.matches("br-region").count() > il.matches("data-br-region").count();
    let has_controls = il.contains("<button")
        || il.contains("<select")
        || il.contains("type=\"range\"")
        || il.contains("br-chip")
        || has_map_region
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
    if il.contains("<style") {
        warning(&mut out, "index.html contains a <style> block — prefer the design-system classes/CSS variables over custom CSS for a native look.");
    }
    if il.contains("color:#") || il.contains("color: #") || il.contains("background:#") {
        warning(&mut out, "index.html uses raw hex colors — use var(--br-text)/var(--br-accent)/… tokens so the app matches BioRouter's theme.");
    }
    if !il.contains("br-") {
        warning(&mut out, "index.html uses no BioRouter design-system classes (br-*). The UI will look off-theme; compose with br-card/br-btn/br-select/etc.");
    }
    if !il.contains("br-output") && !il.contains("data-br-chat") {
        warning(&mut out, "No result surface found. Add a <div class=\"br-output\" id=\"out\"></div> (target for br.run) or a [data-br-chat] panel.");
    }
    let authored = format!("{}\n{}\n{}", il, main.to_lowercase(), system_prompt);
    let contains_word = |word: &str| {
        regex::Regex::new(&format!(r"\b{}\b", regex::escape(word)))
            .map(|re| re.is_match(&authored))
            .unwrap_or(false)
    };
    let visual_claim = [
        "visual",
        "visualize",
        "visualizes",
        "visualized",
        "visualizing",
        "visualization",
        "visualizations",
        "chart",
        "charts",
        "diagram",
        "diagrams",
        "figure",
        "figures",
    ]
    .iter()
    .any(|word| contains_word(word))
        || authored.contains("graph visual")
        || authored.contains("visual graph")
        || authored.contains("network visual")
        || authored.contains("knowledge map");
    let asks_for_rendered_visual = [
        "```chart",
        "```graph",
        "```diagram",
        "```network",
        "\\`\\`\\`chart",
        "\\`\\`\\`graph",
        "\\`\\`\\`diagram",
        "\\`\\`\\`network",
        "renderchart",
        "rendergraph",
        // Agent-driven UI satisfies the visualization contract directly: the
        // agent draws into the page rather than emitting a markdown fence.
        "ui_chart",
        "ui_graph",
        "ui_panel",
    ]
    .iter()
    .any(|needle| authored.contains(needle));
    if calls_agent && visual_claim && !asks_for_rendered_visual {
        warning(&mut out, "This app appears to promise visual output, but its prompt/UI never asks for a rendered visual. Either tell the agent to call ui_chart/ui_graph, or to emit a ```chart / ```graph block, so the result surface shows an actual visualization rather than tool logs or prose.");
    }

    // (5) Agent-driven UI coherence.
    if !ui_enabled {
        if main.contains("br.ui.") {
            error(&mut out, "src/main.ts uses `br.ui` but this app sets capabilities.ui.enabled = false — the agent can never send a UI command, so those handlers are dead. Enable the capability or drop the code.");
        }
        if system_prompt.contains("ui_") {
            warning(&mut out, "The system prompt tells the agent to call ui_* tools, but capabilities.ui.enabled = false so it has none. Enable the capability or rewrite the prompt.");
        }
    } else {
        // Duplicate region names make `ui_render(target=\"@region:x\")` ambiguous —
        // the SDK resolves the first match and silently ignores the rest.
        let names = region_names(&index);
        let mut seen = std::collections::HashSet::new();
        for n in &names {
            if !seen.insert(*n) {
                warning(&mut out, &format!(
                    "index.html declares data-br-region=\"{n}\" more than once. The agent's `@region:{n}` target resolves to the first one only — give each region a unique name."
                ));
            }
        }
    }
    out
}

/// The `data-br-region="…"` names an app declares — the targets the agent may
/// address as `@region:<name>`.
///
/// Only quoted attribute values are recognised. Written with `split`/`split_once`
/// rather than byte slicing: `&rest[1..]` after taking the first *char* panics on
/// `data-br-region=é`, which is exactly the malformed markup a model emits now
/// and then, and it would take the bundler down with it.
fn region_names(index: &str) -> Vec<&str> {
    let mut names = Vec::new();
    for chunk in index.split("data-br-region=").skip(1) {
        let mut chars = chunk.chars();
        let Some(quote) = chars.next() else {
            continue;
        };
        if quote != '"' && quote != '\'' {
            continue; // unquoted attribute value — nothing well-defined to read
        }
        if let Some((name, _)) = chars.as_str().split_once(quote) {
            names.push(name);
        }
    }
    names
}

/// Element ids that `main.ts` looks up: `getElementById("x")` and CSS-id
/// selectors `"#x"` / `'#x'` (covers querySelector and `br.run(_, "#x")`).
fn referenced_ids(main: &str) -> Vec<String> {
    use std::collections::BTreeSet;
    let mut ids: BTreeSet<String> = BTreeSet::new();
    let main = strip_js_comments(main);
    if let Ok(re) = regex::Regex::new(r#"getElementById\(\s*["']([A-Za-z0-9_-]+)["']"#) {
        for c in re.captures_iter(&main) {
            ids.insert(c[1].to_string());
        }
    }
    if let Ok(re) = regex::Regex::new(r#"["']#([A-Za-z0-9_-]+)["']"#) {
        for c in re.captures_iter(&main) {
            ids.insert(c[1].to_string());
        }
    }
    ids.into_iter().collect()
}

fn strip_js_comments(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let mut in_block = false;
    for line in source.lines() {
        let mut rest = line;
        loop {
            if in_block {
                if let Some(end) = rest.find("*/") {
                    let (_, after) = rest.split_at(end + 2);
                    rest = after;
                    in_block = false;
                } else {
                    break;
                }
            } else if let Some(start) = rest.find("/*") {
                let (before, marker_and_after) = rest.split_at(start);
                out.push_str(before);
                let (_, after) = marker_and_after.split_at(2);
                rest = after;
                in_block = true;
            } else {
                let code = rest.split_once("//").map(|(code, _)| code).unwrap_or(rest);
                out.push_str(code);
                break;
            }
        }
        out.push('\n');
    }
    out
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
/// The App SDK an app *should* be bundling — the template compiled into this
/// binary.
pub fn sdk_template() -> &'static str {
    include_str!("templates/sdk.ts")
}

/// A short fingerprint of the current App SDK. Stored on the manifest after a
/// build so a daemon can tell that an app's bundle predates an SDK upgrade and
/// rebuild it, rather than serving a stale runtime that ignores frames the
/// server now sends (this is how agent-driven UI reaches apps built before it
/// existed).
pub fn sdk_fingerprint() -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    sdk_template().hash(&mut h);
    format!("{:x}", h.finish())
}

/// Overwrite the app's vendored `src/sdk.ts` when it differs from the template.
///
/// The SDK is a *provided runtime*, not authored code: `create_app` writes it and
/// nothing is meant to edit it. Refreshing it here means "rebuild the app" is all
/// it takes to pick up SDK fixes, instead of a manual re-copy into every project.
/// Returns whether it was replaced.
fn refresh_sdk(project_dir: &Path) -> bool {
    let path = project_dir.join("src/sdk.ts");
    let current = std::fs::read_to_string(&path).unwrap_or_default();
    if current == sdk_template() {
        return false;
    }
    std::fs::write(&path, sdk_template()).is_ok()
}

pub fn build_app(project_dir: &Path) -> std::io::Result<BuildReport> {
    let entry = project_dir.join("src/main.ts");
    if !entry.exists() {
        return Ok(BuildReport {
            ok: false,
            used: "none".into(),
            log: "src/main.ts not found — nothing to build".into(),
        });
    }
    let refreshed = refresh_sdk(project_dir);
    let dist = project_dir.join("dist");
    std::fs::create_dir_all(&dist)?;
    let out = dist.join("app.js");

    let note = |mut report: BuildReport| {
        if refreshed {
            report.log = format!(
                "Refreshed src/sdk.ts to the current App SDK.\n{}",
                report.log
            );
        }
        report
    };

    if let Some((program, lead)) = find_esbuild() {
        match run_esbuild(&program, &lead, &entry, &out) {
            // esbuild ran: surface its result either way rather than silently
            // masking a syntax error with the weaker fallback.
            Ok(report) => return Ok(note(report)),
            Err(e) => {
                // esbuild could not be spawned; fall through to the stripper.
                let mut report = fallback_bundle(project_dir, &out)?;
                report.log = format!("esbuild spawn failed ({e}); used fallback.\n{}", report.log);
                return Ok(note(report));
            }
        }
    }

    fallback_bundle(project_dir, &out).map(note)
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
    // `None` = not skipping. `Some(true)` = inside a multi-line `type X = …`
    // alias (ends at a `;` once braces are balanced — covers multi-line union
    // types like `type AgentEvent =\n | {…}\n | {…};`). `Some(false)` = inside an
    // interface/declare block (ends when braces balance).
    let mut skipping: Option<bool> = None;
    let mut depth = 0i32;
    for line in ts.lines() {
        let trimmed = line.trim_start();
        if let Some(is_alias) = skipping {
            depth += count(line, '{') - count(line, '}');
            let done = if is_alias {
                depth <= 0 && trimmed.trim_end().ends_with(';')
            } else {
                depth <= 0
            };
            if done {
                skipping = None;
            }
            continue;
        }
        let is_type_alias = trimmed.starts_with("type ") || trimmed.starts_with("export type ");
        let is_iface = trimmed.starts_with("interface ")
            || trimmed.starts_with("export interface ")
            || trimmed.starts_with("declare ");
        if is_type_alias || is_iface {
            depth = count(line, '{') - count(line, '}');
            let complete = if is_type_alias {
                // Single-line alias: braces balanced AND statement-terminated.
                depth <= 0 && trimmed.trim_end().ends_with(';')
            } else {
                // Single-line interface/declare with balanced braces (rare).
                depth <= 0 && line.contains('}')
            };
            if !complete {
                skipping = Some(is_type_alias);
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
            let (indent, _) = l.split_at(l.len() - l.trim_start().len());
            l = format!("{indent}{rest}");
        }
        // Strip TS-only class-member modifiers (e.g. `private`, `readonly`),
        // which are not valid JS, while preserving indentation. `static` is
        // valid JS and is intentionally kept. Loops to handle combinations like
        // `private readonly foo`.
        loop {
            let ls = l.trim_start();
            let indent_len = l.len() - ls.len();
            let modifier = [
                "private ",
                "public ",
                "protected ",
                "readonly ",
                "abstract ",
                "override ",
            ]
            .iter()
            .find(|kw| ls.starts_with(**kw))
            .map(|kw| kw.len());
            match modifier {
                Some(n) => {
                    let (indent, _) = l.split_at(indent_len);
                    let (_, after_modifier) = ls.split_at(n);
                    l = format!("{indent}{after_modifier}");
                }
                None => break,
            }
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
    // Remove value-position generic type args: `new Promise<void>(` → `new Promise(`.
    s = strip_value_generics(&s);
    // Remove TS non-null assertions: `this.ws!.send(` → `this.ws.send(`.
    s = strip_non_null_assertions(&s);
    s
}

/// Strip TS non-null assertion operators (`expr!`) while leaving logical-not
/// (`!expr`) and inequality (`!=`/`!==`) intact. A non-null assertion is a
/// postfix `!`: it follows an operand char and precedes a member/call/terminator.
fn strip_non_null_assertions(line: &str) -> String {
    let chars: Vec<char> = line.chars().collect();
    let mut out = String::with_capacity(line.len());
    let mut in_str: Option<char> = None;
    let mut i = 0;
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
        if c == '!' {
            let prev = if i > 0 { chars[i - 1] } else { ' ' };
            let next = if i + 1 < chars.len() {
                chars[i + 1]
            } else {
                ' '
            };
            let postfix_operand =
                prev.is_alphanumeric() || prev == '_' || prev == ')' || prev == ']';
            let member_or_end =
                matches!(next, '.' | ')' | ';' | ',' | ']' | '(' | '[') || next.is_whitespace();
            if postfix_operand && member_or_end && next != '=' {
                i += 1; // drop the assertion `!`
                continue;
            }
        }
        out.push(c);
        i += 1;
    }
    out
}

/// Remove value-position generic type arguments that directly precede a call,
/// e.g. `new Promise<void>(...)` or `foo<T>(...)` → `Promise(...)` / `foo(...)`.
/// Conservative: only strips a `<…>` that (1) immediately follows an identifier
/// char, (2) contains only type-ish characters, and (3) is immediately followed
/// by `(` — so comparisons (`a < b`) and bit-shifts (`a << b`) are untouched.
fn strip_value_generics(line: &str) -> String {
    let chars: Vec<char> = line.chars().collect();
    let mut out = String::with_capacity(line.len());
    let mut in_str: Option<char> = None;
    let mut i = 0;
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
        if c == '<' && i > 0 && (chars[i - 1].is_alphanumeric() || chars[i - 1] == '_') {
            let mut j = i + 1;
            let mut depth = 1i32;
            let mut type_only = true;
            while j < chars.len() && depth > 0 {
                let d = chars[j];
                if d == '<' {
                    depth += 1;
                } else if d == '>' {
                    depth -= 1;
                } else if !(d.is_alphanumeric()
                    || matches!(d, '_' | ',' | ' ' | '[' | ']' | '.' | '|' | '&'))
                {
                    type_only = false;
                    break;
                }
                j += 1;
            }
            if type_only && depth == 0 && j < chars.len() && chars[j] == '(' {
                // Drop the `<…>` entirely; resume at the `(`.
                i = j;
                continue;
            }
        }
        out.push(c);
        i += 1;
    }
    out
}

fn regex_lite_replace_as(line: &str) -> String {
    // Replace " as Something" casts up to a delimiter. Char-based scan so it is
    // safe on multibyte UTF-8 (em-dash, box-drawing, ellipsis in comments) —
    // byte indexing here previously panicked on a non-char-boundary slice.
    let chars: Vec<char> = line.chars().collect();
    let mut result = String::with_capacity(line.len());
    let mut i = 0;
    while i < chars.len() {
        if i + 4 <= chars.len()
            && chars[i] == ' '
            && chars[i + 1] == 'a'
            && chars[i + 2] == 's'
            && chars[i + 3] == ' '
        {
            i += 4;
            while i < chars.len() {
                let c = chars[i];
                if c == ')' || c == ';' || c == ',' || c == '}' || c == ']' {
                    break;
                }
                i += 1;
            }
        } else {
            result.push(chars[i]);
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
                        "string"
                            | "number"
                            | "boolean"
                            | "void"
                            | "any"
                            | "null"
                            | "undefined"
                            | "unknown"
                            | "never"
                            | "this"
                            | "object"
                    ));
            // Only strip in a clearly typed position (preceded by an identifier
            // char, `)`, or `?` for optional params), not an object literal
            // `{ key: value }`.
            let prev = if i > 0 { chars[i - 1] } else { ' ' };
            let typed_position =
                prev.is_alphanumeric() || prev == '_' || prev == ')' || prev == '?';
            if starts_type && typed_position {
                // An optional-param/field marker `name?:` — drop the trailing `?`
                // we already emitted (it is TS-only and invalid in plain JS).
                if prev == '?' {
                    out.pop();
                }
                // Consume the whole type expression — unions `|`, intersections
                // `&`, generics `<…>`, tuples `[…]`, function types `(…) =>` —
                // up to a top-level terminator, tracking nesting so commas /
                // parens inside generics or function types don't end it early.
                i = j;
                let (mut ang, mut par, mut brk) = (0i32, 0i32, 0i32);
                while i < chars.len() {
                    let d = chars[i];
                    match d {
                        '<' => ang += 1,
                        '>' if ang > 0 => ang -= 1,
                        '(' => par += 1,
                        ')' if par > 0 => par -= 1,
                        '[' => brk += 1,
                        ']' if brk > 0 => brk -= 1,
                        _ if ang == 0
                            && par == 0
                            && brk == 0
                            && matches!(d, '=' | ',' | ')' | ';' | '{' | '\n') =>
                        {
                            break;
                        }
                        _ => {}
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
        (PathBuf::from("src/sdk.ts"), sdk_template().to_string()),
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
    fn strip_module_syntax_drops_multiline_type_alias_union() {
        // Regression: a multi-line `type X =` union (no `{` on the first line)
        // must be fully removed, not leak its `| { … }` members as statements.
        let ts = "export type E =\n  | { type: \"a\"; x: number }\n  | { type: \"b\"; y: string };\nconst k = 1;";
        let out = strip_module_syntax(ts);
        for l in out.lines() {
            assert!(
                !l.trim_start().starts_with("| "),
                "union member leaked into JS: {l:?}"
            );
        }
        assert!(out.contains("const k = 1"));
    }

    /// The App SDK renders MODEL-authored markdown into `innerHTML` (panels, the
    /// `ui_ask` prompt, chat), and the app is not a sandboxed iframe. So the
    /// markdown escaper must neutralize attribute breakout: escape quotes, and
    /// only ever emit `http(s)` link hrefs stripped of quotes/angles/space. A
    /// regression here is an XSS in the app's own origin, so guard the source.
    #[test]
    fn sdk_markdown_escaper_is_hardened_against_attribute_breakout() {
        let sdk = sdk_template();
        // escapeHtml must cover both quote characters.
        assert!(
            sdk.contains(r#".replace(/"/g, "&quot;")"#),
            "escapeHtml must escape double quotes"
        );
        assert!(
            sdk.contains(r#".replace(/'/g, "&#39;")"#),
            "escapeHtml must escape single quotes"
        );
        // The link rule must stop the URL at whitespace and scrub the href.
        assert!(
            sdk.contains(r#"(https?:[^)\s]+)"#),
            "link URLs must not span whitespace (an injected ` onX=` breaks the attribute)"
        );
        assert!(
            sdk.contains(r#".replace(/["'<>`\s]/g, "")"#),
            "the link href must be stripped of quotes/angles/backtick/space"
        );
    }

    #[test]
    fn regex_lite_replace_as_is_multibyte_safe() {
        // Must not panic on multibyte chars (em-dash, box-drawing, ellipsis)
        // and must preserve them while still stripping ` as T` casts.
        let line = "  // ── header — note … done; const x = y as Foo;";
        let out = regex_lite_replace_as(line);
        assert!(out.contains('─') && out.contains('—') && out.contains('…'));
        assert!(!out.contains(" as Foo"));
    }

    #[test]
    fn fallback_strips_real_sdk_template_into_valid_js() {
        // The actual shipped SDK template must survive the no-esbuild fallback:
        // no panic, no leaked TS (imports / interfaces / union members), and the
        // key runtime symbols survive. If `node` is present, assert it parses.
        let sdk = include_str!("templates/sdk.ts");
        let main = include_str!("templates/main.ts");
        let stripped_sdk = strip_module_syntax(sdk);
        let stripped_main = strip_module_syntax(main);

        for (name, body) in [("sdk.ts", &stripped_sdk), ("main.ts", &stripped_main)] {
            for l in body.lines() {
                let t = l.trim_start();
                assert!(!t.starts_with("import "), "{name}: leaked import: {l:?}");
                assert!(!t.starts_with("| "), "{name}: leaked union member: {l:?}");
                assert!(
                    !t.starts_with("interface ") && !t.starts_with("export "),
                    "{name}: leaked TS decl: {l:?}"
                );
            }
        }
        // Core runtime API must remain.
        assert!(stripped_sdk.contains("function createApp"));
        assert!(stripped_sdk.contains("class BioRouterClient"));
        assert!(stripped_sdk.contains("function renderMarkdown"));
        assert!(stripped_sdk.contains("approve"));

        // Wrap as the fallback bundler does and (optionally) node --check it.
        let js = format!("(function(){{\n{stripped_sdk}\n{stripped_main}\n}})();\n");
        if let Ok(node) = which::which("node") {
            let dir = TempDir::new().unwrap();
            let f = dir.path().join("app.js");
            std::fs::write(&f, &js).unwrap();
            let out = std::process::Command::new(node)
                .arg("--check")
                .arg(&f)
                .output()
                .expect("run node --check");
            assert!(
                out.status.success(),
                "node --check rejected the fallback bundle:\n{}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
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

    #[test]
    fn lint_app_flags_portability_wiring_and_runtime_breakers() {
        let dir = TempDir::new().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(
            dir.path().join("index.html"),
            r#"<html><head><script src="https://cdn.example/app.js"></script></head>
               <body><main class="br-container"><button id="run" class="br-btn">Run</button></main></body></html>"#,
        )
        .unwrap();
        std::fs::write(
            src.join("main.ts"),
            r##"import confetti from "canvas-confetti";
import { createApp } from "./sdk";
const br = createApp({ autoChat: false });
br.run("go", "#missing");
"##,
        )
        .unwrap();

        let findings = lint_app(dir.path());
        let formatted = format_lint(&findings);
        assert!(
            findings.iter().any(|f| f.level == LintLevel::Error),
            "{formatted}"
        );
        assert!(formatted.contains("external <script"), "{formatted}");
        assert!(formatted.contains("non-local import"), "{formatted}");
        assert!(formatted.contains("#missing"), "{formatted}");
    }

    /// H6: a hardcoded text color won't adapt to the theme and can go invisible.
    #[test]
    fn lint_warns_on_hardcoded_text_color_but_not_theme_tokens() {
        let mk = |html: &str| {
            let dir = TempDir::new().unwrap();
            std::fs::create_dir_all(dir.path().join("src")).unwrap();
            std::fs::write(dir.path().join("index.html"), html).unwrap();
            std::fs::write(
                dir.path().join("src/main.ts"),
                "import { createApp } from \"./sdk\";\ncreateApp();\n",
            )
            .unwrap();
            format_lint(&lint_app(dir.path()))
        };
        // Hardcoded text color → warned.
        let bad = mk(r#"<html><body><p style="color:#282217">hi</p></body></html>"#);
        assert!(bad.contains("hardcodes a text color"), "{bad}");
        // rgb() form too.
        let bad2 = mk(r#"<html><body><p style="color: rgb(40,34,23)">hi</p></body></html>"#);
        assert!(bad2.contains("hardcodes a text color"), "{bad2}");
        let unicode = mk(
            r#"<html><body><p>Résumé</p><div style="background-color:#fff;color:#282217">hi</div></body></html>"#,
        );
        assert!(unicode.contains("hardcodes a text color"), "{unicode}");
        // Theme token → NOT warned; and background-color hardcodes are not text.
        let good = mk(
            r#"<html><body><p style="color:var(--br-text)">hi</p>
               <div style="background-color:#fff">x</div></body></html>"#,
        );
        assert!(!good.contains("hardcodes a text color"), "{good}");
        // A SURFACE token used as text color → its own warning.
        let surf = mk(r#"<html><body><p style="color: var(--br-muted)">hi</p></body></html>"#);
        assert!(surf.contains("SURFACE token as a text color"), "{surf}");
        // …but a surface token as a *background* is fine.
        let okbg = mk(r#"<html><body><div style="background: var(--br-muted)">x</div></body></html>"#);
        assert!(!okbg.contains("SURFACE token as a text color"), "{okbg}");
    }

    #[test]
    fn lint_app_requires_visible_progress_for_manual_prompt_loops() {
        let dir = TempDir::new().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(
            dir.path().join("index.html"),
            r#"<html><body><main class="br-container">
               <button id="run" class="br-btn">Run</button>
               <div id="out" class="br-output"></div>
               </main></body></html>"#,
        )
        .unwrap();
        std::fs::write(
            src.join("main.ts"),
            r#"import { createApp } from "./sdk";
const br = createApp({ autoChat: false });
document.getElementById("run")!.addEventListener("click", () => br.prompt("go"));
"#,
        )
        .unwrap();

        let formatted = format_lint(&lint_app(dir.path()));
        assert!(formatted.contains("visible step progress"), "{formatted}");
    }

    #[test]
    fn region_names_reads_quoted_values_and_survives_malformed_markup() {
        assert_eq!(
            region_names(r#"<i data-br-region="a"></i><i data-br-region='b'></i>"#),
            vec!["a", "b"]
        );
        // Unquoted and unterminated values are skipped, not guessed at.
        assert!(region_names("<i data-br-region=a>").is_empty());
        assert!(region_names(r#"<i data-br-region="a>"#).is_empty());
        assert!(region_names("data-br-region=").is_empty());
        // A multibyte char right after `=` used to panic the bundler.
        assert!(region_names("<i data-br-region=é>").is_empty());
        assert_eq!(region_names(r#"<i data-br-region="é—ø">"#), vec!["é—ø"]);
    }

    #[test]
    fn lint_app_warns_when_visual_app_lacks_rendered_visual_contract() {
        let dir = TempDir::new().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(
            dir.path().join("index.html"),
            r#"<html><body><main class="br-container">
               <button id="run" class="br-btn">Build graph</button>
               <div id="out" class="br-output"></div>
               </main></body></html>"#,
        )
        .unwrap();
        std::fs::write(
            src.join("main.ts"),
            r##"import { createApp } from "./sdk";
const br = createApp({ autoChat: false });
document.getElementById("run")!.addEventListener("click", () => br.run("visualize the graph as prose", "#out"));
"##,
        )
        .unwrap();

        let formatted = format_lint(&lint_app(dir.path()));
        assert!(
            formatted.contains("never asks for a rendered visual"),
            "{formatted}"
        );
    }

    /// Write a minimal app, optionally with a manifest, and lint it.
    fn lint_with(index: &str, main: &str, manifest: Option<&str>) -> String {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("index.html"), index).unwrap();
        std::fs::write(dir.path().join("src/main.ts"), main).unwrap();
        if let Some(m) = manifest {
            std::fs::write(dir.path().join("manifest.json"), m).unwrap();
        }
        format_lint(&lint_app(dir.path()))
    }

    fn manifest_json(system_prompt: &str, ui_enabled: bool) -> String {
        format!(
            r#"{{"id":"a","title":"A","description":"","kind":"agentic","entry":"index.html",
                "created_at":0,"updated_at":0,
                "agent":{{"system_prompt":{},"capabilities":{{"ui":{{"enabled":{ui_enabled}}}}}}}}}"#,
            serde_json::to_string(system_prompt).unwrap()
        )
    }

    const CHATTY_INDEX: &str = r#"<html><body><main class="br-container">
        <div class="br-card" data-br-chat></div></main></body></html>"#;
    const CHATTY_MAIN: &str = "import { createApp } from \"./sdk\";\ncreateApp();\n";

    /// The agent drawing with `ui_chart` satisfies the visualization contract —
    /// it renders straight into the page, no markdown fence needed.
    #[test]
    fn lint_accepts_ui_chart_as_the_visualization_contract() {
        let out = lint_with(
            CHATTY_INDEX,
            CHATTY_MAIN,
            Some(&manifest_json(
                "Visualize the gene counts: call ui_chart with the top 10.",
                true,
            )),
        );
        assert!(!out.contains("never asks for a rendered visual"), "{out}");
    }

    /// A `data-br-region` is an agent render target, not a clickable control.
    /// Matching it as one made every app that declares a target warn.
    #[test]
    fn lint_does_not_treat_a_render_region_as_an_unwired_control() {
        let index = r#"<html><body><main class="br-container" data-br-main>
            <div class="br-card" data-br-chat></div>
            <section data-br-region="results"></section></main></body></html>"#;
        let out = lint_with(index, CHATTY_MAIN, None);
        assert!(!out.contains("wires no events"), "{out}");
    }

    #[test]
    fn lint_flags_duplicate_region_names_as_ambiguous_targets() {
        let index = r#"<html><body><main class="br-container">
            <div class="br-card" data-br-chat></div>
            <section data-br-region="results"></section>
            <section data-br-region="results"></section></main></body></html>"#;
        let out = lint_with(index, CHATTY_MAIN, None);
        assert!(out.contains("more than once"), "{out}");
        assert!(out.contains("@region:results"), "{out}");
    }

    #[test]
    fn lint_flags_ui_code_in_an_app_that_disabled_the_capability() {
        let main = "import { createApp } from \"./sdk\";\nconst br = createApp();\nbr.ui.onState(() => {});\n";
        let out = lint_with(
            CHATTY_INDEX,
            main,
            Some(&manifest_json("Answer questions.", false)),
        );
        assert!(out.contains("capabilities.ui.enabled = false"), "{out}");
        assert!(
            out.to_lowercase().contains("error"),
            "must block the build: {out}"
        );
    }

    #[test]
    fn lint_flags_a_prompt_that_calls_ui_tools_the_app_does_not_grant() {
        let out = lint_with(
            CHATTY_INDEX,
            CHATTY_MAIN,
            Some(&manifest_json(
                "Always call ui_panel to show results.",
                false,
            )),
        );
        assert!(out.contains("it has none"), "{out}");
    }
}
