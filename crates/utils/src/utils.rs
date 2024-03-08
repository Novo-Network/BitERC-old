use std::str::FromStr;

use anyhow::Result;

use bitcoin::{
    secp256k1::{All, Secp256k1, SecretKey},
    Address, Network, PrivateKey,
};

pub fn parse_sk(sk: &str, network: &str) -> Result<(PrivateKey, Address)> {
    let private_key = PrivateKey {
        compressed: true,
        network: Network::from_core_arg(network)?,
        inner: SecretKey::from_str(sk.strip_prefix("0x").unwrap_or(sk))?,
    };

    let secp: Secp256k1<All> = Secp256k1::new();
    let pk = private_key.public_key(&secp);

    let address = Address::p2wpkh(&pk, private_key.network)?;

    Ok((private_key, address))
}
