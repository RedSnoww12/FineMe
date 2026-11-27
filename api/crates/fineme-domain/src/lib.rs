//! Domaine FineMe : entités, value objects et règles financières pures.
//! Aucune E/S ici — ni SQL, ni JSON, ni réseau, ni horloge système.

pub mod access_token;
pub mod account;
pub mod alert;
pub mod budget;
pub mod connection;
pub mod error;
pub mod fx;
pub mod goal;
pub mod history;
pub mod holding;
pub mod ids;
pub mod import_profile;
pub mod loan;
pub mod login;
pub mod member;
pub mod oauth;
pub mod operation;
pub mod ownership;
pub mod performance;
pub mod portfolio;
pub mod property;
pub mod throttle;
pub mod timeline;
pub mod totp;
pub mod transaction;
pub mod valuation;

pub use error::DomainError;
