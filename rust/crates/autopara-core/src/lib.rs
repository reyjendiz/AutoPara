//! AutoPara's core: everything that is not window or tray.
//!
//! The behaviour it ports is described in `rust/PORT_SPEC.md`.

pub mod days;
pub mod db;
pub mod docx;
pub mod i18n;
pub mod import;
pub mod legacy;
pub mod model;
pub mod parser;
pub mod text;
pub mod time;
