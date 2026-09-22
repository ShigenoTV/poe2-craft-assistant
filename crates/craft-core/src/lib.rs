//! Moteur de craft exact : modèle d'objet, tirage pondéré, monnaies, objectif, Monte-Carlo.
//! Aucune dépendance à l'UI ni au format de données : tout est indexé (`u16`) pour le chemin chaud.

pub mod goal;
pub mod mc;
pub mod model;

pub use goal::*;
pub use mc::*;
pub use model::*;
