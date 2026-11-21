import { describe, expect, it } from 'vitest'
import { columnName, crc32, escapeXml, excelSerial, toXlsx, zip } from './xlsx'

const encoder = new TextEncoder()
const decoder = new TextDecoder()

/** Relit une archive ZIP stockée (méthode 0) via son répertoire central. */
function unzip(bytes: Uint8Array): Map<string, string> {
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength)
  const end = bytes.length - 22
  expect(view.getUint32(end, true)).toBe(0x06054b50)
  const count = view.getUint16(end + 10, true)
  let at = view.getUint32(end + 16, true)
  const files = new Map<string, string>()
  for (let i = 0; i < count; i += 1) {
    expect(view.getUint32(at, true)).toBe(0x02014b50)
    const size = view.getUint32(at + 20, true)
    const nameLength = view.getUint16(at + 28, true)
    const local = view.getUint32(at + 42, true)
    const name = decoder.decode(bytes.subarray(at + 46, at + 46 + nameLength))
    const dataStart = local + 30 + view.getUint16(local + 26, true)
    const data = bytes.subarray(dataStart, dataStart + size)
    expect(crc32(data)).toBe(view.getUint32(at + 16, true))
    files.set(name, decoder.decode(data))
    at += 46 + nameLength
  }
  return files
}

describe('briques du classeur', () => {
  it('calcule le CRC-32 standard', () => {
    expect(crc32(encoder.encode('123456789'))).toBe(0xcbf43926)
    expect(crc32(new Uint8Array())).toBe(0)
  })

  it('nomme les colonnes comme Excel', () => {
    expect(columnName(0)).toBe('A')
    expect(columnName(25)).toBe('Z')
    expect(columnName(26)).toBe('AA')
    expect(columnName(27)).toBe('AB')
  })

  it('convertit une date en numéro de série Excel', () => {
    expect(excelSerial('1900-03-01')).toBe(61)
    expect(excelSerial('2026-01-01')).toBe(46023)
    expect(excelSerial('pas une date')).toBeNull()
  })

  it('échappe le XML et retire les caractères de contrôle', () => {
    expect(escapeXml('A & B <c> "d"\u0001')).toBe('A &amp; B &lt;c&gt; &quot;d&quot;')
  })

  it('produit une archive relisible', () => {
    const files = unzip(zip([{ path: 'a.txt', data: encoder.encode('été') }]))
    expect(files.get('a.txt')).toBe('été')
  })
})

describe('toXlsx', () => {
  const bytes = toXlsx([
    { name: 'Comptes', columns: ['Compte', 'Valeur', 'Date'], rows: [['Livret <A> & B', 1234.5, { date: '2026-01-01' }], [null, -3, null]] },
    { name: 'Positions: titres', columns: ['Nom'], rows: [] },
  ])
  const files = unzip(bytes)

  it('contient les parties requises par le format et un onglet par tableau', () => {
    expect([...files.keys()].sort()).toEqual([
      '[Content_Types].xml',
      '_rels/.rels',
      'xl/_rels/workbook.xml.rels',
      'xl/styles.xml',
      'xl/workbook.xml',
      'xl/worksheets/sheet1.xml',
      'xl/worksheets/sheet2.xml',
    ])
    expect(files.get('xl/workbook.xml')).toContain('<sheet name="Comptes" sheetId="1" r:id="rId1"/>')
    // `:` est interdit dans un nom d'onglet.
    expect(files.get('xl/workbook.xml')).toContain('<sheet name="Positions  titres"')
  })

  it('écrit les nombres en natif, les dates au format date et les textes échappés', () => {
    const sheet = files.get('xl/worksheets/sheet1.xml') ?? ''
    expect(sheet).toContain('<c r="A2" t="inlineStr"><is><t xml:space="preserve">Livret &lt;A&gt; &amp; B</t></is></c>')
    expect(sheet).toContain('<c r="B2"><v>1234.5</v></c>')
    expect(sheet).toContain('<c r="C2" s="1"><v>46023</v></c>')
    expect(sheet).toContain('<row r="3"><c r="B3"><v>-3</v></c></row>')
  })
})
