//! Cas d'usage de FineMe et ports (traits) vers l'extérieur.
//! Cette couche orchestre le domaine ; elle ne connaît ni SQL, ni HTTP, ni JSON.

pub mod access_tokens;
pub mod accounts;
pub mod alerts;
pub mod app;
pub mod auth;
pub mod budget;
pub mod bundle;
pub mod commands;
pub mod connections;
pub mod daily;
pub mod error;
pub mod goals;
pub mod history;
pub mod holdings;
pub mod import_profiles;
pub mod members;
pub mod operations;
pub mod overview;
pub mod performance;
pub mod ports;
pub mod prices;
pub mod reader;
pub mod real_estate;
mod secret;
pub mod totp;
pub mod transactions;

#[cfg(test)]
pub(crate) mod testing;

pub use app::{App, Ports, Settings};
pub use error::{AppError, AppResult};
