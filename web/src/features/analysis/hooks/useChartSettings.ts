import { useCallback, useState } from 'react'
import { parseSettings, type ChartSettings } from '../lib/chartSettings'

function read(key: string): ChartSettings {
  try {
    return parseSettings(localStorage.getItem(key))
  } catch {
    return parseSettings(null)
  }
}

function write(key: string, settings: ChartSettings) {
  try {
    localStorage.setItem(key, JSON.stringify(settings))
  } catch {
    // préférence non mémorisée
  }
}

/** Réglages d'affichage d'un graphique, mémorisés dans `fineme.chart.<id>`. */
export function useChartSettings(id: string) {
  const key = `fineme.chart.${id}`
  const [settings, setSettings] = useState(() => read(key))

  const update = useCallback(
    (change: (current: ChartSettings) => Partial<ChartSettings>) =>
      setSettings((current) => {
        const next = { ...current, ...change(current) }
        write(key, next)
        return next
      }),
    [key],
  )

  return { settings, update }
}

/**
 * Zoom d'un graphique par le `Brush` de Recharts : `brushKey` remonte le Brush (plage complète) à la
 * réinitialisation ou quand le nombre de points change (nouvelle période).
 */
export function useZoom(size: number) {
  const [state, setState] = useState({ key: 0, zoomed: false, size })
  if (state.size !== size) setState({ key: state.key + 1, zoomed: false, size })

  const onChange = useCallback(
    ({ startIndex, endIndex }: { startIndex?: number; endIndex?: number }) =>
      setState((s) => ({ ...s, zoomed: (startIndex ?? 0) > 0 || (endIndex ?? s.size - 1) < s.size - 1 })),
    [],
  )
  const reset = useCallback(() => setState((s) => ({ ...s, key: s.key + 1, zoomed: false })), [])

  return { brushKey: state.key, zoomed: state.zoomed, onChange, reset }
}
