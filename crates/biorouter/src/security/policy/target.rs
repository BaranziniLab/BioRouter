//! Platform / shell-dialect model + target-path normalization and blast-radius
//! classification (BR-68).
//!
//! **This module deliberately contains no `std::path`.** `std::path`'s semantics
//! are compiled for the *host*: on a macOS build `C:\Windows\System32` is not
//! absolute, so it would be joined onto the session cwd and every Windows rule
//! would be validated against garbage. Everything here is pure string logic over
//! an explicit target [`Platform`], which is what makes the Windows rule set
//! testable on a mac/Linux CI box (`normalize_for(Platform::Windows, …)` yields
//! the same answer on every host).
//!
//! The load-bearing piece for false-positive control is [`classify`]: a
//! destructive rule keys off the *blast radius* of the target
//! (`Root`/`SystemDir`/`HomeBare`/`WildcardAtRoot`), never off the shape of the
//! command. That is why `Remove-Item -Recurse -Force .\dist` and
//! `rm -rf node_modules` keep working while `Remove-Item -Recurse -Force C:\`
//! does not.

use std::collections::BTreeMap;

use serde::Deserialize;

/// The host we are screening commands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    Macos,
    Linux,
    Windows,
}

impl Platform {
    pub const ALL: &'static [Platform] = &[Platform::Macos, Platform::Linux, Platform::Windows];

    /// The platform this process is screening for. A pure function of `cfg!` in
    /// production; tests reach the other platforms through the `*_for` entry
    /// points rather than by overriding this.
    pub fn host() -> Platform {
        if cfg!(target_os = "windows") {
            Platform::Windows
        } else if cfg!(target_os = "macos") {
            Platform::Macos
        } else {
            Platform::Linux
        }
    }

    /// The dialect the developer shell tool will hand the model on this
    /// platform, mirroring `ShellConfig::default()` (biorouter-mcp
    /// `developer/shell.rs`): PowerShell on Windows, `$SHELL -c` elsewhere.
    pub fn default_dialect(self) -> Dialect {
        match self {
            Platform::Windows => Dialect::PowerShell,
            _ => Dialect::Posix,
        }
    }

    pub fn is_windows(self) -> bool {
        matches!(self, Platform::Windows)
    }
}

/// The shell that will actually interpret a command segment. Applicability is
/// (platform × dialect), not platform alone: `pwsh` is cross-platform, so
/// `pwsh -c "Remove-Item -Recurse -Force /"` on Linux is a real PowerShell
/// command running against a POSIX filesystem.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Dialect {
    Posix,
    #[serde(alias = "pwsh", alias = "powershell")]
    PowerShell,
    #[serde(alias = "cmd.exe")]
    Cmd,
}

/// The blast radius of a target path — the discriminator every destructive rule
/// keys off. A rule denies iff `Blast != Ordinary`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Blast {
    /// The filesystem root / a whole volume / a raw physical device.
    Root,
    /// A system directory (`/etc`, `C:\Windows`, …) or anything beneath it.
    SystemDir,
    /// *Exactly* the home directory — never anything beneath it, so
    /// `rm -rf ~/Downloads` stays legitimate.
    HomeBare,
    /// A glob (`C:\*`, `/etc/*`) whose fixed prefix is Root/SystemDir/HomeBare.
    WildcardAtRoot,
    /// Everything else. The overwhelmingly common case.
    Ordinary,
}

impl Blast {
    pub fn is_ordinary(self) -> bool {
        matches!(self, Blast::Ordinary)
    }
}

/// A cwd that itself classifies as `SystemDir` only earns the cwd-containment
/// escape hatch once it is this many components deep. Concretely: a macOS
/// session whose working dir is `/var/folders/ab/T/tmp123` (TMPDIR lives under
/// `/var`, a system dir) must still be able to `rm -rf ./build`; a session
/// parked *in* `/etc` must not get a free pass to delete `./passwd`.
const SYSTEM_CWD_TRUST_DEPTH: usize = 3;

