//! Adaptateurs sortants de FineMe : Postgres (sqlx), fournisseurs de cours et de change (reqwest),
//! hachage argon2, jetons JWT et horloge système.

pub mod clock;
pub mod market;
pub mod postgres;
pub mod security;
