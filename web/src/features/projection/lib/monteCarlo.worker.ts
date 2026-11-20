// Exécute la simulation hors du fil principal : l'interface reste fluide pendant le calcul.
import { simulateMonteCarlo, type MonteCarloInput, type MonteCarloResult } from './monteCarlo'

export type WorkerRequest = { id: number; input: MonteCarloInput }
export type WorkerResponse = { id: number; result: MonteCarloResult }

self.onmessage = (event: MessageEvent<WorkerRequest>) => {
  const { id, input } = event.data
  const result = simulateMonteCarlo(input)
  const response: WorkerResponse = { id, result }
  // Le tableau des capitaux finaux est transféré, pas copié.
  self.postMessage(response, { transfer: [result.finalValues.buffer] })
}
