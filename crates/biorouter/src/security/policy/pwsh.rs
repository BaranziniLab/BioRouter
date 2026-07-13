//! PowerShell dialect normalizer (BR-68).
//!
//! An alias- and abbreviation-blind regex on `Remove-Item` is security theater,
//! and this module is the reason we do not ship one. PowerShell hands a confused
//! model (or an injected instruction) at least four ways to spell the same
//! destructive command:
//!
//! 1. **Aliases** — `rm`, `del`, `erase`, `rd`, `rmdir`, `ri` all resolve to
//!    `Remove-Item`; `iex` → `Invoke-Expression`; `iwr`/`curl`/`wget` →
//!    `Invoke-WebRequest`.
//! 2. **Parameter prefixes** — any *unambiguous* prefix of a parameter name
//!    binds: `-Recurse` = `-Recurs` = `-Rec` = `-R`; `-Force` = `-Fo`. (`-F` is
//!    genuinely ambiguous — `Filter` vs `Force` — and real PowerShell rejects it,
//!    so we leave it unresolved and let the residue rule speak.)
//! 3. **`-EncodedCommand`** — base64 of UTF-16LE, the standard obfuscation.
//! 4. **Expression obfuscation** — `&('Rem'+'ove-Item')`, `iex (…)`. We do *not*
//!    claim to defeat this with pattern matching; nobody can. It is detected as
//!    *residue* ([`is_obfuscated_exec`]) and answered with `ask`, honestly.
//!
//! Layers 1-2 are defeated completely by normalization, layer 3 by decoding and
//! re-parsing, layer 4 by admitting the limit. Rules run *after* this pass, so
//! they can be written against canonical `Remove-Item -Recurse -Force`.

use std::collections::BTreeMap;

use base64::Engine as _;
use lazy_static::lazy_static;

/// Default PowerShell aliases that matter for safety: destructive, network,
/// exec. Deliberately small and auditable — not the full ~150-entry table.
const ALIASES: &[(&str, &str)] = &[
    // Destructive
    ("rm", "Remove-Item"),
    ("del", "Remove-Item"),
    ("erase", "Remove-Item"),
    ("rd", "Remove-Item"),
    ("rmdir", "Remove-Item"),
    ("ri", "Remove-Item"),
    ("rdr", "Remove-Item"),
    ("ren", "Rename-Item"),
    ("rni", "Rename-Item"),
    ("mi", "Move-Item"),
    ("mv", "Move-Item"),
    ("move", "Move-Item"),
    ("cpi", "Copy-Item"),
    ("cp", "Copy-Item"),
    ("copy", "Copy-Item"),
    ("clv", "Clear-Variable"),
    ("cli", "Clear-Item"),
    ("rp", "Remove-ItemProperty"),
    ("ni", "New-Item"),
    ("si", "Set-Item"),
    ("sp", "Set-ItemProperty"),
    ("sc", "Set-Content"),
    ("ac", "Add-Content"),
    // Read-only (present so a read never looks like a write)
    ("ls", "Get-ChildItem"),
    ("dir", "Get-ChildItem"),
    ("gci", "Get-ChildItem"),
    ("gc", "Get-Content"),
    ("cat", "Get-Content"),
    ("type", "Get-Content"),
    ("gp", "Get-ItemProperty"),
    ("gi", "Get-Item"),
    ("sls", "Select-String"),
    ("gcm", "Get-Command"),
    ("gwmi", "Get-WmiObject"),
    // Exec / network sinks
    ("iex", "Invoke-Expression"),
    ("icm", "Invoke-Command"),
    ("iwr", "Invoke-WebRequest"),
    ("curl", "Invoke-WebRequest"),
    ("wget", "Invoke-WebRequest"),
    ("irm", "Invoke-RestMethod"),
    ("saps", "Start-Process"),
    ("start", "Start-Process"),
    ("spps", "Stop-Process"),
    ("kill", "Stop-Process"),
];

