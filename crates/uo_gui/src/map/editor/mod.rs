//! The map editor: the full-screen mode over the isometric view (`state` holds the state and the
//! tools, `chrome` the panels around the map, `overlay` what is drawn over it), and the rules that
//! need neither the map nor the screen: the brush, the ground height tools, undo and redo, the
//! region clipboard and templates, the visible height band, the terrain brush with its automatic
//! borders and the border sets.

mod art;
pub mod band;
mod bar;
mod border_dialog;
pub mod borders;
pub mod brush;
mod brush_card;
pub mod chrome;
pub mod height;
pub mod history;
pub mod overlay;
pub mod region;
pub mod state;
mod storage;
pub mod terrain;
pub mod text;
pub mod types;

pub use chrome::Request;
pub use state::{MapEditor, Pointer};
