<!-- Jauge d'une mesure : vide et « à tester » tant que la mesure n'est pas faite, jamais un chiffre supposé. -->
<script lang="ts">
  import { segmentsRemplis, texteJauge } from '../jauge';

  interface Props {
    etiquette: string;
    fraction: number | null;
    mesure?: string | undefined;
    segments?: number;
  }

  let { etiquette, fraction, mesure, segments = 24 }: Props = $props();

  const remplis = $derived(segmentsRemplis(fraction, segments));
  const texte = $derived(texteJauge(fraction, mesure));
  const indices = $derived(Array.from({ length: segments }, (_, i) => i));
</script>

<div
  class="jauge"
  role="meter"
  aria-label={etiquette}
  aria-valuemin={0}
  aria-valuemax={100}
  aria-valuenow={fraction === null ? 0 : Math.round(fraction * 100)}
  aria-valuetext={texte}
>
  <span class="etiquette">{etiquette}</span>
  <span class="segments">
    {#each indices as i (i)}
      <i class:plein={i < remplis}></i>
    {/each}
  </span>
  <span class="texte" class:vide={fraction === null}>{texte}</span>
</div>

<style>
  .jauge {
    display: grid;
    grid-template-columns: 90px 1fr auto;
    gap: 14px;
    align-items: center;
  }

  .etiquette {
    font-size: 12px;
    font-weight: 700;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--texte-2);
  }

  .segments {
    display: flex;
    gap: 3px;
  }

  .segments i {
    flex: 1;
    height: 12px;
    background: var(--ligne);
  }

  .segments i.plein {
    background: var(--cyan);
  }

  .texte {
    min-width: 140px;
    font: 400 12px var(--police-code);
    color: var(--texte);
  }

  .texte.vide {
    color: var(--texte-2);
  }
</style>
