//! Compresse le dataset embarqué (`data/sample/dataset.json`, plus de 1 Mo) en gzip au moment de la
//! compilation : l'exécutable n'en garde que la version compressée (~10 fois plus petite), décompressée
//! une seule fois au démarrage (`Dataset::embedded`). Le fichier du dépôt reste du JSON lisible, modifié
//! par les outils (`tools/*.mjs`, update-dataset.bat) comme avant.
use flate2::{write::GzEncoder, Compression};
use std::io::Write;
use std::path::PathBuf;

fn main() {
    let src = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("../../data/sample/dataset.json");
    println!("cargo:rerun-if-changed={}", src.display());
    let json = std::fs::read(&src).unwrap_or_else(|e| panic!("{} illisible : {e}", src.display()));
    let mut gz = GzEncoder::new(Vec::new(), Compression::best());
    gz.write_all(&json).unwrap();
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("dataset.json.gz");
    std::fs::write(&out, gz.finish().unwrap()).unwrap();
}
