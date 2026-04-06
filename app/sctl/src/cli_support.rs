use std::io::Write;
use std::path::{Path, PathBuf};

use crate::{CompleteArgs, CompletionShell};

pub(crate) fn default_completion_path(shell: CompletionShell) -> anyhow::Result<PathBuf> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| anyhow::anyhow!("$HOME is not set"))?;
    shell.default_output_path(&home).ok_or_else(|| {
        anyhow::anyhow!("shell `{shell}` has no default install path; use `--output <PATH>`")
    })
}

pub(crate) fn write_completion_script(path: &Path, script: &[u8]) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, script)?;
    Ok(())
}

pub(crate) fn emit_completion_script(script: &[u8]) -> anyhow::Result<()> {
    std::io::stdout().write_all(script)?;
    Ok(())
}

pub(crate) fn handle_complete(args: CompleteArgs) -> anyhow::Result<()> {
    let shell = resolve_completion_shell(args.shell)?;
    if args.print_path {
        let path = default_completion_path(shell)?;
        println!("{}", path.display());
        return Ok(());
    }

    let script = generate_completion_script(shell)?;
    if let Some(path) = args.output.as_deref() {
        write_completion_script(path, &script)?;
        println!("{}", path.display());
        return Ok(());
    }
    if args.install {
        let path = default_completion_path(shell)?;
        write_completion_script(&path, &script)?;
        println!("{}", path.display());
        return Ok(());
    }

    emit_completion_script(&script)
}

pub(crate) fn parse_positive_usize(input: &str) -> Result<usize, String> {
    parse_positive(input)
}

pub(crate) fn parse_positive_u32(input: &str) -> Result<u32, String> {
    parse_positive(input)
}

fn parse_positive<T>(input: &str) -> Result<T, String>
where
    T: std::str::FromStr + PartialEq + Default,
    T::Err: std::fmt::Display,
{
    let value = input
        .parse::<T>()
        .map_err(|error| format!("invalid positive integer `{input}`: {error}"))?;
    if value == T::default() {
        return Err("value must be greater than 0".to_string());
    }
    Ok(value)
}

fn resolve_completion_shell(shell: Option<CompletionShell>) -> anyhow::Result<CompletionShell> {
    shell.or_else(detect_shell_from_env).ok_or_else(|| {
        anyhow::anyhow!(
            "failed to detect shell from $SHELL; pass one explicitly, e.g. `sctl complete fish`"
        )
    })
}

fn detect_shell_from_env() -> Option<CompletionShell> {
    let shell = std::env::var_os("SHELL")?;
    completion_shell_from_path(Path::new(&shell))
}

fn completion_shell_from_path(path: &Path) -> Option<CompletionShell> {
    let shell = path.file_name()?.to_str()?;
    match shell {
        "bash" => Some(CompletionShell::Bash),
        "elvish" => Some(CompletionShell::Elvish),
        "fish" => Some(CompletionShell::Fish),
        "pwsh" | "powershell" => Some(CompletionShell::PowerShell),
        "zsh" => Some(CompletionShell::Zsh),
        _ => None,
    }
}

fn generate_completion_script(shell: CompletionShell) -> anyhow::Result<Vec<u8>> {
    let output = std::process::Command::new(std::env::current_exe()?)
        .env("COMPLETE", shell.env_name())
        .output()?;
    if output.status.success() {
        return Ok(output.stdout);
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    anyhow::bail!(
        "failed to generate {} completion script: {}",
        shell,
        stderr.trim()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_positive_usize_rejects_zero() {
        assert_eq!(
            parse_positive_usize("0"),
            Err("value must be greater than 0".to_string())
        );
    }

    #[test]
    fn parse_positive_parsers_accept_non_zero_values() {
        assert_eq!(parse_positive_usize("7"), Ok(7));
        assert_eq!(parse_positive_u32("9"), Ok(9));
    }

    #[test]
    fn parse_positive_parsers_report_invalid_input() {
        assert!(
            parse_positive_usize("abc")
                .expect_err("usize parser should reject non-numeric input")
                .contains("invalid positive integer `abc`")
        );
        assert!(
            parse_positive_u32("-3")
                .expect_err("u32 parser should reject negative input")
                .contains("invalid positive integer `-3`")
        );
    }

    #[test]
    fn completion_shell_from_path_uses_basename() {
        assert_eq!(
            completion_shell_from_path(Path::new("/usr/bin/fish")),
            Some(CompletionShell::Fish)
        );
    }
}
