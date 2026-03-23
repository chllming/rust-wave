use anyhow::Result;
use anyhow::bail;

pub fn launch_not_ready() -> Result<()> {
    bail!("Codex-backed runtime is not implemented in this bootstrap slice")
}
