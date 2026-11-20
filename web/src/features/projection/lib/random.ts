// Générateurs pseudo-aléatoires à graine fixe : la même graine produit toujours la même suite,
// ce qui rend les simulations reproductibles (tests, nouveau rendu de l'écran).

/** Fonction renvoyant un nombre uniforme dans [0, 1). */
export type Random = () => number

/** Mulberry32 : rapide, 32 bits d'état, largement suffisant pour une simulation de patrimoine. */
export function seededRandom(seed: number): Random {
  let state = seed >>> 0
  return () => {
    state = (state + 0x6d2b79f5) >>> 0
    let t = state
    t = Math.imul(t ^ (t >>> 15), t | 1)
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61)
    return ((t ^ (t >>> 14)) >>> 0) / 4_294_967_296
  }
}

/**
 * Tirages d'une loi normale centrée réduite (Box-Muller) à partir d'un générateur uniforme.
 * Chaque paire de tirages uniformes donne deux valeurs normales : la seconde est gardée pour l'appel suivant.
 */
export function standardNormal(random: Random): Random {
  let spare: number | null = null
  return () => {
    if (spare !== null) {
      const value = spare
      spare = null
      return value
    }
    // 1 - u est dans ]0, 1] : le logarithme reste fini.
    const radius = Math.sqrt(-2 * Math.log(1 - random()))
    const angle = 2 * Math.PI * random()
    spare = radius * Math.sin(angle)
    return radius * Math.cos(angle)
  }
}
