import { describe, expect, it } from 'vitest'
import { groupSecret, qrDataUri, recoveryCodesText } from './totp'

describe('second facteur', () => {
  it('encode le SVG du QR code en data URI', () => {
    const uri = qrDataUri('<svg xmlns="http://www.w3.org/2000/svg"><rect fill="#000"/></svg>')
    expect(uri.startsWith('data:image/svg+xml;charset=utf-8,')).toBe(true)
    expect(uri).not.toContain('<')
    expect(uri).not.toContain('#')
    expect(decodeURIComponent(uri.split(',')[1])).toContain('<rect fill="#000"/>')
  })

  it('groupe le secret par quatre caractères', () => {
    expect(groupSecret('GEZDGNBVGY3TQOJQ')).toBe('GEZD GNBV GY3T QOJQ')
    expect(groupSecret('GEZDG')).toBe('GEZD G')
    expect(groupSecret('')).toBe('')
  })

  it('liste les codes de secours un par ligne', () => {
    expect(recoveryCodesText(['abcde-fgh89', 'zzzzz-11111'])).toBe(
      'Codes de secours FineMe (usage unique)\n\nabcde-fgh89\nzzzzz-11111\n',
    )
  })
})
