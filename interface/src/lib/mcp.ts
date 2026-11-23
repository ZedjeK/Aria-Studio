// Réglages → Claude (MCP) : le bloc « mcpServers » à coller dans ~/.claude.json ou le .mcp.json d'un projet.

import type { DeclarationMcp } from './api';

/** Le serveur d'Aria sous son nom, indenté comme la v2 le montrait. */
export function blocMcp(d: DeclarationMcp): string {
  return JSON.stringify({ [d.nom]: d.serveur }, null, 2);
}
