//! The map editor's rules that need neither the map nor the screen: the brush, the ground
//! height tools, undo and redo, the region clipboard and templates, the visible height band,
//! the terrain brush with its automatic borders and the border sets.

pub mod band;
pub mod borders;
pub mod brush;
pub mod height;
pub mod history;
pub mod region;
pub mod terrain;
pub mod types;

#[allow(unused_imports)] // for the editor state and UI
pub use types::*;
