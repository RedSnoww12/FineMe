import { Component, type ErrorInfo, type ReactNode } from 'react'
import { Icon } from '@/shared/ui/Icon'
import { Button } from '@/shared/ui/ui'

type Props = { children: ReactNode }
type State = { failed: boolean }

/**
 * Empêche une erreur de rendu de vider tout l'écran : l'écran fautif est remplacé par un message, la
 * navigation reste utilisable. Monté avec une `key` par adresse, il se réarme à chaque changement d'écran.
 */
export class ErrorBoundary extends Component<Props, State> {
  state: State = { failed: false }

  static getDerivedStateFromError(): State {
    return { failed: true }
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error('Erreur d’affichage', error, info.componentStack)
  }

  render() {
    if (!this.state.failed) return this.props.children
    return (
      <div role="alert" className="flex flex-col items-start gap-3 rounded-[14px] border border-neg/40 bg-neg-soft px-5 py-5">
        <div className="flex items-start gap-2.5 text-neg">
          <Icon name="alert" className="mt-0.5 shrink-0" />
          <div>
            <p className="font-semibold">Cet écran n’a pas pu s’afficher</p>
            <p className="mt-0.5 text-ink">
              Une erreur inattendue s’est produite. Rechargez la page ; si le problème persiste, l’API n’est peut-être
              pas à jour.
            </p>
          </div>
        </div>
        <Button size="sm" icon="refresh" onClick={() => window.location.reload()}>
          Recharger la page
        </Button>
      </div>
    )
  }
}
