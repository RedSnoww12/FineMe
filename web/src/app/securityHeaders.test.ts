import { describe, expect, it } from 'vitest'
import indexHtml from '../../index.html?raw'
import vercelConfig from '../../vercel.json?raw'

type VercelConfig = { headers: { source: string; headers: { key: string; value: string }[] }[] }

function header(name: string): string {
  const config = JSON.parse(vercelConfig) as VercelConfig
  const found = config.headers.flatMap((rule) => rule.headers).find((h) => h.key === name)
  if (!found) throw new Error(`En-tête ${name} absent de vercel.json`)
  return found.value
}

async function sha256Base64(text: string): Promise<string> {
  const digest = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(text))
  return btoa(String.fromCharCode(...new Uint8Array(digest)))
}

describe('en-têtes de sécurité Vercel', () => {
  it('autorisent le script en ligne du thème par son empreinte, et lui seul', async () => {
    const inline = [...indexHtml.matchAll(/<script>([\s\S]*?)<\/script>/g)].map((match) => match[1])
    expect(inline).toHaveLength(1)
    const csp = header('Content-Security-Policy')
    // Modifier le script de index.html impose de recopier sa nouvelle empreinte dans vercel.json.
    expect(csp).toContain(`'sha256-${await sha256Base64(inline[0])}'`)
    expect(csp).not.toMatch(/script-src[^;]*'unsafe-inline'/)
  })

  it('limitent les appels réseau au site et à l’API Render, et interdisent l’intégration en cadre', () => {
    const csp = header('Content-Security-Policy')
    expect(csp).toContain("connect-src 'self' https://*.onrender.com")
    expect(csp).toContain("frame-ancestors 'none'")
    // PWA : service worker (sw.js, registerSW.js) et manifeste servis par le site lui-même.
    expect(csp).toContain("worker-src 'self'")
    expect(csp).toContain("manifest-src 'self'")
    expect(header('X-Frame-Options')).toBe('DENY')
    expect(header('X-Content-Type-Options')).toBe('nosniff')
    expect(header('Referrer-Policy')).toBe('strict-origin-when-cross-origin')
    expect(header('Permissions-Policy')).toContain('camera=()')
  })
})