/// Canonical parameter sets for the cmdlets we govern. Prefix expansion is only
/// as faithful as this list, so each set carries the *ambiguity-creating*
/// siblings too (`Filter` next to `Force`) — dropping them would silently turn
/// `-f` into `-Force` and diverge from real PowerShell.
const PARAM_SETS: &[(&str, &[&str])] = &[
    (
        "Remove-Item",
        &[
            "Path",
            "LiteralPath",
            "Filter",
            "Include",
            "Exclude",
            "Recurse",
            "Force",
            "Credential",
            "WhatIf",
            "Confirm",
            "Stream",
            "Verbose",
            "ErrorAction",
        ],
    ),
    (
        "Get-ChildItem",
        &[
            "Path",
            "LiteralPath",
            "Filter",
            "Include",
            "Exclude",
            "Recurse",
            "Depth",
            "Force",
            "Name",
            "Attributes",
            "Directory",
            "File",
            "Hidden",
            "ReadOnly",
            "System",
        ],
    ),
    (
        "Copy-Item",
        &[
            "Path",
            "LiteralPath",
            "Destination",
            "Container",
            "Filter",
            "Include",
            "Exclude",
            "Recurse",
            "Force",
            "PassThru",
            "Confirm",
            "WhatIf",
        ],
    ),
    (
        "Move-Item",
        &[
            "Path",
            "LiteralPath",
            "Destination",
            "Filter",
            "Include",
            "Exclude",
            "Force",
            "PassThru",
            "Confirm",
            "WhatIf",
        ],
    ),
    (
        "Invoke-WebRequest",
        &[
            "Uri",
            "OutFile",
            "Method",
            "Body",
            "Headers",
            "UseBasicParsing",
            "UserAgent",
            "Credential",
            "SkipCertificateCheck",
        ],
    ),
    (
        "Stop-Computer",
        &["ComputerName", "Credential", "Force", "WhatIf", "Confirm"],
    ),
    (
        "Restart-Computer",
        &[
            "ComputerName",
            "Credential",
            "Force",
            "Wait",
            "WhatIf",
            "Confirm",
        ],
    ),
    (
        "Set-ExecutionPolicy",
        &["ExecutionPolicy", "Scope", "Force", "WhatIf", "Confirm"],
    ),
];

/// `powershell.exe` / `pwsh` *host* switches. These are not cmdlet parameters —
/// the host parses them itself, with its own historical abbreviations (`-enc`,
/// `-ec`, `-e` all mean `-EncodedCommand`), so they get their own table.
const HOST_SWITCH_ALIASES: &[(&str, &str)] = &[
    ("e", "EncodedCommand"),
    ("en", "EncodedCommand"),
    ("ec", "EncodedCommand"),
    ("enc", "EncodedCommand"),
    ("encoded", "EncodedCommand"),
    ("encodedcommand", "EncodedCommand"),
    ("c", "Command"),
    ("com", "Command"),
    ("command", "Command"),
    ("f", "File"),
    ("file", "File"),
    ("ep", "ExecutionPolicy"),
    ("executionpolicy", "ExecutionPolicy"),
    ("nop", "NoProfile"),
    ("noprofile", "NoProfile"),
    ("noni", "NonInteractive"),
    ("noninteractive", "NonInteractive"),
    ("w", "WindowStyle"),
    ("windowstyle", "WindowStyle"),
];

