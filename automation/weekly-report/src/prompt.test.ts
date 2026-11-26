import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { SKILL_URL, systemPromptFromSkill } from './prompt.ts';
import { SECTIONS } from './report.ts';

const skill = readFileSync(SKILL_URL, 'utf8');

describe('prompt système', () => {
  it('reprend le skill sans son en-tête, avec la période', () => {
    const prompt = systemPromptFromSkill(skill);
    expect(prompt.startsWith('# Rapport de patrimoine FineMe')).toBe(true);
    expect(prompt).not.toContain('allowed-tools');
    expect(prompt).not.toContain('$ARGUMENTS');
    expect(prompt).toContain('Période demandée : « 1m »');
  });

  it('garde le même format que la validation du rapport', () => {
    const format = /```\n(# Rapport de patrimoine[\s\S]*?)```/.exec(skill)![1];
    const headings = format.split('\n').filter((line) => line.startsWith('## '));
    expect(headings).toEqual(SECTIONS.map((section) => `## ${section}`));
  });

  it('refuse un fichier sans en-tête', () => {
    expect(() => systemPromptFromSkill('# sans en-tête')).toThrow('en-tête');
  });
});
