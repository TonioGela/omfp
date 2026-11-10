//! The host glue between `omfp-client` and the app (04-ui.md, "State and code structure"): the client's model becomes
//! the [`crate::ViewInput`] at most once per animation frame ([`frame`], UI-012, [`view`]), every [`crate::Intent`]
//! becomes protocol calls ([`dispatch`]), and on the web the access token comes from the URL fragment or the user
//! ([`token`], UI-002). Everything but the browser bindings ([`web`]) compiles and is tested natively.

pub mod dispatch;
pub mod frame;
pub mod token;
pub mod view;
#[cfg(target_arch = "wasm32")]
pub mod web;
