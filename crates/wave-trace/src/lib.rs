use anyhow::Result;
use anyhow::bail;

pub fn replay_not_ready() -> Result<()> {
    bail!("Trace capture and replay are not implemented in this bootstrap slice")
}
