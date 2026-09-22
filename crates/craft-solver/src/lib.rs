//! Résolution du reverse-crafting : MDP « Stochastic Shortest Path » sur un état abstrait.
//! Pipeline : abstraction (`state`) → transitions analytiques (`model`) → value iteration (`solve`)
//!            → graphe de plan (`plan`) → vérification Monte-Carlo sur le moteur exact (`verify`).

pub mod model;
pub mod plan;
pub mod solve;
pub mod state;
pub mod verify;

pub use model::*;
pub use plan::*;
pub use solve::*;
pub use state::*;
pub use verify::*;

#[cfg(test)]
mod tests;
