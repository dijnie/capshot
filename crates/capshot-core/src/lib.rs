//! capshot-core: everything that decides what a screenshot looks like.
//!
//! The editor preview and the exported file both come from [`render::render`],
//! so what the user sees is what gets saved.

pub mod annotation;
pub mod background;
pub mod color;
pub mod config;
pub mod crop;
pub mod document;
pub mod edge_color;
pub mod edit;
pub mod export;
pub mod geometry;
pub mod history;
pub mod icon;
pub mod library;
pub mod presets;
pub mod render;

pub use annotation::{Annotation, AnnotationKind, FontStyle, TextAlign};
pub use document::{Document, EditorSettings};
pub use geometry::{Point, Rect};
