//! Données de jeu (bases, affixes, poids, monnaies, prix) + lecture du texte d'objet (presse-papiers).

pub mod dataset;
pub mod itemtext;
pub mod prices;

pub use dataset::*;
pub use itemtext::*;
pub use prices::*;
