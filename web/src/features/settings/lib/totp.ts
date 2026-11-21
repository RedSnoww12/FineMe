/** Image du QR code à partir du document SVG renvoyé par l'API (`data:` URI, autorisé par la CSP `img-src`).
 * Affiché dans une balise `<img>` : un SVG y est inerte, aucun script ne peut s'exécuter. */
export function qrDataUri(svg: string): string {
  return `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`
}

/** Secret base32 par groupes de 4 pour la saisie manuelle : « GEZD GNBV GY3T ». */
export function groupSecret(secret: string): string {
  return secret.match(/.{1,4}/g)?.join(' ') ?? ''
}

/** Codes de secours en texte, un par ligne, pour les copier ou les enregistrer. */
export function recoveryCodesText(codes: string[]): string {
  return `Codes de secours FineMe (usage unique)\n\n${codes.join('\n')}\n`
}
