import { describe, expect, it } from 'vitest';
import { blocMcp } from './mcp';

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
});
