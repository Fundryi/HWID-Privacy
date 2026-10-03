//! Provider stub; the owning wave A work package ports the C# section here.

use crate::{hw::Ctx, report::Out, win};

/// Collects this hardware section through the shared output builder.
pub fn collect(_ctx: &Ctx, out: &mut Out) -> Result<(), win::Error> {
    out.source("not ported yet");
    Err(win::Error::msg("tpm", "not ported yet"))
}
