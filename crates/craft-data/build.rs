//! Compresse `data/sample/dataset.json` (plus de 1 Mo de JSON) en gzip dans `OUT_DIR` : l'exécutable
//! embarque ~100 Ko au lieu du JSON brut, décompressé au chargement par `Dataset::embedded`.
use flate2::{write::GzEncoder, Compression};
use std::io::Write;
use std::path::PathBuf;

fn main() {
    let src = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("../../data/sample/dataset.json");
    println!("cargo:rerun-if-changed={}", src.display());
    let json = std::fs::read(&src).unwrap_or_else(|e| panic!("lecture de {} : {e}", src.display()));
    let mut gz = GzEncoder::new(Vec::new(), Compression::best());
    gz.write_all(&json).unwrap();
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("dataset.json.gz");
    std::fs::write(&out, gz.finish().unwrap()).unwrap();
}
