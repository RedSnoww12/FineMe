import { useEffect, useMemo, useRef, useState } from 'react'
import { simulateMonteCarlo, type MonteCarloInput, type MonteCarloResult } from '../lib/monteCarlo'
import type { WorkerRequest, WorkerResponse } from '../lib/monteCarlo.worker'

/** Délai après la dernière saisie avant de relancer la simulation. */
const DEBOUNCE_MS = 200

const workerAvailable = typeof Worker !== 'undefined'

type Computed = { key: string; result: MonteCarloResult }

/**
 * Lance la simulation dans un Web Worker et renvoie le dernier résultat reçu. `computing` est vrai tant que le
 * résultat affiché ne correspond pas encore aux hypothèses courantes. Sans Worker (tests jsdom, navigateur
 * ancien) ou si le Worker échoue, le calcul se fait directement dans le fil principal.
 */
export function useMonteCarlo(input: MonteCarloInput): { result: MonteCarloResult | null; computing: boolean } {
  const key = JSON.stringify(input)
  const workerRef = useRef<Worker | null>(null)
  const latest = useRef<{ id: number; key: string }>({ id: 0, key: '' })
  const [computed, setComputed] = useState<Computed | null>(null)
  const [workerFailed, setWorkerFailed] = useState(!workerAvailable)

  useEffect(() => {
    if (workerFailed) return
    const worker = new Worker(new URL('../lib/monteCarlo.worker.ts', import.meta.url), { type: 'module' })
    worker.onmessage = (event: MessageEvent<WorkerResponse>) => {
      // Une réponse à des hypothèses déjà remplacées est ignorée.
      if (event.data.id === latest.current.id) setComputed({ key: latest.current.key, result: event.data.result })
    }
    worker.onerror = () => setWorkerFailed(true)
    workerRef.current = worker
    return () => {
      worker.terminate()
      workerRef.current = null
    }
  }, [workerFailed])

  useEffect(() => {
    const worker = workerRef.current
    if (!worker) return
    const timer = setTimeout(() => {
      const request: WorkerRequest = { id: latest.current.id + 1, input: JSON.parse(key) as MonteCarloInput }
      latest.current = { id: request.id, key }
      worker.postMessage(request)
    }, DEBOUNCE_MS)
    return () => clearTimeout(timer)
  }, [key, workerFailed])

  const direct = useMemo(
    () => (workerFailed ? simulateMonteCarlo(JSON.parse(key) as MonteCarloInput) : null),
    [key, workerFailed],
  )

  if (direct) return { result: direct, computing: false }
  return { result: computed?.result ?? null, computing: computed?.key !== key }
}
