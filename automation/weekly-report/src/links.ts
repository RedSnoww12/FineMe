// Les libellés importés (un tiers choisit celui de son virement) peuvent pousser le modèle à écrire un lien qui
// emporterait des chiffres : aucun lien venu du modèle ou d'une erreur ne doit rester cliquable dans l'email.

const REMOVED = '[lien retiré]';

/** Adresses avec schéma, liens `mailto:`, `tel:`, `sms:` et adresses email : retirés. */
const ADDRESS = /\b[a-z][a-z0-9+.-]*:\/\/\S+|\b(?:mailto|tel|sms):\S+|[^\s<>()@]+@[^\s<>()@]+\.[^\s<>()@]+/giu;

/** Noms de domaine et adresses IP nus, que les clients mail rendent cliquables : points désamorcés. */
const HOST = /(?:[\p{L}\p{N}-]+\.)+\p{L}{2,}|(?:\d{1,3}\.){3}\d{1,3}/gu;

export function defangLinks(text: string): string {
  return text.replace(ADDRESS, REMOVED).replace(HOST, (host) => host.replaceAll('.', '[.]'));
}
