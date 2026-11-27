//! Adaptateurs sortants de FineMe : Postgres (sqlx), fournisseurs de cours et de change (reqwest),
//! lecture des métadonnées des clients OAuth, hachage argon2, jetons JWT, primitives du second facteur et
//! horloge système.

pub mod access_token;
pub mod client_metadata;
pub mod clock;
pub mod market;
pub mod postgres;
pub mod security;
pub mod totp;
