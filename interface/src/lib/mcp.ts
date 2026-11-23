// Réglages → Claude (MCP) : le bloc « mcpServers » à coller dans ~/.claude.json ou le .mcp.json d'un projet.

import type { DeclarationMcp } from './api';

/** Le serveur d'Aria sous son nom, indenté comme la v2 le montrait. */
export function blocMcp(d: DeclarationMcp): string {
  return JSON.stringify({ [d.nom]: d.serveur }, null, 2);
}

/** Ce que Claude peut faire par le serveur MCP d'Aria : les outils qu'il expose (aria_coeur::mcp). */
export const OUTILS_MCP = ['etat', 'ecrire_prompt', 'generer_image', 'suivre_rendu', 'fiche_image'] as const;
