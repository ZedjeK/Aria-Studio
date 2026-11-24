<!--
  Enrobage de l'état en direct (composant EtatDirect du modèle, laissé tel quel) : la VRAM de la carte en fond des
  pastilles. Chaque pastille est le bloc entier de la VRAM totale, un léger fond orangé en remplit la part utilisée,
  et une info-bulle dit « 9,8 Go sur 16 Go ». Sans lecture (pas de carte NVIDIA), rien : pas de fond, pas d'info-bulle.
-->
<script lang="ts">
  import type { Vram } from './api';
  import { fractionVram, texteVram, type Pastille } from './menu';
  import EtatDirect from './composants/EtatDirect.svelte';

  interface Props {
    pastilles: Pastille[];
    vram: Vram | null;
  }

  let { pastilles, vram }: Props = $props();

  const fraction = $derived(fractionVram(vram));
  const texte = $derived(texteVram(vram));
</script>

<div class="vram" class:avec-fond={fraction !== null} style:--vram={fraction === null ? null : `${(fraction * 100).toFixed(1)}%`} title={texte ?? undefined}>
  <EtatDirect {pastilles} />
</div>

<style>
  /* Le fond se pose sur les pastilles du modèle sans toucher à son composant : un dégradé orangé, de la gauche vers la part utilisée. */
  .avec-fond :global(.etat-direct li) {
    background-image: linear-gradient(
      90deg,
      color-mix(in srgb, var(--orange) 24%, transparent) var(--vram),
      transparent var(--vram)
    );
  }
</style>
