#[cfg(all(feature = "event-stream", not(target_arch = "wasm32")))] // TODO: spin off parse instead.
pub(crate) mod waker;

#[cfg(not(target_arch = "wasm32"))] // TODO: spin off parse instead.
pub(crate) mod file_descriptor;
pub(crate) mod parse;
