<!--
  Sous-onglets d'une page, en haut de la zone principale. Forme en encoche (coin haut droit coupé) ; l'onglet actif a
  un contour cyan et une barre épaisse dessous.
-->
<script lang="ts">
  interface Onglet {
    id: string;
    titre: string;
  }

  interface Props {
    onglets: Onglet[];
    actif: string;
  }

  let { onglets, actif = $bindable() }: Props = $props();
</script>

<div class="onglets" role="tablist">
  {#each onglets as onglet (onglet.id)}
    <button
      type="button"
      role="tab"
      class="onglet"
      class:actif={onglet.id === actif}
      aria-selected={onglet.id === actif}
      onclick={() => (actif = onglet.id)}
    >
      <span>{onglet.titre}</span>
      <i class="barre" aria-hidden="true"></i>
    </button>
  {/each}
</div>

<style>
  .onglets {
    display: flex;
    gap: 6px;
    border-bottom: 1px solid var(--ligne);
  }

  .onglet {
    --encoche: polygon(0 0, calc(100% - 10px) 0, 100% 10px, 100% 100%, 0 100%);
    position: relative;
    padding: 1px;
    border: 0;
    cursor: pointer;
    clip-path: var(--encoche);
    background: var(--ligne);
    color: var(--texte-2);
    font: 700 12px var(--police);
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  .onglet span {
    display: block;
    padding: 9px 22px 11px 16px;
    clip-path: var(--encoche);
    background: var(--panneau);
  }

  .barre {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    height: 3px;
  }

  .actif {
    background: var(--cyan);
    color: var(--cyan);
  }

  .actif .barre {
    background: var(--cyan);
  }

  .onglet:hover:not(.actif) {
    color: var(--texte);
  }
</style>