lazy_static! {
    static ref ALIAS_MAP: BTreeMap<&'static str, &'static str> = ALIASES.iter().copied().collect();
    static ref PARAM_MAP: BTreeMap<&'static str, &'static [&'static str]> =
        PARAM_SETS.iter().copied().collect();
    static ref HOST_SWITCH_MAP: BTreeMap<&'static str, &'static str> =
        HOST_SWITCH_ALIASES.iter().copied().collect();
}

/// Is this argv[0] a PowerShell *host* (as opposed to a cmdlet)?
pub fn is_powershell_host(binary: &str) -> bool {
    let b = binary.trim_end_matches(".exe").to_ascii_lowercase();
    matches!(b.as_str(), "powershell" | "pwsh" | "powershell_ise")
}

/// Resolve a PowerShell alias to its canonical cmdlet name. Non-aliases pass
/// through with their original spelling (case-normalized for known cmdlets).
pub fn resolve_alias(name: &str) -> String {
    let lower = name.trim_end_matches(".exe").to_ascii_lowercase();
    if let Some(canon) = ALIAS_MAP.get(lower.as_str()) {
        return (*canon).to_string();
    }
    // A cmdlet typed with different casing (`remove-item`) must canonicalize too.
    for (canon, _) in PARAM_MAP.iter() {
        if canon.eq_ignore_ascii_case(&lower) {
            return (*canon).to_string();
        }
    }
    name.to_string()
}

/// The outcome of expanding one `-Param` token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParamExpansion {
    /// Resolved to exactly one canonical parameter.
    Resolved(String),
    /// A prefix of two or more parameters — real PowerShell rejects this, and so
    /// do we: it stays unresolved and feeds the residue rule.
    Ambiguous,
    /// Not a prefix of any known parameter of this cmdlet (or the cmdlet is not
    /// governed): pass through verbatim.
    Unknown,
}

/// Expand `-rec` → `Recurse` for `cmdlet`, honoring PowerShell's
/// unambiguous-prefix rule. `token` is the raw `-Xyz` argv token.
pub fn expand_param(cmdlet: &str, token: &str) -> ParamExpansion {
    let Some(body) = token.strip_prefix('-') else {
        return ParamExpansion::Unknown;
    };
    // `-Recurse:$true` / `-Force:$false` — the value is not part of the name.
    let name = body.split(':').next().unwrap_or(body).to_ascii_lowercase();
    if name.is_empty() {
        return ParamExpansion::Unknown;
    }
    let Some(params) = PARAM_MAP.get(cmdlet) else {
        return ParamExpansion::Unknown;
    };
    // An exact (case-insensitive) hit always wins over prefix ambiguity.
    if let Some(p) = params.iter().find(|p| p.eq_ignore_ascii_case(&name)) {
        return ParamExpansion::Resolved((*p).to_string());
    }
    let hits: Vec<&&str> = params
        .iter()
        .filter(|p| p.to_ascii_lowercase().starts_with(&name))
        .collect();
    match hits.len() {
        0 => ParamExpansion::Unknown,
        1 => ParamExpansion::Resolved((*hits[0]).to_string()),
        _ => ParamExpansion::Ambiguous,
    }
}

/// Resolve a `powershell.exe`/`pwsh` host switch (`-enc` → `EncodedCommand`).
pub fn expand_host_switch(token: &str) -> Option<&'static str> {
    let body = token
        .strip_prefix("--")
        .or_else(|| token.strip_prefix('-'))?;
    let name = body.split(':').next().unwrap_or(body).to_ascii_lowercase();
    HOST_SWITCH_MAP.get(name.as_str()).copied()
}

/// Decode a `-EncodedCommand` payload: base64 → UTF-16LE → script text.
/// Returns `None` for anything that does not decode cleanly — that residue is
/// itself a signal, and the caller treats it as obfuscation rather than
/// pretending the command is benign.
pub fn decode_encoded_command(b64: &str) -> Option<String> {
    let trimmed = b64.trim().trim_matches(|c| c == '"' || c == '\'');
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(trimmed)
        .ok()?;
    if bytes.is_empty() || bytes.len() % 2 != 0 {
        return None;
    }
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    let s = String::from_utf16(&units).ok()?;
    if s.chars().any(|c| c == '\u{0}') {
        return None;
    }
    Some(s)
}

