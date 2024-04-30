use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use anyhow::Result;
use rustls::{Certificate, RootCertStore};

pub fn load_root_certs(path: Option<impl AsRef<Path>>) -> Result<RootCertStore> {
    let mut roots = RootCertStore::empty();

    match path {
        Some(path) => {
            let cert_file = File::open(path)?;
            let mut reader = BufReader::new(cert_file);
            for cert in rustls_pemfile::certs(&mut reader)? {
                roots.add(&Certificate(cert))?
            }
        }
        None => {
            let certs = rustls_native_certs::load_native_certs()?;
            for cert in certs {
                roots.add(&Certificate(cert.0))?;
            }
        }
    }

    Ok(roots)
}
