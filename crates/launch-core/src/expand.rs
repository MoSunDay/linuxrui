//! Field-code expansion and launch-plan construction.
//!
//! Expansion rules (Desktop Entry Specification):
//! - `%f`/`%u`: first file/URL; the whole argument is removed without files.
//! - `%F`/`%U`: every file/URL as separate arguments (spread); removed
//!   without files. Embedded in a larger token they are joined with spaces
//!   (the spec only defines standalone use).
//! - `%i`: standalone expands to the two arguments `--icon <icon>`; removed
//!   entirely when the entry has no Icon.
//! - `%c`: localized Name; removed when the caller has no display name.
//! - `%k`: desktop file location; removed when unknown.
//! - `%%`: literal percent (already folded by the tokenizer).
//! - Deprecated `%d %D %n %N %v %m` are deleted.
//! - Anything else is rejected with [`LaunchPlanError::UnknownFieldCode`].

use crate::exec::{tokenize, ExecToken, Piece};
use crate::{LaunchFile, LaunchPlan, LaunchPlanError, LaunchRequest};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CodeKind {
    FirstFile,
    FileList,
    Icon,
    Name,
    DesktopPath,
    Deprecated,
}

fn classify(code: char) -> Option<CodeKind> {
    match code {
        'f' | 'u' => Some(CodeKind::FirstFile),
        'F' | 'U' => Some(CodeKind::FileList),
        'i' => Some(CodeKind::Icon),
        'c' => Some(CodeKind::Name),
        'k' => Some(CodeKind::DesktopPath),
        'd' | 'D' | 'n' | 'N' | 'v' | 'm' => Some(CodeKind::Deprecated),
        _ => None,
    }
}

/// Files chosen for `%f`/`%F` (any kind) and `%u`/`%U` (URIs preferred,
/// paths passed through verbatim — no ad-hoc URI encoding here).
fn files_for(files: &[LaunchFile], urls_only: bool) -> Vec<&str> {
    let urls: Vec<&str> = files
        .iter()
        .filter(|f| matches!(f, LaunchFile::Uri(_)))
        .map(LaunchFile::as_str)
        .collect();
    if urls_only && !urls.is_empty() {
        return urls;
    }
    files.iter().map(LaunchFile::as_str).collect()
}

/// Expand one token into zero, one or many argv entries.
fn expand_token(
    token: &ExecToken,
    req: &LaunchRequest<'_>,
) -> Result<Vec<String>, LaunchPlanError> {
    let standalone = matches!(token.0.as_slice(), [Piece::Code(_)]);
    let mut out: Vec<String> = Vec::new();
    let mut current = String::new();
    for piece in &token.0 {
        match piece {
            Piece::Literal(s) => current.push_str(s),
            Piece::Code(c) => {
                let Some(kind) = classify(*c) else {
                    let code = if *c == '%' {
                        "%".to_string()
                    } else {
                        format!("%{c}")
                    };
                    return Err(LaunchPlanError::UnknownFieldCode { code });
                };
                match kind {
                    CodeKind::Deprecated => {}
                    CodeKind::FirstFile => {
                        let want_urls = *c == 'u';
                        let files = files_for(&req.files, want_urls);
                        let Some(first) = files.first() else {
                            return Ok(Vec::new()); // no file: remove the argument
                        };
                        current.push_str(first);
                    }
                    CodeKind::FileList => {
                        let want_urls = *c == 'U';
                        let list = files_for(&req.files, want_urls);
                        if list.is_empty() {
                            return Ok(Vec::new()); // no files: remove the argument
                        }
                        if standalone {
                            out.extend(list.iter().map(|s| (*s).to_string()));
                        } else {
                            current.push_str(&list.join(" "));
                        }
                    }
                    CodeKind::Icon => match (standalone, req.icon) {
                        (true, Some(icon)) => {
                            out.push("--icon".to_string());
                            out.push(icon.to_string());
                        }
                        (false, Some(icon)) => current.push_str(icon),
                        (_, None) => return Ok(Vec::new()), // no Icon: remove the argument
                    },
                    CodeKind::Name => match req.display_name {
                        Some(name) => current.push_str(name),
                        None => return Ok(Vec::new()),
                    },
                    CodeKind::DesktopPath => match req.desktop_file_path {
                        Some(p) => current.push_str(p),
                        None => return Ok(Vec::new()),
                    },
                }
            }
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    Ok(out)
}

/// Build the launch plan. The first argv entry is the program (argv[0]);
/// PATH resolution, spawning, chdir and reaping belong to the future runtime
/// launcher, which must exec this argv directly — never via a shell.
pub fn plan_launch(req: &LaunchRequest<'_>) -> Result<LaunchPlan, LaunchPlanError> {
    if req.terminal {
        let hint = req.display_name.map(str::to_string).unwrap_or_else(|| {
            req.exec
                .split_whitespace()
                .next()
                .unwrap_or_default()
                .to_string()
        });
        return Err(LaunchPlanError::UnsupportedTerminal { entry_hint: hint });
    }
    let tokens = tokenize(req.exec).map_err(|e| LaunchPlanError::Tokenize {
        detail: e.to_string(),
    })?;
    if tokens.is_empty() {
        return Err(LaunchPlanError::EmptyExec);
    }
    let mut argv = Vec::new();
    for token in &tokens {
        argv.extend(expand_token(token, req)?);
    }
    if argv.is_empty() {
        return Err(LaunchPlanError::EmptyExec);
    }
    Ok(LaunchPlan {
        argv,
        working_directory: req.working_directory.map(std::path::Path::to_path_buf),
        activation_env: req
            .activation
            .map(|(value, kind)| (value.to_string(), kind.env_var())),
    })
}
