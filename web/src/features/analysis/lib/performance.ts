// Performance latente (depuis l'achat) par compte et par position, à la part détenue par la vue.
import type { Account, AccountDetail } from '@/shared/api/types'

/** Part d'un compte détenue par la vue : somme des parts pour le foyer, part du membre sinon. */
export function viewFraction(owners: Account['owners'], memberId: string | null): number {
  return owners
    .filter((owner) => memberId === null || owner.member_id === memberId)
    .reduce((sum, owner) => sum + owner.share, 0)
}

export type PerformanceRow = {
  key: string
  label: string
  /** Précision affichée sous le libellé : établissement ou compte de la position. */
  detail: string | null
  gain: number
  invested: number
  /** Plus-value rapportée au prix de revient ; indépendante de la part détenue. */
  pct: number | null
}

function row(key: string, label: string, detail: string | null, gain: number, invested: number, fraction: number): PerformanceRow {
  return {
    key,
    label,
    detail,
    gain: gain * fraction,
    invested: invested * fraction,
    pct: invested > 0 ? (gain / invested) * 100 : null,
  }
}

function byGainDesc(a: PerformanceRow, b: PerformanceRow): number {
  return b.gain - a.gain
}

/** Comptes d'actifs dont le prix de revient est connu, de la meilleure à la moins bonne plus-value. */
export function accountPerformance(accounts: readonly Account[], memberId: string | null): PerformanceRow[] {
  return accounts
    .filter((account) => !account.is_liability && account.gain !== null && account.invested !== null)
    .map((account) => ({ account, fraction: viewFraction(account.owners, memberId) }))
    .filter(({ fraction }) => fraction > 0)
    .map(({ account, fraction }) =>
      row(account.id, account.name, account.institution, account.gain ?? 0, account.invested ?? 0, fraction),
    )
    .sort(byGainDesc)
}

/** Positions dont le PRU est connu, tous comptes confondus. */
export function holdingPerformance(details: readonly AccountDetail[], memberId: string | null): PerformanceRow[] {
  return details
    .flatMap((detail) => {
      const fraction = viewFraction(detail.owners, memberId)
      if (fraction <= 0) return []
      return detail.holdings
        .filter((holding) => holding.gain !== null && holding.invested !== null)
        .map((holding) => row(holding.id, holding.name, detail.name, holding.gain ?? 0, holding.invested ?? 0, fraction))
    })
    .sort(byGainDesc)
}

/** Comptes susceptibles de porter des positions (titres, fonds, crypto). */
export function holdsPositions(account: Account): boolean {
  return account.kind !== 'real_estate' && account.kind !== 'loan'
}
