use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use anyhow::{anyhow, Result};
use rustls::RootCertStore;

pub fn load_root_certs(path: impl AsRef<Path>) -> Result<RootCertStore> {
    let mut roots = RootCertStore::empty();

    let cert_file = File::open(path)?;
    let mut reader = BufReader::new(cert_file);
    let cert = rustls_pemfile::certs(&mut reader)
        .into_iter()
        .next()
        .ok_or(anyhow!("invalid root certificate"))??;
    roots.add(cert)?;

    Ok(roots)
}
