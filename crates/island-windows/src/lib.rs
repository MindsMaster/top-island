pub mod activate;
pub mod appid;
pub mod banner;
pub mod clipboard;
pub mod coreaudio;
pub mod dialog;
pub mod displays;
pub mod error;
pub mod input;
pub mod smtc;
pub mod wide;
pub mod wpn;

pub use error::{Result, WinError};
pub use input::{start_input, HoverChange, InputHandlers, Rect};
