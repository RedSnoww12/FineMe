import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { BrowserRouter } from 'react-router'
import App from '@/app/App'
import { Providers } from '@/app/Providers'
import { createQueryClient } from '@/app/queryClient'
import { createCachePersister } from '@/shared/api/persistence'
import './index.css'

createRoot(document.getElementById('root') as HTMLElement).render(
  <StrictMode>
    <Providers queryClient={createQueryClient()} persister={createCachePersister()}>
      <BrowserRouter>
        <App />
      </BrowserRouter>
    </Providers>
  </StrictMode>,
)
