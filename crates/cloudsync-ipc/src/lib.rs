//! CloudSync IPC Layer
//!
//! This crate provides the IPC (Inter-Process Communication) server and
//! protocol for communication between the CloudSync daemon and shell
//! extensions (Nautilus, Dolphin, Windows Explorer, Finder).
//!
//! Communication uses newline-delimited JSON over Unix sockets.

pub mod error;
pub mod handler;
pub mod messages;
pub mod path_resolver;
pub mod server;
pub mod transport;

pub use error::{Error, Result};
pub use handler::RequestHandler;
pub use messages::{Request, Response};
pub use path_resolver::{PathResolver, ResolvedPath};
pub use server::{IpcServer, ShutdownHandle};
