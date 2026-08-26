//! Sandboxed Lua 5.4 processor for captured MIDI events.

mod engine;

pub use engine::{DEFAULT_SOURCE, ScriptEngine, ScriptError};
