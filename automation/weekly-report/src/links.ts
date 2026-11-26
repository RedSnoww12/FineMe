// Les libellés importés (un tiers choisit celui de son virement) peuvent pousser le modèle à écrire un lien qui
// emporterait des chiffres : aucun lien venu du modèle ou d'une erreur ne doit rester cliquable dans l'email.

const REMOVED = '[lien retiré]';

// Pas de `\b` : un chiffre ou un « _ » collé devant le schéma suffisait à le contourner. Les textes filtrés sont
// bornés (rapport et cause), ce qui borne aussi le coût de ces motifs.
/** Tout mot contenant `://`, quoi qu'il y ait devant. */
const URL_LIKE = /(?<!\S)\S*:\/\/\S*/gu;
const ACTION_LINK = /(?<!\p{L})(?:mailto|tel|sms):\S+/giu;
/** Le domaine n'a pas besoin de point : mieux vaut retirer trop que laisser une adresse écrite autrement. */
const EMAIL = /(?<![^\s<>()@])[^\s<>()@]+@[^\s<>()@]+/gu;

/** Noms de domaine et adresses IP nus, que les clients mail rendent cliquables (variantes Unicode du point comprises). */
// Les marques (`\p{M}`) font partie d'un nom : accents décomposés, écritures non latines.
const HOST = /(?<![\p{L}\p{M}\p{N}-])(?:[\p{L}\p{M}\p{N}-]+[.。．｡])+\p{L}[\p{L}\p{M}]+|(?:\d{1,3}[.。．｡]){3}\d{1,3}/gu;

export function defangLinks(text: string): string {
  return text
    .replace(URL_LIKE, REMOVED)
    .replace(ACTION_LINK, REMOVED)
    .replace(EMAIL, REMOVED)
    .replace(HOST, (host) => host.replace(/[.。．｡]/gu, '[.]'));
}
