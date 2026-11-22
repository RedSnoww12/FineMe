import type { Overview } from '@/shared/api/types'
import { pct } from '@/shared/lib/format'
import { Panel, SelectField } from '@/shared/ui/ui'
import { currentAllocation, QUESTIONS, targetRiskyShare, type Profile, type SafetyRange } from '../lib/profile'

type Question = keyof typeof QUESTIONS

function optionsOf(question: Question) {
  return Object.entries(QUESTIONS[question].options).map(([value, label]) => ({ value, label }))
}

/** Questionnaire court, fourchette de matelas recommandée et allocation actuelle comparée au profil. */
export function ProfilePanel({
  profile,
  onChange,
  range,
  overview,
}: {
  profile: Profile
  onChange: (profile: Profile) => void
  range: SafetyRange
  overview: Overview | undefined
}) {
  const allocation = overview ? currentAllocation(overview) : null
  const target = targetRiskyShare(profile)
  const select = (question: Question) => (value: string) => onChange({ ...profile, [question]: value })

  return (
    <Panel title="Profil investisseur" className="mb-4 sm:mb-5">
      <div className="grid gap-5 lg:grid-cols-[minmax(0,20rem)_minmax(0,1fr)]">
        <div className="flex flex-col gap-3">
          {(Object.keys(QUESTIONS) as Question[]).map((question) => (
            <SelectField
              key={question}
              label={QUESTIONS[question].label}
              value={profile[question]}
              onChange={select(question)}
              options={optionsOf(question)}
            />
          ))}
          <label className="flex min-h-10 items-center gap-2">
            <input
              type="checkbox"
              className="size-4 accent-[var(--accent)]"
              checked={profile.homeowner}
              onChange={(event) => onChange({ ...profile, homeowner: event.target.checked })}
            />
            Propriétaire de votre logement
          </label>
        </div>
        <div className="flex min-w-0 flex-col gap-4">
          <section aria-label="Matelas de sécurité recommandé">
            <h3 className="font-semibold">
              Matelas recommandé : {range.min} à {range.max} mois de dépenses
            </h3>
            <ul className="mt-2 flex flex-col gap-1 text-[14px] text-muted">
              {range.factors.map((factor) => (
                <li key={factor.label} className="flex justify-between gap-3">
                  <span>{factor.label}</span>
                  <span className="num shrink-0">
                    +{factor.min} à {factor.max} mois
                  </span>
                </li>
              ))}
            </ul>
          </section>
          <section aria-label="Allocation comparée au profil">
            <h3 className="font-semibold">Allocation des placements financiers</h3>
            {allocation ? (
              <p className="mt-1 text-[14px] text-muted">
                Titres et crypto : <span className="num text-ink">{pct(allocation.riskyPct, 0)}</span> aujourd’hui, environ{' '}
                <span className="num text-ink">{pct(target, 0)}</span> pour ce profil ; liquidités :{' '}
                <span className="num text-ink">{pct(allocation.liquidPct, 0)}</span>. Immobilier non compris.
              </p>
            ) : (
              <p className="mt-1 text-[14px] text-muted">Aucun placement financier à comparer (titres, crypto, épargne).</p>
            )}
          </section>
          <p className="rounded-[9px] bg-surface-2 px-3 py-2.5 text-[13.5px] text-muted">
            Indication générale, calculée à partir de vos seules réponses : ce n’est pas un conseil en investissement. Le
            profil reste sur cet appareil.
          </p>
        </div>
      </div>
    </Panel>
  )
}
