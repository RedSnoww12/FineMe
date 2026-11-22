// Vue consolidée d'un actif détenu sur plusieurs comptes, à la part détenue par la vue.
import type { AccountDetail, Holding } from '@/shared/api/types'

/** Identifiant d'un actif tous comptes confondus : ISIN, sinon symbole, sinon nom (casse et espaces ignorés). */
export function assetKey(holding: Pick<Holding, 'isin' | 'symbol' | 'name'>): string {
  const isin = holding.isin?.trim()
  if (isin) return `isin:${isin.toUpperCase()}`
  const symbol = holding.symbol?.trim()
  if (symbol) return `symbol:${symbol.toUpperCase()}`
  return `name:${holding.name.trim().replace(/\s+/g, ' ').toLowerCase()}`
}

export type AssetLine = {
  holding: Holding
  accountId: string
  accountName: string
  /** Part du compte détenue par la vue. */
  fraction: number
  quantity: number
  value: number
}

export type Asset = {
  key: string
  name: string
  isin: string | null
  symbol: string | null
  quantity: number
  value: number
  /** Prix de revient des lignes au PRU connu ; null si aucune. */
  invested: number | null
  gain: number | null
  gainPct: number | null
  /** PRU moyen pondéré par les quantités, devise des lignes ; null sans PRU ou si les devises diffèrent. */
  averageBuyPrice: number | null
  currency: string
  lines: AssetLine[]
  /** Valeur détenue par membre, d'après les parts de chaque compte. */
  members: { memberId: string; name: string; value: number }[]
}

function viewFraction(owners: AccountDetail['owners'], memberId: string | null): number {
  return owners.filter((owner) => memberId === null || owner.member_id === memberId).reduce((sum, owner) => sum + owner.share, 0)
}

function sum<T>(items: readonly T[], value: (item: T) => number): number {
  return items.reduce((total, item) => total + value(item), 0)
}

function averageBuyPrice(lines: readonly AssetLine[]): number | null {
  const priced = lines.filter((line) => line.holding.buy_price !== null && line.holding.quantity > 0)
  const currencies = new Set(lines.map((line) => line.holding.currency))
  if (priced.length === 0 || currencies.size > 1) return null
  const quantity = sum(priced, (line) => line.holding.quantity)
  return sum(priced, (line) => line.holding.quantity * (line.holding.buy_price ?? 0)) / quantity
}

function toAsset(key: string, lines: AssetLine[], details: readonly AccountDetail[], memberId: string | null): Asset {
  const first = lines[0].holding
  const known = lines.filter((line) => line.holding.invested !== null)
  const invested = known.length > 0 ? sum(known, (line) => (line.holding.invested ?? 0) * line.fraction) : null
  const gain = known.length > 0 ? sum(known, (line) => (line.holding.gain ?? 0) * line.fraction) : null

  const members = new Map<string, { memberId: string; name: string; value: number }>()
  for (const line of lines) {
    const owners = details.find((detail) => detail.id === line.accountId)?.owners ?? []
    for (const owner of owners) {
      if (memberId !== null && owner.member_id !== memberId) continue
      const current = members.get(owner.member_id) ?? { memberId: owner.member_id, name: owner.name, value: 0 }
      members.set(owner.member_id, { ...current, value: current.value + line.holding.value * owner.share })
    }
  }

  return {
    key,
    name: first.name,
    isin: lines.find((line) => line.holding.isin)?.holding.isin ?? null,
    symbol: lines.find((line) => line.holding.symbol)?.holding.symbol ?? null,
    quantity: sum(lines, (line) => line.quantity),
    value: sum(lines, (line) => line.value),
    invested,
    gain,
    gainPct: invested !== null && invested > 0 && gain !== null ? (gain / invested) * 100 : null,
    averageBuyPrice: averageBuyPrice(lines),
    currency: first.currency,
    lines: [...lines].sort((a, b) => b.value - a.value),
    members: [...members.values()].sort((a, b) => b.value - a.value),
  }
}

/** Actifs détenus par la vue, une ligne par actif tous comptes confondus, du plus gros au plus petit. */
export function consolidate(details: readonly AccountDetail[], memberId: string | null): Asset[] {
  const groups = new Map<string, AssetLine[]>()
  for (const detail of details) {
    const fraction = viewFraction(detail.owners, memberId)
    if (fraction <= 0) continue
    for (const holding of detail.holdings) {
      const line = {
        holding,
        accountId: detail.id,
        accountName: detail.name,
        fraction,
        quantity: holding.quantity * fraction,
        value: holding.value * fraction,
      }
      const key = assetKey(holding)
      groups.set(key, [...(groups.get(key) ?? []), line])
    }
  }
  return [...groups.entries()].map(([key, lines]) => toAsset(key, lines, details, memberId)).sort((a, b) => b.value - a.value)
}
