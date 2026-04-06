use std::io::Write;
use std::path::{Path, PathBuf};

use crate::CompletionShell;

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
}
