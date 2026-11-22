<!--
  Les LoRA de la famille du workflow choisi, comme en v2 : une case, un curseur de force et des mots déclencheurs
  (facultatifs, mis en tête du prompt) par LoRA. Plusieurs cochées : un conseil de baisser leur force.
-->
<script lang="ts">
  import type { LoraProposee } from '../api';
  import { actives, forceLisible, type LigneLora } from '../loras';

  interface Props {
    famille: string;
    proposees: LoraProposee[];
    forceMax: number;
    lignes: Record<string, LigneLora>;
  }

  let { famille, proposees, forceMax, lignes = $bindable() }: Props = $props();

  const compte = $derived(actives(lignes));
</script>

<details class="loras" open={compte > 0}>
  <summary>
    LoRA · {famille}
    <span class="discret">· {compte === 0 ? 'aucune active' : `${compte} active${compte > 1 ? 's' : ''}`}</span>
  </summary>
  {#if proposees.length === 0}
    <p class="discret">Aucune LoRA : cherche-en dans Modèles → Rechercher, ou dépose des .safetensors dans son dossier.</p>
  {:else}
    {#each proposees as l (l.fichier)}
      {@const ligne = lignes[l.fichier]}
      {#if ligne}
        <div class="lora" class:active={ligne.actif}>
          <label title={l.fichier}>
            <input type="checkbox" bind:checked={ligne.actif} />
            {l.nom}
          </label>
          <input
            type="range"
            min="0"
            max={forceMax}
            step="0.05"
            bind:value={ligne.force}
            aria-label={`Force de ${l.nom}`}
          />
          <span class="valeur">{forceLisible(ligne.force)}</span>
          <input
            type="text"
            placeholder="mots déclencheurs (facultatif)"
            aria-label={`Mots déclencheurs de ${l.nom}`}
            bind:value={ligne.declencheurs}
          />
        </div>
      {/if}
    {/each}
    {#if compte >= 2}
      <p class="discret">
        💡 Plusieurs LoRA cochées : baisse un peu la force de chacune (vers 0,6 à 0,8), sinon elles se marchent dessus
        et « cuisent » l'image.
      </p>
    {/if}
  {/if}
</details>

<style>
  .loras {
    margin: 10px 0;
  }

  summary {
    cursor: pointer;
  }

  .lora {
    display: grid;
    grid-template-columns: minmax(120px, 1fr) minmax(100px, 160px) 3em minmax(140px, 2fr);
    align-items: center;
    gap: 10px;
    margin: 6px 0;
    color: var(--texte-2);
  }

  .lora.active {
    color: var(--texte);
  }

  label {
    display: flex;
    align-items: center;
    gap: 6px;
    overflow-wrap: anywhere;
  }

  .valeur {
    font-variant-numeric: tabular-nums;
  }
</style>
