<!--
  Une fenêtre de dialogue posée sur la page : voile sombre, cadre à coins coupés au liseré fin, titre en cyan, puis le
  contenu (À savoir, panneaux) et la rangée du bas (ligne d'état, boutons à droite, l'orange en dernier). La fenêtre
  n'est jamais accentuée : le liseré accentué va au panneau sur lequel agit le bouton orange. Échap la ferme.
-->
<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    titre: string;
    fermer: () => void;
    children: Snippet;
    pied: Snippet;
  }

  let { titre, fermer, children, pied }: Props = $props();
  let cadre = $state<HTMLElement | null>(null);

  $effect(() => {
    cadre?.focus();
    const touche = (e: KeyboardEvent) => {
      if (e.key === 'Escape') fermer();
    };
    window.addEventListener('keydown', touche);
    return () => window.removeEventListener('keydown', touche);
  });
</script>

<div class="voile">
  <div class="cadre" role="dialog" aria-modal="true" aria-labelledby="titre-dialogue" tabindex="-1" bind:this={cadre}>
    <div class="dedans">
      <h2 id="titre-dialogue">{titre}</h2>
      {@render children()}
      <div class="pied">{@render pied()}</div>
    </div>
  </div>
</div>

<style>
  .voile {
    position: fixed;
    inset: 0;
    z-index: 20;
    display: grid;
    place-items: center;
    padding: 24px;
    background: color-mix(in srgb, var(--fond) 72%, transparent);
  }

  .cadre {
    --coins: polygon(
      var(--coupe) 0,
      100% 0,
      100% calc(100% - var(--coupe)),
      calc(100% - var(--coupe)) 100%,
      0 100%,
      0 var(--coupe)
    );
    width: min(620px, 100%);
    max-height: 100%;
    padding: 1px;
    clip-path: var(--coins);
    background: color-mix(in srgb, var(--cyan) 45%, transparent);
    outline: none;
  }

  .dedans {
    display: flex;
    flex-direction: column;
    gap: 16px;
    max-height: calc(100vh - 50px);
    overflow-y: auto;
    padding: 24px;
    clip-path: var(--coins);
    background: var(--fond);
  }

  h2 {
    margin: 0;
    color: var(--cyan);
    font-size: 20px;
    font-weight: 600;
  }

  .pied {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: flex-end;
    gap: 12px;
  }
</style>
