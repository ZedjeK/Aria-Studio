<!--
  Un bandeau en haut de la zone des pages, sous la barre de titre : une étiquette en capitales, une phrase, puis ses
  boutons à droite. Teinté d'orange : il signale quelque chose à regarder (une mise à jour disponible, par exemple).
-->
<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    etiquette: string;
    texte: string;
    children: Snippet;
  }

  let { etiquette, texte, children }: Props = $props();
</script>

<div class="bandeau" role="status">
  <span class="etiquette code">{etiquette}</span>
  <span class="texte">{texte}</span>
  <div class="actions">{@render children()}</div>
</div>

<style>
  .bandeau {
    position: relative;
    z-index: 2;
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 10px 28px;
    border-bottom: 1px solid color-mix(in srgb, var(--orange) 40%, transparent);
    background: color-mix(in srgb, var(--orange) 12%, var(--fond));
  }

  .etiquette {
    color: var(--orange);
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.14em;
    text-transform: uppercase;
  }

  .texte {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .actions {
    display: flex;
    gap: 10px;
  }
</style>
