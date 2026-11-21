import { Bar, BarChart, CartesianGrid, ResponsiveContainer, Tooltip, XAxis, YAxis } from 'recharts'
import type { BudgetFlows } from '@/shared/api/types'
import { money, moneyCompact, pctFromFraction } from '@/shared/lib/format'
import { AXIS_TICK, Legend, MoneyTooltip } from '@/shared/ui/charts'
import { Panel } from '@/shared/ui/ui'
import { monthLabel, monthShort } from '../lib/budget'

const SERIES = [
  { key: 'income', label: 'Revenus', color: 'var(--pos)' },
  { key: 'expenses', label: 'Dépenses', color: 'var(--c2)' },
] as const

/** Revenus et dépenses des 12 derniers mois, avec un tableau équivalent pour les lecteurs d'écran. */
export function TrendPanel({ trend }: { trend: BudgetFlows[] }) {
  return (
    <Panel title="Évolution sur 12 mois">
      <figure aria-hidden="true" className="h-[240px]">
        <ResponsiveContainer width="100%" height="100%">
          <BarChart data={trend} margin={{ top: 8, right: 4, bottom: 0, left: 0 }} accessibilityLayer={false}>
            <CartesianGrid stroke="var(--line)" vertical={false} />
            <XAxis dataKey="month" tickFormatter={monthShort} tick={AXIS_TICK} tickLine={false} axisLine={{ stroke: 'var(--line)' }} />
            <YAxis tickFormatter={moneyCompact} tick={AXIS_TICK} tickLine={false} axisLine={false} width={64} />
            <Tooltip
              cursor={{ fill: 'var(--surface-2)' }}
              content={(props) => <MoneyTooltip {...props} formatLabel={(label) => monthLabel(String(label))} />}
            />
            {SERIES.map((series) => (
              <Bar key={series.key} dataKey={series.key} name={series.label} fill={series.color} radius={[3, 3, 0, 0]} isAnimationActive={false} />
            ))}
          </BarChart>
        </ResponsiveContainer>
      </figure>
      <Legend items={SERIES.map((series) => ({ label: series.label, color: series.color }))} />
      <details className="mt-3 text-[14px]">
        <summary className="cursor-pointer text-muted">Voir le détail mois par mois</summary>
        <div className="table-wrap mt-2">
          <table className="table">
            <caption className="sr-only">Revenus, dépenses et épargne des 12 derniers mois</caption>
            <thead>
              <tr>
                <th>Mois</th>
                <th className="r">Revenus</th>
                <th className="r">Dépenses</th>
                <th className="r">Épargne</th>
                <th className="r">Taux d’épargne</th>
              </tr>
            </thead>
            <tbody>
              {trend.map((row) => (
                <tr key={row.month}>
                  <td>{monthLabel(row.month)}</td>
                  <td className="r num">{money(row.income)}</td>
                  <td className="r num">{money(row.expenses)}</td>
                  <td className="r num">{money(row.savings, { signed: true })}</td>
                  <td className="r num">{pctFromFraction(row.savings_rate, 0)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </details>
    </Panel>
  )
}