/// PowerShell tokenizer. The critical difference from `shlex`: **backslash is
/// not an escape character** — backtick is. That single fact is why the POSIX
/// tokenizer mangled `C:\Users\me` into `C:Usersme` (GAP-3), and why every
/// Windows rule written against the old parser would have matched nothing.
///
/// Single quotes are literal (`''` escapes a quote); double quotes interpolate
/// and honor backtick escapes.
pub fn tokenize(input: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut has_token = false;
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];
        match c {
            c if c.is_whitespace() => {
                if has_token {
                    out.push(std::mem::take(&mut cur));
                    has_token = false;
                }
                i += 1;
            }
            '`' => {
                // Backtick escape: the next char is literal.
                if i + 1 < chars.len() {
                    cur.push(chars[i + 1]);
                    has_token = true;
                    i += 2;
                } else {
                    i += 1;
                }
            }
            '\'' => {
                has_token = true;
                i += 1;
                while i < chars.len() {
                    if chars[i] == '\'' {
                        if chars.get(i + 1) == Some(&'\'') {
                            cur.push('\'');
                            i += 2;
                            continue;
                        }
                        i += 1;
                        break;
                    }
                    cur.push(chars[i]);
                    i += 1;
                }
            }
            '"' => {
                has_token = true;
                i += 1;
                while i < chars.len() {
                    match chars[i] {
                        '"' => {
                            if chars.get(i + 1) == Some(&'"') {
                                cur.push('"');
                                i += 2;
                                continue;
                            }
                            i += 1;
                            break;
                        }
                        '`' if i + 1 < chars.len() => {
                            cur.push(chars[i + 1]);
                            i += 2;
                        }
                        other => {
                            cur.push(other);
                            i += 1;
                        }
                    }
                }
            }
            other => {
                cur.push(other);
                has_token = true;
                i += 1;
            }
        }
    }
    if has_token {
        out.push(cur);
    }
    out
}

/// A PowerShell segment after normalization: canonical cmdlet, canonical
/// parameter names, positional arguments, and the residue signals.
#[derive(Debug, Clone, Default)]
pub struct PwshSegment {
    /// Canonical cmdlet (`Remove-Item`), or the literal binary for non-cmdlets.
    pub cmdlet: String,
    /// argv[0] exactly as typed, lowercased (`rd`, `del`, `takeown`).
    pub raw_binary: String,
    /// Canonical parameter names present, lowercased (`recurse`, `force`).
    pub params: Vec<String>,
    /// Non-parameter arguments, in order.
    pub positional: Vec<String>,
    /// Normalized argv (`Remove-Item -Recurse -Force C:\`), for `arg_regex`.
    pub argv: Vec<String>,
    /// At least one `-X` token was an ambiguous prefix (real PowerShell would
    /// have rejected the command).
    pub ambiguous_param: bool,
}

impl PwshSegment {
    pub fn has_param(&self, name: &str) -> bool {
        self.params.iter().any(|p| p.eq_ignore_ascii_case(name))
    }
}

/// Normalize one already-tokenized PowerShell stage.
pub fn normalize(tokens: &[String]) -> PwshSegment {
    let Some(first) = tokens.first() else {
        return PwshSegment::default();
    };
    let raw_binary = basename_lower(first);
    let cmdlet = resolve_alias(&raw_binary);

    let mut seg = PwshSegment {
        cmdlet: cmdlet.clone(),
        raw_binary,
        argv: vec![cmdlet.clone()],
        ..Default::default()
    };

    for tok in tokens.iter().skip(1) {
        if tok.starts_with('-') && tok.len() > 1 && !tok.starts_with("--") {
            match expand_param(&cmdlet, tok) {
                ParamExpansion::Resolved(p) => {
                    seg.params.push(p.to_ascii_lowercase());
                    seg.argv.push(format!("-{p}"));
                }
                ParamExpansion::Ambiguous => {
                    seg.ambiguous_param = true;
                    seg.argv.push(tok.clone());
                }
                ParamExpansion::Unknown => {
                    seg.argv.push(tok.clone());
                }
            }
            continue;
        }
        seg.positional.push(tok.clone());
        seg.argv.push(tok.clone());
    }
    seg
}

