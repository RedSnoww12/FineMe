//! Briques partagées par les adaptateurs entrants (`fineme-http` et `fineme-mcp`), pour que l'un ne dépende
//! pas de l'autre : origine de la requête (IP du client derrière le proxy) et forme JSON du contrat
//! (`docs/API.md`), dont l'API REST renvoie les objets complets et les tools MCP des sous-ensembles.

pub mod contract;
pub mod origin;
