import { describe, expect, it } from 'vitest';
import { defangLinks } from './links.ts';

describe('liens dans les emails', () => {
  it.each([
    ['voir https://evil.example/?n=10500 ici', 'voir [lien retiré] ici'],
    ['HTTP://1.2.3.4/x', '[lien retiré]'],
    ['écrire à mailto:x@y', 'écrire à [lien retiré]'],
    ['tel:+33600000000', '[lien retiré]'],
    ['sms:+33600000000?body=10500', '[lien retiré]'],
    ['contact vol@evil.example', 'contact [lien retiré]'],
    ['aller sur www.evil.example/10500', 'aller sur www[.]evil[.]example/10500'],
    ['banque-sécurisée.fr', 'banque-sécurisée[.]fr'],
    ['10.0.0.1/10500', '10[.]0[.]0[.]1/10500'],
  ])('désamorce « %s »', (text, expected) => {
    expect(defangLinks(text)).toBe(expected);
  });

  it('laisse les montants, dates et phrases intacts', () => {
    const text = '- Patrimoine : 10 500 € (+1,5 %) au 26/11/2026. Mois 2026-11 : 0 €. Version 1.5.';
    expect(defangLinks(text)).toBe(text);
  });
});