/// The environment a command's paths resolve against. Injected rather than read
/// from `std::env`, so a Windows test case can supply Windows facts on a mac.
#[derive(Debug, Clone)]
pub struct EnvFacts {
    pub platform: Platform,
    /// Session working directory, in the target platform's native form.
    pub cwd: String,
    /// Home directory (`$HOME` / `%USERPROFILE%`), native form.
    pub home: String,
    /// Variables consulted for `%VAR%` / `$env:VAR` expansion. Keys uppercased.
    pub vars: BTreeMap<String, String>,
}

impl EnvFacts {
    /// Facts for the running host, with an explicit session cwd.
    pub fn host(cwd: &str) -> Self {
        let platform = Platform::host();
        let home = std::env::var("HOME")
            .ok()
            .filter(|h| !h.is_empty())
            .or_else(|| std::env::var("USERPROFILE").ok().filter(|h| !h.is_empty()))
            .unwrap_or_default();
        let mut vars: BTreeMap<String, String> = std::env::vars()
            .map(|(k, v)| (k.to_ascii_uppercase(), v))
            .collect();
        fill_defaults(platform, &home, &mut vars);
        Self {
            platform,
            cwd: cwd.to_string(),
            home,
            vars,
        }
    }

    /// Facts for the running host with the process cwd. Used by the BR-20 floor,
    /// which is handed a bare command string with no session context.
    pub fn host_default() -> Self {
        let cwd = std::env::current_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        Self::host(&cwd)
    }

    /// Facts for an arbitrary target platform — the entry point that lets the
    /// Windows matrix run on a mac.
    pub fn for_platform(platform: Platform, cwd: &str, home: &str) -> Self {
        let mut vars = BTreeMap::new();
        fill_defaults(platform, home, &mut vars);
        Self {
            platform,
            cwd: cwd.to_string(),
            home: home.to_string(),
            vars,
        }
    }

    pub fn var(&self, name: &str) -> Option<&str> {
        self.vars
            .get(&name.to_ascii_uppercase())
            .map(|s| s.as_str())
    }
}

fn fill_defaults(platform: Platform, home: &str, vars: &mut BTreeMap<String, String>) {
    let mut put = |k: &str, v: String| {
        vars.entry(k.to_string()).or_insert(v);
    };
    if platform.is_windows() {
        let drive = home
            .chars()
            .next()
            .filter(|c| c.is_ascii_alphabetic())
            .map(|c| format!("{c}:"))
            .unwrap_or_else(|| "C:".to_string());
        put("USERPROFILE", home.to_string());
        put("HOMEPATH", home.to_string());
        put("SYSTEMROOT", format!("{drive}\\Windows"));
        put("WINDIR", format!("{drive}\\Windows"));
        put("SYSTEMDRIVE", drive.clone());
        put("PROGRAMFILES", format!("{drive}\\Program Files"));
        put("PROGRAMFILES(X86)", format!("{drive}\\Program Files (x86)"));
        put("PROGRAMDATA", format!("{drive}\\ProgramData"));
        put("APPDATA", format!("{home}\\AppData\\Roaming"));
        put("LOCALAPPDATA", format!("{home}\\AppData\\Local"));
        put("TEMP", format!("{home}\\AppData\\Local\\Temp"));
        put("TMP", format!("{home}\\AppData\\Local\\Temp"));
    } else {
        put("HOME", home.to_string());
        put("TMPDIR", "/tmp".to_string());
    }
}

/// A path argument, normalized for a *named* target platform.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetPath {
    pub platform: Platform,
    /// `/`-separated, `..`-folded, env-expanded. Lowercased on Windows (whose
    /// filesystem is case-insensitive); case-preserving on POSIX.
    pub norm: String,
    /// The wildcard-free directory prefix, when the raw token contained a glob.
    pub fixed_prefix: Option<String>,
    /// The token exactly as it appeared in argv.
    pub raw: String,
}

