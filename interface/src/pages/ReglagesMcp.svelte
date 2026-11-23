<!--
  Réglages → Claude (MCP), comme la carte de la v2 : de quoi déclarer le serveur MCP d'Aria dans Claude Code, en
  commande ou en bloc JSON (« mcpServers »). L'exe est celui de cette Aria ; Claude le lance avec --mcp.
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import { commandeMcp, messageErreur, type DeclarationMcp } from '../lib/api';
  import { blocMcp } from '../lib/mcp';
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

<Panneau titre="Claude (MCP)">
  <p class="discret">
    Ce que ces outils rendent (prompts, fiches des images) part vers ton compte Claude. Les images restent sur ton PC :
    Claude ne reçoit que leur chemin.
  </p>
  {#if declaration}
    <h3>Avec la commande <code>claude</code></h3>
    <div class="ligne">
      <code class="commande">{declaration.commande}</code>
      <Bouton petit onclick={() => void copier(declaration?.commande ?? '')}>Copier</Bouton>
    </div>
    <details>
      <summary>Sans elle : le serveur à ajouter sous « mcpServers »</summary>
      <p class="discret">
        Dans <code>~/.claude.json</code> pour tous tes projets, ou dans le <code>.mcp.json</code> d'un projet. Puis relance
        Claude.
      </p>
      <pre class="bloc">{bloc}</pre>
      <Bouton petit onclick={() => void copier(bloc)}>Copier</Bouton>
    </details>
  {:else}
    <p class="discret">Lecture…</p>
  {/if}
</Panneau>

<style>
  h3 {
    margin: 12px 0 6px;
    font-size: 14px;
  }

  .ligne {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px;
  }

  .commande,
  .bloc {
    overflow-wrap: anywhere;
    white-space: pre-wrap;
  }

  .commande {
    flex: 1;
    min-width: 0;
  }

  .bloc {
    margin: 8px 0;
    padding: 8px;
    border: 1px solid var(--ligne);
  }
</style>
