//! Release signing tool (CI only; never shipped). The key is a 32-byte hex seed in the
//! VIIMA_SIGNING_KEY environment variable.
//!
//!   viima_sign pubkey                 -> public key hex (goes into updater.rs)
//!   viima_sign <Viima.exe> <version>  -> signature hex (goes into latest.json)
#[path = "../update_sig.rs"]
mod update_sig;

use ed25519_dalek::{Signer, SigningKey};
use sha2::{Digest, Sha256};

fn main() {
    let fail = |msg: &str| -> ! {
        eprintln!("viima_sign: {msg}");
        std::process::exit(1)
    };
    let seed = std::env::var("VIIMA_SIGNING_KEY").unwrap_or_else(|_| fail("VIIMA_SIGNING_KEY puuttuu"));
    let key = SigningKey::from_bytes(
        &update_sig::from_hex::<32>(&seed).unwrap_or_else(|| fail("VIIMA_SIGNING_KEY ei ole 64 heksamerkkiä")),
    );
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [cmd] if cmd == "pubkey" => println!("{}", update_sig::to_hex(key.verifying_key().as_bytes())),
        [exe, version] => {
            let bytes = std::fs::read(exe).unwrap_or_else(|e| fail(&format!("{exe}: {e}")));
            let sha256 = update_sig::to_hex(&Sha256::digest(&bytes));
            let sig = key.sign(&update_sig::signed_message(version, &sha256));
            println!("{}", update_sig::to_hex(&sig.to_bytes()));
        }
        _ => fail("käyttö: viima_sign pubkey | viima_sign <Viima.exe> <versio>"),
    }
}