/// Normalize a raw path token for `platform`. Never touches the filesystem and
/// never consults `std::path` — a security check must not depend on FS state,
/// and it must fold `..` traversal itself.
pub fn normalize_for(platform: Platform, raw: &str, env: &EnvFacts) -> TargetPath {
    let stripped = strip_quotes(raw);
    let expanded = expand_vars(platform, &stripped, env);

    let (root, rest) = if platform.is_windows() {
        split_windows_root(&expanded, env)
    } else {
        split_posix_root(&expanded, env)
    };

    // Anything without its own root is resolved against the session cwd.
    let (root, rest) = match root {
        Some(r) => (r, rest),
        None => {
            let cwd = if platform.is_windows() {
                let (cr, crest) = split_windows_root(&env.cwd, env);
                (cr.unwrap_or_else(|| "c:/".into()), crest)
            } else {
                let (cr, crest) = split_posix_root(&env.cwd, env);
                (cr.unwrap_or_else(|| "/".into()), crest)
            };
            let mut joined = cwd.1;
            joined.extend(rest);
            (cwd.0, joined)
        }
    };

    let folded = fold(rest);
    let norm = assemble(&root, &folded, platform);

    let fixed_prefix = folded
        .iter()
        .position(|c| c.contains('*') || c.contains('?'))
        .map(|idx| assemble(&root, &folded[..idx], platform));

    TargetPath {
        platform,
        norm,
        fixed_prefix,
        raw: raw.to_string(),
    }
}

/// The verdict a destructive rule keys off.
pub fn classify(p: &TargetPath, env: &EnvFacts) -> Blast {
    let base = classify_norm(p.platform, &p.norm, env);
    if base != Blast::Ordinary {
        // A target strictly below the session working directory is Ordinary by
        // definition — that is the whole point of having a workspace. The escape
        // hatch is withheld when the cwd itself sits at (or right beneath) a
        // system root, so a session parked in `/etc` cannot launder `./passwd`.
        if is_strictly_below_cwd(p.platform, &p.norm, env) && cwd_is_trusted(env) {
            return Blast::Ordinary;
        }
        return base;
    }

    if let Some(prefix) = &p.fixed_prefix {
        let pb = classify_norm(p.platform, prefix, env);
        if matches!(pb, Blast::Root | Blast::SystemDir | Blast::HomeBare) {
            if is_strictly_below_cwd(p.platform, prefix, env) && cwd_is_trusted(env) {
                return Blast::Ordinary;
            }
            return Blast::WildcardAtRoot;
        }
    }
    Blast::Ordinary
}

/// Convenience: normalize + classify in one step.
pub fn blast_of(platform: Platform, raw: &str, env: &EnvFacts) -> Blast {
    classify(&normalize_for(platform, raw, env), env)
}

fn cwd_is_trusted(env: &EnvFacts) -> bool {
    let cwd = normalize_for(env.platform, &env.cwd, env);
    match classify_norm(env.platform, &cwd.norm, env) {
        Blast::Ordinary => true,
        Blast::SystemDir => depth(&cwd.norm, env.platform) >= SYSTEM_CWD_TRUST_DEPTH,
        _ => false,
    }
}

fn is_strictly_below_cwd(platform: Platform, norm: &str, env: &EnvFacts) -> bool {
    let cwd = normalize_for(platform, &env.cwd, env).norm;
    if cwd.is_empty() {
        return false;
    }
    let base = cwd.trim_end_matches('/');
    norm.len() > base.len() + 1 && norm.starts_with(base) && norm.as_bytes()[base.len()] == b'/'
}

/// Number of path components below the root (`/etc` = 1, `/var/folders/a/b` = 4).
fn depth(norm: &str, platform: Platform) -> usize {
    let body = if platform.is_windows() {
        match norm.find('/') {
            Some(i) => &norm[i + 1..],
            None => "",
        }
    } else {
        norm.trim_start_matches('/')
    };
    body.split('/').filter(|c| !c.is_empty()).count()
}

