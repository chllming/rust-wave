use anyhow::Result;
use anyhow::bail;

pub fn run_not_ready() -> Result<()> {
    bail!("The Codex-backed Wave TUI lands in a later wave")
}
