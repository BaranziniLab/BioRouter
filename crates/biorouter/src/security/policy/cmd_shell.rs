//! `cmd.exe` dialect normalizer (BR-68).
//!
//! Three things `shlex` gets wrong about `cmd.exe`, each of which silently
//! defeats a Windows rule:
//!
//! * **Backslash is not an escape** — `del /q C:\Users\me` must keep its
//!   separators (the GAP-3 bug). The escape character is the caret `^`.
//! * **Switches start with `/`** — `del /f /s /q C:\` has three switches and one
//!   path. The POSIX parser calls `/f`, `/s`, `/q` *filesystem paths*, resolves
//!   them against the cwd, and buries the real target.
//! * **`%VAR%` expansion** — `%SystemRoot%` and `%USERPROFILE%` are how a Windows
//!   command names the very directories a destructive rule cares about.
//!
//! Variable-slicing obfuscation (`%CD:~0,1%`) is *not* defeated here; like the
//! PowerShell case it is left as residue and answered with `ask`, honestly.

use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    /// A `cmd.exe` switch: `/f`, `/s`, `/q`, `/w:C:\`, `/fs:ntfs`, `/grant`.
    /// A token containing a further `/` (`/usr/local`) is never a switch.
    ///
    /// NOTE: on Windows *any* `/x` token is a switch, so this is only consulted
    /// when the target platform is Windows or the dialect is `Cmd` — the gate
    /// lives in `command.rs::is_path_arg`. Without that gate, `pwsh -c
    /// 'Remove-Item -Recurse -Force /'` on Linux would lose its `/` target.
    static ref CMD_SWITCH: Regex =
        Regex::new(r"(?i)^/([a-z?][a-z0-9?_-]*)(?::(.*))?$").unwrap();
}

/// Tokenize a `cmd.exe` stage: caret escapes, double quotes, whitespace split.
/// Backslash is a plain character.
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
            '^' => {
                if i + 1 < chars.len() {
                    cur.push(chars[i + 1]);
                    has_token = true;
                    i += 2;
                } else {
                    i += 1;
                }
            }
            '"' => {
                has_token = true;
                i += 1;
                while i < chars.len() && chars[i] != '"' {
                    cur.push(chars[i]);
                    i += 1;
                }
                i += 1; // closing quote
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

/// Split a token into `(switch_name, switch_value)` when it is a `cmd.exe`
/// switch. `/w:C:\` → `("w", Some("C:\\"))`; `/s` → `("s", None)`.
pub fn as_switch(token: &str) -> Option<(String, Option<String>)> {
    let caps = CMD_SWITCH.captures(token)?;
    let name = caps.get(1)?.as_str().to_ascii_lowercase();
    let value = caps.get(2).map(|m| m.as_str().to_string());
    Some((name, value))
}

/// A registry path (`HKLM\SYSTEM`) is not a filesystem path and must never be
/// canonicalized against the cwd.
pub fn is_registry_path(token: &str) -> bool {
    let t = token.trim_start_matches(['"', '\'']).to_ascii_uppercase();
    t.starts_with("HKLM")
        || t.starts_with("HKCU")
        || t.starts_with("HKCR")
        || t.starts_with("HKU")
        || t.starts_with("HKCC")
        || t.starts_with("HKEY_")
}

/// `cmd /c "<script>"` / `cmd /k …`: the inner script, so its real binary and
/// target are seen by the same rule as the direct form (the mirror image of the
/// `sh -c` unwrap the POSIX path already does).
pub fn extract_cmd_script(tokens: &[String]) -> Option<String> {
    let mut idx = 1;
    while idx < tokens.len() {
        if let Some((name, value)) = as_switch(&tokens[idx]) {
            if name == "c" || name == "k" || name == "r" {
                if let Some(v) = value {
                    if !v.is_empty() {
                        return Some(v);
                    }
                }
                return Some(tokens[idx + 1..].join(" "));
            }
            idx += 1;
            continue;
        }
        idx += 1;
    }
    None
}

/// Is this argv[0] `cmd.exe`?
pub fn is_cmd_host(binary: &str) -> bool {
    let b = binary.trim_end_matches(".exe").to_ascii_lowercase();
    b == "cmd"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backslashes_survive_tokenization() {
        assert_eq!(
            tokenize(r"del /f /s /q C:\Users\me"),
            vec!["del", "/f", "/s", "/q", r"C:\Users\me"]
        );
    }

    #[test]
    fn caret_is_the_escape_character() {
        assert_eq!(tokenize("echo a^ b"), vec!["echo", "a b"]);
        assert_eq!(tokenize("echo a^^b"), vec!["echo", "a^b"]);
        // Caret-splicing a command name is normalizable; we un-escape it.
        assert_eq!(tokenize("d^el /q x"), vec!["del", "/q", "x"]);
    }

    #[test]
    fn quotes() {
        assert_eq!(
            tokenize(r#"rd /s /q "C:\Program Files\App""#),
            vec!["rd", "/s", "/q", r"C:\Program Files\App"]
        );
    }

    #[test]
    fn switch_recognition() {
        assert_eq!(as_switch("/s"), Some(("s".into(), None)));
        assert_eq!(as_switch("/Q"), Some(("q".into(), None)));
        assert_eq!(
            as_switch(r"/w:C:\"),
            Some(("w".into(), Some(r"C:\".into())))
        );
        assert_eq!(
            as_switch("/fs:ntfs"),
            Some(("fs".into(), Some("ntfs".into())))
        );
        assert_eq!(as_switch("/grant"), Some(("grant".into(), None)));
        // Never a switch: a bare slash, or anything with a further separator.
        assert_eq!(as_switch("/"), None);
        assert_eq!(as_switch("/usr/local"), None);
        // `/etc` *is* switch-shaped; the platform gate in command.rs is what keeps
        // it a path on POSIX. Asserted here so the gate is not quietly removed.
        assert_eq!(as_switch("/etc"), Some(("etc".into(), None)));
    }

    #[test]
    fn registry_paths_are_not_filesystem_paths() {
        assert!(is_registry_path(r"HKLM\SYSTEM"));
        assert!(is_registry_path("HKEY_LOCAL_MACHINE"));
        assert!(!is_registry_path(r"C:\Windows"));
    }

    #[test]
    fn cmd_slash_c_unwrap() {
        assert_eq!(
            extract_cmd_script(&tokenize(r"cmd /c del /f /s /q C:\")),
            Some(r"del /f /s /q C:\".to_string())
        );
    }
}