// --- classification ------------------------------------------------------

/// POSIX system directories. Mirrors `baseline.policy.yaml`'s `path_glob` list
/// plus the macOS-only roots, so `Blast::SystemDir` and the existing rules agree.
const POSIX_SYSTEM_DIRS: &[&str] = &[
    "/etc",
    "/usr",
    "/bin",
    "/sbin",
    "/lib",
    "/lib32",
    "/lib64",
    "/boot",
    "/sys",
    "/proc",
    "/dev",
    "/var",
    "/System",
    "/Library",
    "/private/etc",
    "/private/var",
    "/opt",
];

/// Windows system directories, matched on any drive letter.
const WINDOWS_SYSTEM_DIRS: &[&str] = &[
    "windows",
    "program files",
    "program files (x86)",
    "programdata",
    "boot",
    "$recycle.bin",
    "system volume information",
    "perflogs",
];

fn classify_norm(platform: Platform, norm: &str, env: &EnvFacts) -> Blast {
    if platform.is_windows() {
        classify_windows(norm, env)
    } else {
        classify_posix(norm, env)
    }
}

fn classify_posix(norm: &str, env: &EnvFacts) -> Blast {
    if norm == "/" {
        return Blast::Root;
    }
    for dir in POSIX_SYSTEM_DIRS {
        if norm == *dir || norm.starts_with(&format!("{dir}/")) {
            return Blast::SystemDir;
        }
    }
    if !env.home.is_empty() {
        let home = normalize_for(Platform::Linux, &env.home, &bare(env)).norm;
        if !home.is_empty() && norm == home {
            return Blast::HomeBare;
        }
    }
    Blast::Ordinary
}

fn classify_windows(norm: &str, env: &EnvFacts) -> Blast {
    // Raw device paths: //./physicaldrive0, //./c:
    if norm.starts_with("//./") || norm.starts_with("//?/") {
        return Blast::Root;
    }
    // UNC: //server/share (the share root itself) is Root; deeper is Ordinary.
    if let Some(rest) = norm.strip_prefix("//") {
        let comps: Vec<&str> = rest.split('/').filter(|c| !c.is_empty()).collect();
        if comps.len() <= 2 {
            return Blast::Root;
        }
        return Blast::Ordinary;
    }

    let Some((drive, rest)) = split_drive(norm) else {
        return Blast::Ordinary;
    };
    let _ = drive;
    let body = rest.trim_start_matches('/');
    if body.is_empty() {
        return Blast::Root; // `c:` / `c:/`
    }
    for dir in WINDOWS_SYSTEM_DIRS {
        if body == *dir || body.starts_with(&format!("{dir}/")) {
            return Blast::SystemDir;
        }
    }
    // The bare `C:\Users` container (never a user's own profile beneath it).
    if body == "users" {
        return Blast::SystemDir;
    }
    if !env.home.is_empty() {
        let home = normalize_for(Platform::Windows, &env.home, &bare(env)).norm;
        if !home.is_empty() && norm == home {
            return Blast::HomeBare;
        }
    }
    Blast::Ordinary
}

/// An `EnvFacts` clone with no home, used to normalize the home path itself
/// without recursing.
fn bare(env: &EnvFacts) -> EnvFacts {
    EnvFacts {
        platform: env.platform,
        cwd: env.cwd.clone(),
        home: String::new(),
        vars: env.vars.clone(),
    }
}

fn split_drive(norm: &str) -> Option<(char, &str)> {
    let bytes = norm.as_bytes();
    if bytes.len() >= 2 && (bytes[0] as char).is_ascii_alphabetic() && bytes[1] == b':' {
        Some((bytes[0] as char, &norm[2..]))
    } else {
        None
    }
}

// --- normalization internals ---------------------------------------------

