use serde::Serialize;

pub enum OutputFormat {
    Table,
    Json,
    Yaml,
}

impl OutputFormat {
    pub fn from_str(s: &str) -> Self {
        match s {
            "json" => Self::Json,
            "yaml" => Self::Yaml,
            _ => Self::Table,
        }
    }
}

pub fn print_json<T: Serialize>(value: &T) -> anyhow::Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}
