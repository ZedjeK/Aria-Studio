<!-- Encadré bleu « À savoir » : le commentaire de l'onglet actif, ou une information (une mise à jour, par exemple). -->
<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    titre?: string;
    children: Snippet;
  }

  let { titre = 'À savoir', children }: Props = $props();
</script>

<aside class="a-savoir">
  <strong>{titre}</strong>
  <div class="texte">{@render children()}</div>
</aside>

<style>
  .a-savoir {
    position: relative;
    z-index: 1;
    display: flex;
    gap: 16px;
    align-items: baseline;
    padding: 10px 16px;
    border: 1px solid var(--bleu);
    background: color-mix(in srgb, var(--bleu) 12%, var(--panneau));
  }

  strong {
    flex: none;
    font-size: 12px;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--bleu);
  }

  .texte {
    color: var(--texte);
  }
</style>
