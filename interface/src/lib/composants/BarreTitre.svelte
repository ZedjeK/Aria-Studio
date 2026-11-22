<!--
  La barre de titre de la fenêtre, sans cadre de Windows : une bande qu'on attrape pour déplacer la fenêtre (double
  clic : agrandir), et les boutons réduire, agrandir, fermer, seuls en haut à droite. Fermer suit l'appli : une appli
  en arrière-plan se range dans son icône.
-->
<script lang="ts">
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import type { Snippet } from 'svelte';

  interface Props {
    children?: Snippet;
  }

  let { children }: Props = $props();
  const fenetre = getCurrentWindow();
</script>

<div class="barre" data-tauri-drag-region>
  <div class="contenu" data-tauri-drag-region>{@render children?.()}</div>
  <div class="boutons">
    <button type="button" title="Réduire" aria-label="Réduire" onclick={() => void fenetre.minimize()}>
      <svg viewBox="0 0 10 10" aria-hidden="true"><path d="M1 5h8" /></svg>
    </button>
    <button type="button" title="Agrandir" aria-label="Agrandir" onclick={() => void fenetre.toggleMaximize()}>
      <svg viewBox="0 0 10 10" aria-hidden="true"><rect x="1.5" y="1.5" width="7" height="7" /></svg>
    </button>
    <button type="button" class="fermer" title="Fermer" aria-label="Fermer" onclick={() => void fenetre.close()}>
      <svg viewBox="0 0 10 10" aria-hidden="true"><path d="M1.5 1.5l7 7M8.5 1.5l-7 7" /></svg>
    </button>
  </div>
</div>

<style>
  .barre {
    display: flex;
    align-items: center;
    justify-content: space-between;
    height: 44px;
    padding-left: 28px;
    user-select: none;
  }

  .contenu {
    display: flex;
    flex: 1;
    align-items: center;
    height: 100%;
  }

  .boutons {
    display: flex;
    align-self: flex-start;
  }

  button {
    display: grid;
    place-items: center;
    width: 46px;
    height: 34px;
    border: 0;
    background: none;
    color: var(--texte-2);
    cursor: pointer;
  }

  button:hover,
  button:focus-visible {
    background: color-mix(in srgb, var(--cyan) 10%, transparent);
    color: var(--texte);
    outline: none;
  }

  button.fermer:hover,
  button.fermer:focus-visible {
    background: var(--rouge);
    color: #fff;
  }

  svg {
    width: 10px;
    height: 10px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.2;
  }
</style>
