// Le format du rapport a une seule source : le skill /rapport-patrimoine, repris ici comme prompt système.

export const SKILL_URL = new URL('../../../.claude/skills/rapport-patrimoine/SKILL.md', import.meta.url);

export const USER_PROMPT = 'Rédige le rapport de patrimoine hebdomadaire. Réponds uniquement par le rapport.';

const FRONTMATTER = /^---\r?\n[\s\S]*?\r?\n---\r?\n/;

/** Corps du skill (sans son en-tête), avec la période à la place de l'argument de la commande. */
export function systemPromptFromSkill(skill: string, period = '1m'): string {
  if (!FRONTMATTER.test(skill)) {
    throw new Error('SKILL.md sans en-tête : format inattendu.');
  }
  return skill.replace(FRONTMATTER, '').replaceAll('$ARGUMENTS', period).trim();
}
