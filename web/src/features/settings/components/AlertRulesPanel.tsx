import { useState, type FormEvent } from 'react'
import { useAlertRules, useUpdateAlertRule } from '../hooks/useAlertRules'
import { ALERT_RULE_ORDER, ALERT_RULE_TEXT, hasCeiling, hasThreshold, readRuleForm, type RuleFormErrors } from '../lib/alertRules'
import type { AlertRule } from '@/shared/api/types'
import { toInput } from '@/shared/lib/format'
import { Button, ErrorState, FormError, LoadingBlock, Notice, NumberField, Panel } from '@/shared/ui/ui'

function RuleThresholds({ rule }: { rule: AlertRule }) {
  const [threshold, setThreshold] = useState(toInput(rule.threshold_pct))
  const [ceiling, setCeiling] = useState(toInput(rule.ceiling))
  const [errors, setErrors] = useState<RuleFormErrors>({})
  const [saved, setSaved] = useState(false)
  const update = useUpdateAlertRule(() => setSaved(true))
  const text = ALERT_RULE_TEXT[rule.kind]

  const submit = (event: FormEvent) => {
    event.preventDefault()
    setSaved(false)
    const result = readRuleForm(rule, { threshold, ceiling })
    if (result.errors) {
      setErrors(result.errors)
      return
    }
    setErrors({})
    update.mutate({ kind: rule.kind, input: result.input })
  }

  return (
    <form onSubmit={submit} noValidate aria-label={`Seuils : ${text.label}`} className="flex flex-col gap-3 pl-7">
      <div className="grid gap-3 sm:grid-cols-[repeat(auto-fit,minmax(12rem,1fr))]">
        {hasThreshold(rule) ? (
          <NumberField
            label={rule.kind === 'savings_ceiling' ? 'Seuil (en % du plafond)' : 'Seuil de variation'}
            suffix="%"
            value={threshold}
            onChange={setThreshold}
            error={errors.threshold}
          />
        ) : null}
        {hasCeiling(rule) ? (
          <NumberField
            label="Plafond des livrets"
            suffix="€"
            value={ceiling}
            onChange={setCeiling}
            error={errors.ceiling}
            hint="Appliqué à chaque compte d’épargne."
          />
        ) : null}
      </div>
      <div className="flex flex-wrap items-center gap-3">
        <Button size="sm" type="submit" busy={update.isPending}>
          Enregistrer les seuils
        </Button>
        {saved ? <Notice tone="pos">Seuils enregistrés.</Notice> : null}
      </div>
      <FormError error={update.error} />
    </form>
  )
}

function RuleRow({ rule }: { rule: AlertRule }) {
  const update = useUpdateAlertRule()
  const text = ALERT_RULE_TEXT[rule.kind]
  const descriptionId = `alert-rule-${rule.kind}-description`
  return (
    <li className="flex flex-col gap-3 py-3.5">
      <label className="flex cursor-pointer items-start gap-3">
        <input
          type="checkbox"
          checked={rule.enabled}
          disabled={update.isPending}
          aria-describedby={descriptionId}
          onChange={(event) => update.mutate({ kind: rule.kind, input: { enabled: event.target.checked } })}
          className="mt-1 size-4 shrink-0 accent-[var(--accent)]"
        />
        <span className="flex flex-col gap-0.5">
          <span className="font-medium">{text.label}</span>
          <span id={descriptionId} className="text-[14px] text-muted">
            {text.description}
          </span>
        </span>
      </label>
      {rule.enabled && (hasThreshold(rule) || hasCeiling(rule)) ? <RuleThresholds rule={rule} /> : null}
      <FormError error={update.error} />
    </li>
  )
}

/** Règles d'alerte : activer, désactiver et régler les seuils. */
export function AlertRulesPanel() {
  const rules = useAlertRules()
  const ordered = rules.data
    ? ALERT_RULE_ORDER.flatMap((kind) => rules.data.filter((rule) => rule.kind === kind))
    : []

  return (
    <div id="alertes" className="scroll-mt-20">
      <Panel title="Alertes et rappels">
        {rules.isPending ? (
          <LoadingBlock rows={2} label="Chargement des alertes…" />
        ) : rules.isError ? (
          <ErrorState error={rules.error} onRetry={() => void rules.refetch()} />
        ) : ordered.length === 0 ? (
          <p className="text-muted">Aucune règle d’alerte disponible.</p>
        ) : (
          <div className="flex flex-col gap-1">
            <p className="text-muted">
              Les règles sont vérifiées chaque matin et à la demande depuis les notifications.
            </p>
            <ul aria-label="Règles d’alerte" className="flex flex-col divide-y divide-line">
              {ordered.map((rule) => (
                <RuleRow key={rule.kind} rule={rule} />
              ))}
            </ul>
          </div>
        )}
      </Panel>
    </div>
  )
}
