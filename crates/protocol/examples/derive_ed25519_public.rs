use std::{error::Error, path::PathBuf};

use ed25519_dalek::SigningKey;
use zeroize::Zeroizing;

fn main() -> Result<(), Box<dyn Error>> {
    let mut arguments = std::env::args_os().skip(1);
    let input = PathBuf::from(arguments.next().ok_or("missing seed path")?);
    let output = PathBuf::from(arguments.next().ok_or("missing public key path")?);
    if arguments.next().is_some() || input == output {
        return Err("invalid public key derivation arguments".into());
    }
    let bytes = Zeroizing::new(std::fs::read(input)?);
    let seed: &[u8; 32] = bytes
        .as_slice()
        .try_into()
        .map_err(|_| "seed file must contain exactly 32 raw bytes")?;
    std::fs::write(
        output,
        SigningKey::from_bytes(seed).verifying_key().to_bytes(),
    )?;
    Ok(())
}
