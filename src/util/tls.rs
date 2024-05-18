use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use anyhow::{anyhow, Result};
use rustls::RootCertStore;

pub fn load_root_certs(path: Option<impl AsRef<Path>>) -> Result<RootCertStore> {
    let mut roots = RootCertStore::empty();

    match path {
        Some(path) => {
            let cert_file = File::open(path)?;
            let mut reader = BufReader::new(cert_file);
            let cert = rustls_pemfile::certs(&mut reader)
                .into_iter()
                .next()
                .ok_or(anyhow!("invalid root certificate"))??;
            roots.add(cert)?;
        }
        None => {
            let certs = rustls_native_certs::load_native_certs()?;
            for cert in certs {
                roots.add(cert)?;
            }
        }
    }

    Ok(roots)
}