fn strip_quotes(raw: &str) -> String {
    let t = raw.trim();
    if t.len() >= 2 {
        let b = t.as_bytes();
        if (b[0] == b'"' && b[b.len() - 1] == b'"') || (b[0] == b'\'' && b[b.len() - 1] == b'\'') {
            return t[1..t.len() - 1].to_string();
        }
    }
    t.to_string()
}

/// Expand `%VAR%`, `$env:VAR`, `${VAR}`, `$VAR` and a leading `~` from the
/// injected facts. Unknown variables are left verbatim (an unexpandable target
/// classifies as Ordinary, and "no target = no deny" keeps that safe).
fn expand_vars(platform: Platform, raw: &str, env: &EnvFacts) -> String {
    let mut out = String::with_capacity(raw.len());
    let chars: Vec<char> = raw.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '~' && i == 0 && (chars.len() == 1 || chars[1] == '/' || chars[1] == '\\') {
            out.push_str(&env.home);
            i += 1;
            continue;
        }
        if c == '%' {
            if let Some(end) = chars[i + 1..].iter().position(|c| *c == '%') {
                let name: String = chars[i + 1..i + 1 + end].iter().collect();
                if let Some(v) = env.var(&name) {
                    out.push_str(v);
                    i += end + 2;
                    continue;
                }
            }
        }
        if c == '$' {
            let rest: String = chars[i..].iter().collect();
            let lower = rest.to_ascii_lowercase();
            let (skip, name) = if lower.starts_with("$env:") {
                let n: String = chars[i + 5..]
                    .iter()
                    .take_while(|c| c.is_ascii_alphanumeric() || **c == '_')
                    .collect();
                (5 + n.len(), n)
            } else if rest.starts_with("${") {
                match chars[i + 2..].iter().position(|c| *c == '}') {
                    Some(end) => {
                        let n: String = chars[i + 2..i + 2 + end].iter().collect();
                        (3 + n.len(), n)
                    }
                    None => (0, String::new()),
                }
            } else {
                let n: String = chars[i + 1..]
                    .iter()
                    .take_while(|c| c.is_ascii_alphanumeric() || **c == '_')
                    .collect();
                (1 + n.len(), n)
            };
            if !name.is_empty() {
                if let Some(v) = env.var(&name) {
                    out.push_str(v);
                    i += skip;
                    continue;
                }
            }
        }
        out.push(c);
        i += 1;
    }
    let _ = platform;
    out
}

/// Split a POSIX path into `(root, components)`. `root` is `Some("/")` only for
/// an absolute path.
fn split_posix_root(p: &str, _env: &EnvFacts) -> (Option<String>, Vec<String>) {
    let comps = |s: &str| -> Vec<String> {
        s.split('/')
            .filter(|c| !c.is_empty())
            .map(String::from)
            .collect()
    };
    if let Some(rest) = p.strip_prefix('/') {
        (Some("/".to_string()), comps(rest))
    } else {
        (None, comps(p))
    }
}

