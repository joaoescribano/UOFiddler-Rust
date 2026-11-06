#![allow(dead_code)] // parts used only by the map editor
//! The map: an isometric view of the facet with the classic client's art, a radar overview, and
//! the map editor that writes changes back to the client files.

pub mod assets;
pub mod atlas;
pub mod capture;
pub mod heights;
pub mod iso;
pub mod panel;
pub mod scene;
pub mod view;

pub use panel::{MapPanel, MapRes};
