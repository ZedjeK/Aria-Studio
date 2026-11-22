<!--
  « Ranger dans », comme en v2 : le dossier des Rushes (ou des Bobines) où ranger les prochaines créations. Le choix
  est retenu d'une fois sur l'autre dans ce navigateur (une commodité : sans lui, rien n'est rangé). Un dossier
  supprimé entre-temps n'est plus proposé.
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import { etatRushes, type GenreRushes } from '../api';

  interface Props {
    genre: GenreRushes;
    dossier: string | null;
    /** Où retenir le choix ; par défaut un par genre. La Retouche a le sien, comme en v2. */
    cle?: string;
    /** Change quand des dossiers ont pu être créés ou supprimés. */
    rafraichir?: number;
  }

  let { genre, dossier = $bindable(), cle, rafraichir = 0 }: Props = $props();

  let noms = $state<string[]>([]);
  const memoire = $derived(cle ?? `aria.destination.${genre}`);

  function retenu(): string | null {
    try {
      return localStorage.getItem(memoire);
    } catch {
      return null;
    }
  }

  function retenir(valeur: string | null) {
    try {
      if (valeur) localStorage.setItem(memoire, valeur);
      else localStorage.removeItem(memoire);
    } catch {
      // Stockage indisponible : le choix vaut pour cette fois.
    }
  }

  async function relire() {
    try {
      noms = (await etatRushes(genre)).dossiers.map((d) => d.nom);
    } catch {
      noms = [];
    }
    const voulu = dossier ?? retenu();
    dossier = voulu && noms.includes(voulu) ? voulu : null;
  }

  $effect(() => {
    if (rafraichir > 0) void relire();
  });

  onMount(() => {
    void relire();
  });
</script>

{#if noms.length > 0}
  <label class="ranger">
    <span class="etiquette">Ranger dans</span>
    <select
      value={dossier ?? ''}
      onchange={(e) => {
        dossier = e.currentTarget.value || null;
        retenir(dossier);
      }}
    >
      <option value="">Non rangés</option>
      {#each noms as nom (nom)}
        <option value={nom}>{nom}</option>
      {/each}
    </select>
  </label>
{/if}

<style>
  .ranger {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .etiquette {
    font-size: 12px;
    color: var(--texte-2);
  }
</style>