/// Split a Windows path into `(root, components)`, understanding `\\?\`/`\\.\`
/// prefixes, UNC shares, drive-absolute (`C:\x`), drive-relative (`C:x`) and
/// rooted (`\Windows`) forms. The root is lowercased and `/`-separated.
fn split_windows_root(p: &str, env: &EnvFacts) -> (Option<String>, Vec<String>) {
    let unified = p.replace('\\', "/");
    let comps = |s: &str| -> Vec<String> {
        s.split('/')
            .filter(|c| !c.is_empty())
            .map(|c| c.to_ascii_lowercase())
            .collect()
    };

    // \\?\C:\x  /  \\?\UNC\server\share  /  \\.\PhysicalDrive0
    if let Some(rest) = unified
        .strip_prefix("//?/")
        .or_else(|| unified.strip_prefix("//./"))
    {
        let device = unified.starts_with("//./");
        // \\?\C:\Windows behaves as a drive-absolute path.
        if !device && rest.len() >= 2 && rest.as_bytes()[1] == b':' {
            let drive = rest[..1].to_ascii_lowercase();
            return (Some(format!("{drive}:/")), comps(&rest[2..]));
        }
        let mut c = comps(rest);
        if c.is_empty() {
            return (Some("//./".to_string()), Vec::new());
        }
        let head = c.remove(0);
        return (Some(format!("//./{head}")), c);
    }

    // UNC \\server\share\...
    if let Some(rest) = unified.strip_prefix("//") {
        let mut c = comps(rest);
        if c.is_empty() {
            return (Some("//".to_string()), Vec::new());
        }
        let server = c.remove(0);
        let share = if c.is_empty() {
            String::new()
        } else {
            c.remove(0)
        };
        return (Some(format!("//{server}/{share}")), c);
    }

    // Drive-absolute `C:\x` / drive-relative `C:x` / bare drive `C:`.
    let b = unified.as_bytes();
    if b.len() >= 2 && (b[0] as char).is_ascii_alphabetic() && b[1] == b':' {
        let drive = unified[..1].to_ascii_lowercase();
        let rest = &unified[2..];
        // A bare drive spec (`C:`) denotes the whole drive — treat it as the
        // drive root, not the current directory, so `format C:` /
        // `Remove-Item C:` classify as Root.
        if rest.is_empty() {
            return (Some(format!("{drive}:/")), Vec::new());
        }
        if let Some(abs) = rest.strip_prefix('/') {
            return (Some(format!("{drive}:/")), comps(abs));
        }
        // Drive-relative: resolve against the cwd if it is on the same drive,
        // else against that drive's root (the conservative reading).
        let (cwd_root, mut cwd_comps) = if env.cwd.is_empty() {
            (None, Vec::new())
        } else {
            split_windows_root_nocwd(&env.cwd)
        };
        let same_drive = cwd_root
            .as_deref()
            .map(|r| r.starts_with(&format!("{drive}:")))
            .unwrap_or(false);
        if same_drive {
            cwd_comps.extend(comps(rest));
            return (Some(format!("{drive}:/")), cwd_comps);
        }
        return (Some(format!("{drive}:/")), comps(rest));
    }

    // Rooted `\Windows` — the current drive's root.
    if let Some(abs) = unified.strip_prefix('/') {
        let drive = current_drive(env);
        return (Some(format!("{drive}:/")), comps(abs));
    }

    (None, comps(&unified))
}

/// Root-split that never consults the cwd (used while splitting the cwd itself).
fn split_windows_root_nocwd(p: &str) -> (Option<String>, Vec<String>) {
    let unified = p.replace('\\', "/");
    let b = unified.as_bytes();
    if b.len() >= 2 && (b[0] as char).is_ascii_alphabetic() && b[1] == b':' {
        let drive = unified[..1].to_ascii_lowercase();
        let rest = unified[2..].trim_start_matches('/');
        return (
            Some(format!("{drive}:/")),
            rest.split('/')
                .filter(|c| !c.is_empty())
                .map(|c| c.to_ascii_lowercase())
                .collect(),
        );
    }
    (None, Vec::new())
}

fn current_drive(env: &EnvFacts) -> String {
    split_windows_root_nocwd(&env.cwd)
        .0
        .and_then(|r| r.chars().next())
        .filter(|c| c.is_ascii_alphabetic())
        .map(|c| c.to_string())
        .unwrap_or_else(|| "c".to_string())
}

/// Fold `.` and `..` lexically. `..` above the root is kept (matching
/// `std::path`'s behaviour for `/..`).
fn fold(comps: Vec<String>) -> Vec<String> {
    let mut stack: Vec<String> = Vec::with_capacity(comps.len());
    for c in comps {
        match c.as_str() {
            "." => {}
            ".." => {
                if stack.last().map(|l| l != "..").unwrap_or(false) {
                    stack.pop();
                } else {
                    stack.push(c);
                }
            }
            _ => stack.push(c),
        }
    }
    stack
}

