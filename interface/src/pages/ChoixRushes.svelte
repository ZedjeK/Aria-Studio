<!--
  Choisir des rushes de départ (S'inspirer, Redessiner) : les plus récents, cliqués dans l'ordre (l'ordre fait leur
  numéro : image 1, image 2…). Au-delà de `max`, le premier choisi cède sa place ; avec `max` à 1, un clic remplace.
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import { imageCreation, lireRushes, messageErreur, type Creation } from '../lib/api';

  interface Props {
    max: number;
    selection: string[];
    signaler: (texte: string, erreur?: boolean) => void;
  }

  let { max, selection = $bindable(), signaler }: Props = $props();

  let rushes = $state<Creation[]>([]);
  let vignettes = $state<Record<string, string>>({});

  function basculer(id: string) {
    if (selection.includes(id)) {
      selection = selection.filter((s) => s !== id);
    } else {
      selection = [...selection, id].slice(-Math.max(1, max));
    }
  }

  // Un workflow qui en reçoit moins : la sélection est raccourcie.
  $effect(() => {
    if (selection.length > max) selection = selection.slice(0, Math.max(0, max));
  });

  onMount(() => {
    void (async () => {
      try {
        rushes = (await lireRushes('tous', null)).slice(0, 48);
        for (const r of rushes) {
          try {
            const octets = await imageCreation(r.id);
            vignettes[r.id] = URL.createObjectURL(new Blob([octets], { type: 'image/png' }));
          } catch {
            // Une image effacée entre-temps : la vignette reste vide.
          }
        }
      } catch (e) {
        signaler(messageErreur(e), true);
      }
    })();
    return () => {
      for (const adresse of Object.values(vignettes)) URL.revokeObjectURL(adresse);
    };
  });
</script>

{#if rushes.length === 0}
  <p class="discret">Aucun rush pour l'instant : crée d'abord une image, ou importe-en une depuis la Retouche.</p>
{:else}
  <div class="choix">
    {#each rushes as rush (rush.id)}
      {@const rang = selection.indexOf(rush.id)}
      <button type="button" class="vignette" class:choisi={rang >= 0} aria-pressed={rang >= 0} onclick={() => basculer(rush.id)}>
        {#if vignettes[rush.id]}
          <img src={vignettes[rush.id]} alt={rush.prompt ?? ''} />
        {:else}
          <span class="vide" aria-hidden="true"></span>
        {/if}
        {#if rang >= 0 && max > 1}
          <span class="rang">{rang + 1}</span>
        {/if}
      </button>
    {/each}
  </div>
{/if}

<style>
  .choix {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(76px, 1fr));
    gap: 8px;
    max-height: 200px;
    overflow-y: auto;
  }

  .vignette {
    position: relative;
    padding: 2px;
    background: transparent;
    border: 1px solid transparent;
    cursor: pointer;
  }

  .vignette.choisi {
    border-color: var(--orange);
  }

  img,
  .vide {
    display: block;
    width: 100%;
    aspect-ratio: 1;
    object-fit: cover;
    background: var(--fond);
  }

  .rang {
    position: absolute;
    top: 4px;
    left: 4px;
    padding: 0 6px;
    font-size: 12px;
    font-weight: 600;
    color: var(--fond);
    background: var(--orange);
  }
</style>
