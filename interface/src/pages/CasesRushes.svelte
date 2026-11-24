<!--
  Les rushes de départ d'un atelier : les vignettes choisies (numérotées quand on peut en prendre plusieurs, un ✕ pour
  retirer) et une case ＋ qui ouvre « Quel rush choisir ? » (DialogueRush), avec l'import depuis le disque.
-->
<script lang="ts">
  import type { Creation } from '../lib/api';
  import VignetteCreation from '../lib/composants/VignetteCreation.svelte';
  import DialogueRush from './DialogueRush.svelte';

  interface Props {
    max: number;
    selection: string[];
    signaler: (texte: string, erreur?: boolean) => void;
    apresImport?: ((fiche: Creation) => void) | undefined;
  }

  let { max, selection = $bindable(), signaler, apresImport }: Props = $props();

  let ouvert = $state(false);

  const plusieurs = $derived(max > 1);
  const place = $derived(selection.length < Math.max(1, max));

  // Un workflow qui en reçoit moins : la sélection est raccourcie.
  $effect(() => {
    if (selection.length > max) selection = selection.slice(0, Math.max(0, max));
  });

  function retirer(id: string) {
    selection = selection.filter((s) => s !== id);
  }
</script>

<div class="cases">
  {#each selection as id, rang (id)}
    <div class="case">
      <VignetteCreation {id} titre={`Image ${rang + 1}`} />
      {#if plusieurs}<span class="rang">{rang + 1}</span>{/if}
      <button type="button" class="retire" aria-label={`Retirer l'image ${rang + 1}`} onclick={() => retirer(id)}>✕</button>
    </div>
  {/each}
  {#if place}
    <button type="button" class="case vide-case" aria-label="Choisir un rush" onclick={() => (ouvert = true)}>＋</button>
  {/if}
</div>

{#if ouvert}
  <DialogueRush
    {max}
    bind:selection
    {signaler}
    {apresImport}
    fermer={() => (ouvert = false)}
    apresChoix={() => {
      if (!plusieurs) ouvert = false;
    }}
  />
{/if}

<style>
  .cases {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-end;
    gap: 12px;
  }

  .case {
    position: relative;
    display: block;
    width: 96px;
    height: 96px;
  }

  .vide-case {
    display: grid;
    place-items: center;
    font: inherit;
    font-size: 22px;
    color: var(--texte-2);
    background: var(--fond);
    border: 1px dashed var(--ligne);
    cursor: pointer;
  }

  .vide-case:hover {
    color: var(--cyan);
    border-color: var(--cyan);
  }

  .retire {
    position: absolute;
    top: 2px;
    right: 2px;
    width: 22px;
    height: 22px;
    padding: 0;
    font: inherit;
    font-size: 12px;
    color: var(--texte);
    background: color-mix(in srgb, var(--fond) 86%, transparent);
    border: 1px solid var(--ligne);
    cursor: pointer;
  }

  .rang {
    position: absolute;
    bottom: 2px;
    left: 2px;
    padding: 0 6px;
    font-size: 12px;
    font-weight: 600;
    color: var(--fond);
    background: var(--orange);
  }
</style>
