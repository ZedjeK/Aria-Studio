<!--
  « Chercher un réglage » et « Modifiés seulement (N) » : une ligne sous l'encadré « À savoir » d'un onglet de réglages.
  Côté interface seulement : elle masque des champs, rien n'est enregistré ni changé.
-->
<script lang="ts">
  interface Props {
    recherche: string;
    modifies: boolean;
    /** Le nombre de réglages modifiés de l'onglet. */
    nombreModifies: number;
    /** Le nombre de réglages montrés, dit quand un filtre est actif. */
    nombreMontres: number;
  }

  let { recherche = $bindable(), modifies = $bindable(), nombreModifies, nombreMontres }: Props = $props();

  const filtre = $derived(recherche.trim() !== '' || modifies);
</script>

<div class="ligne">
  <input type="search" bind:value={recherche} placeholder="🔎 Chercher un réglage" aria-label="Chercher un réglage" />
  <label class="coche">
    <input type="checkbox" bind:checked={modifies} />
    Modifiés seulement ({nombreModifies})
  </label>
  {#if filtre}
    <span class="discret" role="status">{nombreMontres} réglage{nombreMontres > 1 ? 's' : ''} dans cet onglet</span>
  {/if}
</div>

<style>
  .ligne {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 16px;
    margin: 0 0 12px;
  }

  input[type='search'] {
    min-width: 260px;
  }

  .coche {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--texte-2);
    font-size: 13px;
  }

  .discret {
    color: var(--texte-2);
    font-size: 13px;
  }
</style>
