import { http, HttpResponse, type JsonBodyType } from 'msw'
import { setupServer } from 'msw/node'

/** Adresse de l'API simulée : `VITE_API_URL` du projet de test « integration ». */
export const API = 'http://api.test'

export const server = setupServer()

type Method = 'get' | 'post' | 'put' | 'delete'

/** Répond en JSON à `METHOD /api/...` pour le test en cours. */
export function mockJson(method: Method, path: string, body: JsonBodyType, status = 200) {
  server.use(http[method](`${API}${path}`, () => HttpResponse.json(body, { status })))
}

/** Erreur de l'API au format du contrat : `{ "error": "..." }`. */
export function mockApiError(method: Method, path: string, status: number, error: string) {
  mockJson(method, path, { error }, status)
}

/** Simule un serveur injoignable (coupure réseau). */
export function mockUnreachable(method: Method, path: string) {
  server.use(http[method](`${API}${path}`, () => HttpResponse.error()))
}

/** Compte les appels reçus sur une route, pour vérifier qu'un formulaire invalide n'envoie rien. */
export function spyOn(method: Method, path: string) {
  const calls = { count: 0 }
  server.use(
    http[method](`${API}${path}`, () => {
      calls.count += 1
      return HttpResponse.json({ error: 'Appel inattendu.' }, { status: 500 })
    }),
  )
  return calls
}

export { http, HttpResponse }
