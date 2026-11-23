import { describe, expect, it } from 'vitest';
import { blocMcp, OUTILS_MCP } from './mcp';

describe('MCP', () => {
  it('rend le serveur sous son nom', () => {
    const bloc = blocMcp({
      nom: 'aria-studio',
      exe: 'C:/Aria/Aria-Studio.exe',
      commande: 'claude mcp add …',
      serveur: { type: 'stdio', command: 'C:/Aria/Aria-Studio.exe', args: ['--mcp'] },
    });
    expect(JSON.parse(bloc)).toEqual({
      'aria-studio': { type: 'stdio', command: 'C:/Aria/Aria-Studio.exe', args: ['--mcp'] },
    });
    expect(bloc).toContain('\n  "aria-studio"');
  });

  it('liste les cinq outils que Claude peut appeler, sans suppression ni réglage', () => {
    expect([...OUTILS_MCP]).toEqual(['etat', 'ecrire_prompt', 'generer_image', 'suivre_rendu', 'fiche_image']);
  });
});