fn assemble(root: &str, comps: &[String], platform: Platform) -> String {
    let body = comps.join("/");
    let mut s = if root.ends_with('/') {
        format!("{root}{body}")
    } else if body.is_empty() {
        root.to_string()
    } else {
        format!("{root}/{body}")
    };
    if s.len() > 1 && s.ends_with('/') && !(platform.is_windows() && s.len() <= 3) {
        s.pop();
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn win() -> EnvFacts {
        EnvFacts::for_platform(Platform::Windows, r"C:\Users\me\proj", r"C:\Users\me")
    }
    fn nix() -> EnvFacts {
        EnvFacts::for_platform(Platform::Linux, "/home/me/proj", "/home/me")
    }

    fn wnorm(raw: &str) -> String {
        normalize_for(Platform::Windows, raw, &win()).norm
    }
    fn wblast(raw: &str) -> Blast {
        blast_of(Platform::Windows, raw, &win())
    }
    fn pblast(raw: &str) -> Blast {
        blast_of(Platform::Linux, raw, &nix())
    }

    // --- the fact that makes Windows rules testable on a mac ---------------

    #[test]
    fn windows_absolute_paths_survive_on_a_posix_host() {
        // The GAP-3 regression: `std::path` on a mac would call this relative and
        // join it onto the cwd. Pure string logic must not.
        assert_eq!(wnorm(r"C:\Users\me\proj"), "c:/users/me/proj");
        assert_eq!(wnorm(r"C:\Windows\System32"), "c:/windows/system32");
        assert_eq!(wnorm(r"C:\"), "c:/");
        assert_eq!(wnorm("C:"), "c:/");
    }

    #[test]
    fn windows_path_forms() {
        assert_eq!(wnorm(r"\\?\C:\Windows"), "c:/windows");
        assert_eq!(wnorm(r"\\.\PhysicalDrive0"), "//./physicaldrive0");
        assert_eq!(wnorm(r"\\server\share\data"), "//server/share/data");
        assert_eq!(wnorm(r"\Windows\System32"), "c:/windows/system32"); // rooted
        assert_eq!(wnorm(r"C:proj\out"), "c:/users/me/proj/proj/out"); // drive-relative
        assert_eq!(wnorm(r".\dist"), "c:/users/me/proj/dist");
        assert_eq!(wnorm("dist"), "c:/users/me/proj/dist");
        assert_eq!(wnorm(r"..\..\.."), "c:/"); // climbs out of C:\Users\me\proj
        assert_eq!(wnorm(r"C:\Windows\..\Users"), "c:/users");
        assert_eq!(wnorm(r"C:\Windows\"), "c:/windows");
        assert_eq!(wnorm(r#""C:\Program Files""#), "c:/program files");
    }

    #[test]
    fn windows_env_expansion() {
        assert_eq!(wnorm("%SystemRoot%"), "c:/windows");
        assert_eq!(wnorm(r"%WINDIR%\System32"), "c:/windows/system32");
        assert_eq!(wnorm("%USERPROFILE%"), "c:/users/me");
        assert_eq!(wnorm("$env:USERPROFILE"), "c:/users/me");
        assert_eq!(
            wnorm(r"$env:TEMP\build"),
            "c:/users/me/appdata/local/temp/build"
        );
        // Unknown variable: left verbatim, stays Ordinary.
        assert_eq!(wblast("%NOPE%"), Blast::Ordinary);
    }

    #[test]
    fn windows_blast_radius() {
        assert_eq!(wblast(r"C:\"), Blast::Root);
        assert_eq!(wblast("C:"), Blast::Root);
        assert_eq!(wblast(r"\\.\PhysicalDrive0"), Blast::Root);
        assert_eq!(wblast(r"\\server\share"), Blast::Root);
        assert_eq!(wblast(r"C:\Windows"), Blast::SystemDir);
        assert_eq!(wblast(r"C:\Windows\System32"), Blast::SystemDir);
        assert_eq!(wblast(r"C:\Program Files"), Blast::SystemDir);
        assert_eq!(wblast(r"C:\ProgramData\x"), Blast::SystemDir);
        assert_eq!(wblast(r"C:\Users"), Blast::SystemDir);
        assert_eq!(wblast("$env:USERPROFILE"), Blast::HomeBare);
        assert_eq!(wblast(r"C:\Users\me"), Blast::HomeBare);
        assert_eq!(wblast(r"C:\*"), Blast::WildcardAtRoot);
        // A glob *inside* a system dir classifies as SystemDir directly (like the
        // POSIX `/etc/*` case) — still != Ordinary, so still denied.
        assert_eq!(wblast(r"C:\Windows\*"), Blast::SystemDir);
        assert_eq!(wblast(r"%USERPROFILE%\*"), Blast::WildcardAtRoot);
    }

    #[test]
    fn windows_ordinary_targets_stay_ordinary() {
        // These are the commands that decide whether the feature ships.
        for raw in [
            r".\dist",
            "node_modules",
            r".\target",
            r"$env:TEMP\build",
            r"C:\Users\me\proj\out",
            r"C:\Users\me\Downloads",
            r".\build\*",
            "dist",
            r"D:\data\run1",
        ] {
            assert_eq!(wblast(raw), Blast::Ordinary, "{raw} must stay Ordinary");
        }
    }

    #[test]
    fn posix_blast_radius_matches_the_established_discriminator() {
        assert_eq!(pblast("/"), Blast::Root);
        assert_eq!(pblast("/etc"), Blast::SystemDir);
        assert_eq!(pblast("/etc/nginx"), Blast::SystemDir);
        assert_eq!(pblast("/usr/local"), Blast::SystemDir);
        assert_eq!(pblast("~"), Blast::HomeBare);
        assert_eq!(pblast("$HOME"), Blast::HomeBare);
        assert_eq!(pblast("/home/me"), Blast::HomeBare);
        assert_eq!(pblast("/*"), Blast::WildcardAtRoot);
        // `/etc/*` starts with `/etc/`, so it classifies as SystemDir directly —
        // a glob *inside* a system dir is a system-dir operation (still != Ordinary
        // ⇒ still denied). WildcardAtRoot is only for a glob whose fixed prefix is
        // itself a root, like `/*`.
        assert_eq!(pblast("/etc/*"), Blast::SystemDir);
        // …and the near-misses the POSIX floor has always allowed.
        assert_eq!(pblast("~/Downloads"), Blast::Ordinary);
        assert_eq!(pblast("$HOME/.cache"), Blast::Ordinary);
        assert_eq!(pblast("node_modules"), Blast::Ordinary);
        assert_eq!(pblast("./build"), Blast::Ordinary);
        assert_eq!(pblast("/tmp/build"), Blast::Ordinary);
        // Traversal is folded, so it cannot smuggle a system dir past us.
        assert_eq!(pblast("../../../etc"), Blast::SystemDir);
    }

    #[test]
    fn cwd_containment_escape_hatch() {
        // macOS TMPDIR lives under /var (a system dir). A session working there
        // must still be able to delete its own build output.
        let env = EnvFacts::for_platform(
            Platform::Macos,
            "/var/folders/ab/T/tmp123/project",
            "/Users/me",
        );
        assert_eq!(
            classify(&normalize_for(Platform::Macos, "./build", &env), &env),
            Blast::Ordinary
        );
        // But a session parked in /etc does NOT get to launder ./passwd.
        let etc = EnvFacts::for_platform(Platform::Macos, "/etc", "/Users/me");
        assert_eq!(
            classify(&normalize_for(Platform::Macos, "./passwd", &etc), &etc),
            Blast::SystemDir
        );
    }

    #[test]
    fn host_platform_is_one_of_the_three() {
        assert!(Platform::ALL.contains(&Platform::host()));
    }
}
