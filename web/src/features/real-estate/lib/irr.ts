// Valeur actuelle nette et taux de rendement interne. Fonctions pures.

/** VAN de flux périodiques (le flux d'indice 0 n'est pas actualisé) au taux périodique `rate`. */
export function npv(rate: number, flows: number[]): number {
  return flows.reduce((sum, flow, t) => sum + flow / Math.pow(1 + rate, t), 0)
}

const LOW = -0.9999
const HIGH = 100

/**
 * Taux périodique qui annule la VAN, par dichotomie. `null` quand il n'existe pas de changement de signe
 * (par exemple aucun capital investi au départ, ou des flux tous du même signe).
 */
export function irr(flows: number[]): number | null {
  if (flows.length < 2) return null
  const hasNegative = flows.some((flow) => flow < 0)
  const hasPositive = flows.some((flow) => flow > 0)
  if (!hasNegative || !hasPositive) return null
  let low = LOW
  let high = HIGH
  let fLow = npv(low, flows)
  const fHigh = npv(high, flows)
  if (Math.sign(fLow) === Math.sign(fHigh)) return null
  for (let i = 0; i < 200; i++) {
    const mid = (low + high) / 2
    const fMid = npv(mid, flows)
    if (Math.abs(fMid) < 1e-9 || high - low < 1e-12) return mid
    if (Math.sign(fMid) === Math.sign(fLow)) {
      low = mid
      fLow = fMid
    } else {
      high = mid
    }
  }
  return (low + high) / 2
}
