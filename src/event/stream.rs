#[cfg(not(target_arch = "wasm32"))]
mod not_wasm;

#[cfg(not(target_arch = "wasm32"))]
#[doc(inline)]
pub use not_wasm::EventStream;

#[cfg(target_arch = "wasm32")]
mod wasm;

#[cfg(target_arch = "wasm32")]
#[doc(inline)]
pub use wasm::EventStream;
