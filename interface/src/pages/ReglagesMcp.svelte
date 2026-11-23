<!--
  Réglages → Claude (MCP), comme la carte de la v2 : de quoi déclarer le serveur MCP d'Aria dans Claude Code, en
  commande ou en bloc JSON (« mcpServers »). L'exe est celui de cette Aria ; Claude le lance avec --mcp.
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import { commandeMcp, messageErreur, type DeclarationMcp } from '../lib/api';
  import { blocMcp, OUTILS_MCP } from '../lib/mcp';
  import ASavoir from '../lib/composants/ASavoir.svelte';
  import Bouton from '../lib/composants/Bouton.svelte';
  import Panneau from '../lib/composants/Panneau.svelte';

  interface Props {
    signaler: (texte: string, erreur?: boolean) => void;
  }

  let { signaler }: Props = $props();

  let declaration = $state<DeclarationMcp | null>(null);
  const bloc = $derived(declaration ? blocMcp(declaration) : '');

  async function copier(texte: string) {
    try {
      await navigator.clipboard.writeText(texte);
      signaler('Copié.');
    } catch {
      signaler('Copie impossible ici : sélectionne le texte et copie-le à la main.', true);
    }
  }

  onMount(() => {
    void commandeMcp()
      .then((d) => {
        declaration = d;
      })
      .catch((e: unknown) => signaler(messageErreur(e), true));
  });
</script>

<ASavoir>
  Claude Code peut piloter Aria : voir son état, écrire un prompt avec ton LLM local, lancer un rendu et le suivre,
  lire la fiche d'une image. Rien ne supprime, ne déplace ni ne change un réglage, et le filtre de protection des
  mineurs s'applique comme ici. Aria doit être lancée.
</ASavoir>

<Panneau titre="Brancher Claude Code">
  {#if declaration}
    <p class="discret">Colle cette commande dans un terminal, une seule fois :</p>
    <pre class="bloc">{declaration.commande}</pre>
    <div class="actions"><Bouton petit onclick={() => void copier(declaration?.commande ?? '')}>Copier</Bouton></div>
    <p class="discret autre">
      Ou, pour un autre client, ce bloc JSON, à ajouter sous « mcpServers » (dans <code>~/.claude.json</code> pour tous tes
      projets, ou dans le <code>.mcp.json</code> d'un projet), puis relance Claude :
    </p>
    <pre class="bloc">{bloc}</pre>
    <div class="actions"><Bouton petit onclick={() => void copier(bloc)}>Copier</Bouton></div>
  {:else}
    <p class="discret">Lecture…</p>
  {/if}
</Panneau>

<Panneau titre="Ce que Claude peut faire">
  <div class="puces">
    {#each OUTILS_MCP as outil (outil)}<span class="puce">{outil}</span>{/each}
  </div>
  <p class="discret">
    Ce que ces outils rendent (prompts, fiches des images) part vers ton compte Claude. Les images restent sur ton PC :
    Claude ne reçoit que leur chemin.
  </p>
</Panneau>

<style>
  .bloc {
    margin: 8px 0;
    padding: 8px 12px;
    border: 1px solid var(--ligne);
    background: var(--fond);
    font-family: var(--police-code);
    font-size: 12px;
    overflow-wrap: anywhere;
    white-space: pre-wrap;
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    margin-top: 8px;
  }

  .autre {
    margin-top: 14px;
  }

  .puces {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-bottom: 8px;
  }

  .puce {
    padding: 2px 10px;
    border: 1px solid var(--cyan);
    color: var(--cyan);
    font-family: var(--police-code);
    font-size: 11.5px;
    letter-spacing: 0.04em;
  }
</style>
