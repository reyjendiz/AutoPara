//! Commands the interface can call. Every type crossing the boundary derives `TS`, and
//! `cargo test` writes its TypeScript twin into `ui/src/bindings`, so both sides share one shape.

use serde::Serialize;
use ts_rs::TS;

/// The answer to `ping`: proof that the round trip works, and which build answered.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct Pong {
    pub message: String,
    pub version: String,
}

#[tauri::command]
pub fn ping() -> Pong {
    Pong {
        message: "pong".into(),
        version: env!("CARGO_PKG_VERSION").into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ping_reports_this_build() {
        let pong = ping();
        assert_eq!(pong.message, "pong");
        assert_eq!(pong.version, env!("CARGO_PKG_VERSION"));
    }
}
