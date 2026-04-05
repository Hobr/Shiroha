use anyhow::Context;
use serde_json::Value;

pub(crate) fn compact_json(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| value.to_string())
}

pub(crate) fn format_optional_u64(value: Option<u64>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "-".to_string())
}

pub(crate) fn print_json_block(value: &Value, pretty: bool) {
    if pretty {
        match serde_json::to_string_pretty(value) {
            Ok(formatted) => {
                for line in formatted.lines() {
                    println!("  {line}");
                }
            }
            Err(_) => println!("  {value}"),
        }
        return;
    }

    match serde_json::to_string(value) {
        Ok(formatted) => println!("  {formatted}"),
        Err(_) => println!("  {value}"),
    }
}

pub(crate) fn print_json_value(value: &Value) -> anyhow::Result<()> {
    println!(
        "{}",
        serde_json::to_string(value).context("failed to serialize JSON output")?
    );
    Ok(())
}

pub(crate) fn value_to_label(value: &Value) -> String {
    value
        .as_str()
        .map(ToString::to_string)
        .unwrap_or_else(|| compact_json(value))
}
