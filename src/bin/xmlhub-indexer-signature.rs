use std::path::PathBuf;

use anyhow::{anyhow, bail, Context, Result};
use clap::Parser;

use xmlhub_indexer::{
    clap_styles::clap_styles,
    get_terminal_width::get_terminal_width,
    installation::app_signature::{
        AppSignature, AppSignatureKeyPair, AppSignaturePrivateKey, AppSignaturePublicKey,
        SaveLoadKeyFile,
    },
};

#[derive(clap::Parser, Debug)]
#[command(
    next_line_help = true,
    styles = clap_styles(),
    term_width = get_terminal_width(4),
    bin_name = "xmlhub-indexer-signature",
)]
/// Tool to work with app signature keys and app signatures, when this
/// should be necessary (normally the `xmlhub` tool will do it all
/// internally).
struct Opts {
    #[clap(subcommand)]
    command: Command,
}

#[derive(clap::Subcommand, Debug)]
enum Command {
    /// Generate a new key pair.
    GenKey {
        /// Owner full name and email address
        owner: String,
        /// Base path to save the key to: give a file name but without
        /// suffix. Two files will be written with `.pub` and `.priv`
        /// appended.
        output_path: PathBuf,
    },
    /// Create a signature for a file.
    Sign {
        /// The path to the file to be signed. The signature is
        /// written to the path with `.sig` appended.
        file_path: PathBuf,
        /// The path to the private key file to use for signing.
        private_key_path: PathBuf,
    },
    /// Verify a signature for a file.
    Verify {
        /// The path to the file to be verified. The signature is read
        /// from the path with `.sig` appended.
        file_path: PathBuf,
        /// The path to the public key file to use for verification.
        public_key_path: PathBuf,
    },
}

fn main() -> Result<()> {
    let opts = Opts::parse();
    match opts.command {
        Command::GenKey { owner, output_path } => {
            let key_pair = AppSignatureKeyPair::try_keygen(owner)?;
            key_pair.save_to_base(&output_path)?;
        }
        Command::Sign {
            file_path,
            private_key_path,
        } => {
            let private_key = AppSignaturePrivateKey::load_from_base(&private_key_path)?;
            let content =
                std::fs::read(&file_path).with_context(|| anyhow!("reading file {file_path:?}"))?;
            let sig = private_key.sign(&content)?;
            sig.save_to_base(&file_path)?;
        }
        Command::Verify {
            file_path,
            public_key_path,
        } => {
            let public_key = AppSignaturePublicKey::load_from_base(&public_key_path)?;
            let content =
                std::fs::read(&file_path).with_context(|| anyhow!("reading file {file_path:?}"))?;
            let sig = AppSignature::load_from_base(&file_path)?;
            let (is_valid, key_string) = public_key.verify(&content, &sig)?;
            if is_valid {
                println!(
                    "Signature OK, created by {} with key {key_string} on {} on {}",
                    sig.metadata.owner, sig.metadata.creator, sig.metadata.birth
                );
            } else {
                bail!("invalid signature");
            }
        }
    }

    Ok(())
}