/// Residue detection (layer 3). True when a PowerShell stage reaches an
/// *execution sink* whose command name is not a resolvable literal:
/// `&('Rem'+'ove-Item')`, `iex ([Text.Encoding]…)`, `[scriptblock]::Create(…)`.
///
/// This is deliberately an `ask` signal, not a `deny`: `iex .\build.ps1` on a
/// literal local script is legitimate, and a deny here would be a
/// false-positive generator. A denylist is a mistake-catcher, not an
/// adversary-stopper — see BR-64 for the containment answer.
pub fn is_obfuscated_exec(stage: &str, tokens: &[String]) -> bool {
    let lower = stage.to_ascii_lowercase();
    if lower.contains("[scriptblock]::create") {
        return true;
    }
    if lower.contains("frombase64string") {
        return true;
    }

    let Some(first) = tokens.first() else {
        return false;
    };
    // Call operator (`&` / `.`) applied to a non-literal expression.
    let t = first.trim();
    if (t == "&" || t == ".") || t.starts_with("&(") || t.starts_with(".(") || t.starts_with("&$") {
        let rest: String = if t.len() > 1 {
            t[1..].to_string()
        } else {
            tokens.get(1).cloned().unwrap_or_default()
        };
        let r = rest.trim();
        if r.starts_with('(') || r.starts_with('$') || r.contains('+') {
            return true;
        }
    }
    // `iex`/`Invoke-Expression` on anything that is not a plain literal path.
    let bin = resolve_alias(&basename_lower(first));
    if bin.eq_ignore_ascii_case("Invoke-Expression") {
        let arg = tokens
            .iter()
            .skip(1)
            .find(|t| !t.starts_with('-'))
            .cloned()
            .unwrap_or_default();
        let a = arg.trim();
        // A bare `iex` (no argument) does nothing — not residue. A non-literal
        // argument (an expression, variable, or concatenation) is.
        if !a.is_empty()
            && (a.starts_with('(') || a.starts_with('$') || a.contains('+') || a.contains('['))
        {
            return true;
        }
    }
    false
}

