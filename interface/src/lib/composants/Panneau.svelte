<!--
  Panneau à coins coupés, opaque, au liseré cyan fin. « accentue » : le liseré épais et le titre en cyan marquent le
  panneau sur lequel agit le bouton orange de la page. Un seul par page, aucun sur une page sans bouton orange.
-->
<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    titre: string;
    accentue?: boolean;
    children: Snippet;
  }

  let { titre, accentue = false, children }: Props = $props();
</script>

<section class="panneau" class:accentue aria-label={titre}>
  <div class="dedans">
    <h3>{titre}</h3>
    {@render children()}
  </div>
</section>

<style>
  .panneau {
    --coins: polygon(
      var(--coupe) 0,
      100% 0,
      100% calc(100% - var(--coupe)),
      calc(100% - var(--coupe)) 100%,
      0 100%,
      0 var(--coupe)
    );
    position: relative;
    z-index: 1;
    padding: 1px;
    clip-path: var(--coins);
    background: color-mix(in srgb, var(--cyan) 45%, transparent);
  }

  .dedans {
    padding: 18px 22px;
    clip-path: var(--coins);
    background: var(--panneau);
  }

  h3 {
    margin: 0 0 10px;
    font-size: 12px;
    font-weight: 700;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--texte-2);
  }

  .accentue {
    padding: 2px;
    background: var(--cyan);
  }

  .accentue h3 {
    color: var(--cyan);
  }
</style>
