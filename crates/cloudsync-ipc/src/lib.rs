//! CloudSync IPC Layer
//!
//! This crate provides the IPC (Inter-Process Communication) server and
//! protocol for communication between the CloudSync daemon and shell
//! extensions (Nautilus, Dolphin, Windows Explorer, Finder).
//!
//! Communication uses JSON over Unix sockets (Linux/macOS) or named
//! pipes (Windows).
//!
//! Full implementation in Phase 3.1.

pub mod messages;

pub use messages::{Request, Response};

#[cfg(test)]
mod tests {
    #[test]
    fn placeholder_test() {
        assert!(true);
    }
}