fn basename_lower(token: &str) -> String {
    let t = token.rsplit(['/', '\\']).next().unwrap_or(token);
    t.trim_end_matches(".exe").to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backslashes_are_not_escapes() {
        // The GAP-3 bug, at the tokenizer level: `shlex` returns `C:Usersme`.
        assert_eq!(
            tokenize(r"Remove-Item -Recurse -Force C:\Users\me\proj"),
            vec!["Remove-Item", "-Recurse", "-Force", r"C:\Users\me\proj"]
        );
        assert_eq!(
            shlex::split(r"Remove-Item -Recurse -Force C:\Users\me\proj").unwrap(),
            vec!["Remove-Item", "-Recurse", "-Force", "C:Usersmeproj"],
            "this is what the old POSIX tokenizer did — the reason for pwsh.rs"
        );
    }

    #[test]
    fn quoting_and_backtick_escapes() {
        assert_eq!(
            tokenize(r#"Remove-Item "C:\Program Files\App""#),
            vec!["Remove-Item", r"C:\Program Files\App"]
        );
        assert_eq!(
            tokenize(r"Remove-Item 'C:\Program Files'"),
            vec!["Remove-Item", r"C:\Program Files"]
        );
        // Backtick escapes exactly one char (a space here), so `a`<esc-space>`b`
        // is one token "a b". A *second*, unescaped space would split.
        assert_eq!(tokenize("echo a` b"), vec!["echo", "a b"]);
        assert_eq!(tokenize("echo a`  b"), vec!["echo", "a ", "b"]);
        assert_eq!(tokenize(r"'it''s'"), vec!["it's"]);
    }

    #[test]
    fn alias_table_round_trip() {
        for a in ["rm", "del", "erase", "ri", "rd", "rmdir", "Del", "RM"] {
            assert_eq!(resolve_alias(a), "Remove-Item", "alias {a}");
        }
        assert_eq!(resolve_alias("iex"), "Invoke-Expression");
        assert_eq!(resolve_alias("iwr"), "Invoke-WebRequest");
        assert_eq!(resolve_alias("curl"), "Invoke-WebRequest");
        assert_eq!(resolve_alias("remove-item"), "Remove-Item");
        assert_eq!(resolve_alias("takeown"), "takeown");
    }

    #[test]
    fn parameter_prefix_expansion() {
        for p in ["-r", "-re", "-rec", "-recu", "-Recurse", "-RECURSE"] {
            assert_eq!(
                expand_param("Remove-Item", p),
                ParamExpansion::Resolved("Recurse".into()),
                "prefix {p}"
            );
        }
        for p in ["-fo", "-for", "-Force", "-force:$true"] {
            assert_eq!(
                expand_param("Remove-Item", p),
                ParamExpansion::Resolved("Force".into()),
                "prefix {p}"
            );
        }
        // `-f` is Filter-vs-Force: real PowerShell errors, so we do not guess.
        assert_eq!(expand_param("Remove-Item", "-f"), ParamExpansion::Ambiguous);
        assert_eq!(
            expand_param("Remove-Item", "-fi"),
            ParamExpansion::Resolved("Filter".into())
        );
        assert_eq!(expand_param("Remove-Item", "-zzz"), ParamExpansion::Unknown);
    }

    #[test]
    fn normalize_alias_plus_abbreviation() {
        let seg = normalize(&tokenize(r"ri -rec -fo C:\"));
        assert_eq!(seg.cmdlet, "Remove-Item");
        assert_eq!(seg.raw_binary, "ri");
        assert!(seg.has_param("recurse") && seg.has_param("force"));
        assert_eq!(seg.positional, vec![r"C:\"]);
        assert_eq!(seg.argv.join(" "), r"Remove-Item -Recurse -Force C:\");
    }

    #[test]
    fn rm_dash_rf_does_not_silently_become_recurse_force() {
        // `-rf` is not a prefix of any Remove-Item parameter; it must not be
        // laundered into `-Recurse -Force`.
        let seg = normalize(&tokenize("rm -rf node_modules"));
        assert_eq!(seg.cmdlet, "Remove-Item");
        assert!(!seg.has_param("recurse"));
        assert!(!seg.has_param("force"));
    }

    #[test]
    fn encoded_command_decodes() {
        // UTF-16LE base64 of: Remove-Item -Recurse -Force C:\
        let script = r"Remove-Item -Recurse -Force C:\";
        let utf16: Vec<u8> = script
            .encode_utf16()
            .flat_map(|u| u.to_le_bytes())
            .collect();
        let b64 = base64::engine::general_purpose::STANDARD.encode(&utf16);
        assert_eq!(decode_encoded_command(&b64).as_deref(), Some(script));
        assert_eq!(decode_encoded_command("not base64!!"), None);
    }

    #[test]
    fn host_switch_abbreviations() {
        for s in ["-e", "-en", "-ec", "-enc", "-EncodedCommand"] {
            assert_eq!(expand_host_switch(s), Some("EncodedCommand"), "{s}");
        }
        assert_eq!(expand_host_switch("-Command"), Some("Command"));
        assert_eq!(expand_host_switch("-c"), Some("Command"));
        assert_eq!(expand_host_switch("-NoProfile"), Some("NoProfile"));
    }

    #[test]
    fn obfuscation_residue_is_detected_but_literals_are_not() {
        assert!(is_obfuscated_exec(
            r"&('Rem'+'ove-Item') -Recurse -Force C:\",
            &tokenize(r"&('Rem'+'ove-Item') -Recurse -Force C:\")
        ));
        assert!(is_obfuscated_exec(
            "iex ([Text.Encoding]::Unicode.GetString([Convert]::FromBase64String($x)))",
            &tokenize("iex ([Text.Encoding]::Unicode.GetString([Convert]::FromBase64String($x)))")
        ));
        assert!(is_obfuscated_exec(
            "[scriptblock]::Create($s).Invoke()",
            &tokenize("[scriptblock]::Create($s).Invoke()")
        ));
        // A literal local script through iex is legitimate — no residue.
        assert!(!is_obfuscated_exec(
            r"iex .\build.ps1",
            &tokenize(r"iex .\build.ps1")
        ));
        assert!(!is_obfuscated_exec(
            r"Remove-Item -Recurse -Force .\dist",
            &tokenize(r"Remove-Item -Recurse -Force .\dist")
        ));
    }
}
