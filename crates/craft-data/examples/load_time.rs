//! Mesure le temps de chargement du dataset embarqué : `cargo run --release -p craft-data --example load_time`.
use std::time::Instant;

fn main() {
    let n = 20;
    let t = Instant::now();
    for _ in 0..n {
        std::hint::black_box(craft_data::Dataset::embedded());
    }
    println!("Dataset::embedded() : {:.1} ms en moyenne sur {n} chargements", t.elapsed().as_secs_f64() * 1000.0 / n as f64);
}
