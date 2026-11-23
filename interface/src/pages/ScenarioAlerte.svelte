<!--
  Bande d'alerte du Scénario, comme la maquette : un libellé en capitales (« Contrôle ») et le message. Ambre pour un
  avertissement, rouge pour une erreur. Les pages du Scénario la posent sous le prompt écrit par le LLM.
-->
<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    titre?: string;
    erreur?: boolean;
    children: Snippet;
  }

  let { titre = 'Contrôle', erreur = false, children }: Props = $props();
</script>

<div class="alerte" class:erreur role={erreur ? 'alert' : 'status'}>
  <b>{titre}</b>
  <span>{@render children()}</span>
</div>

<style>
  .alerte {
    display: flex;
    gap: 12px;
    align-items: baseline;
    padding: 8px 14px;
    border: 1px solid var(--ambre);
    background: color-mix(in srgb, var(--ambre) 9%, var(--panneau));
    font-size: 13px;
  }

  b {
    flex: none;
    color: var(--ambre);
    font-size: 11px;
    letter-spacing: 0.12em;
    text-transform: uppercase;
  }

  .erreur {
    border-color: var(--rouge);
    background: color-mix(in srgb, var(--rouge) 9%, var(--panneau));
  }

  .erreur b {
    color: var(--rouge);
  }
</style>
