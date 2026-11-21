//! Domaine FineMe : entités, value objects et règles financières pures.
//! Aucune E/S ici — ni SQL, ni JSON, ni réseau, ni horloge système.

pub mod account;
pub mod connection;
pub mod error;
pub mod fx;
pub mod goal;
pub mod history;
pub mod holding;
pub mod ids;
pub mod loan;
pub mod login;
pub mod member;
pub mod ownership;
pub mod portfolio;
pub mod property;
pub mod throttle;
pub mod transaction;

pub use error::DomainError;
