<!--
  La vignette d'une création (image, ou affiche d'un clip), lue par Rust et montrée par une adresse blob:, libérée quand
  elle change ou disparaît.
-->
<script lang="ts">
  import { imageCreation } from '../api';

  interface Props {
    id: string;
    titre?: string;
  }

  let { id, titre = '' }: Props = $props();

  let source = $state<string | null>(null);

  $effect(() => {
    const demande = id;
    let annulee = false;
    let creee: string | null = null;
    source = null;
    void imageCreation(demande)
      .then((octets) => {
        if (annulee) return;
        creee = URL.createObjectURL(new Blob([octets], { type: 'image/png' }));
        source = creee;
      })
      .catch(() => {
        source = null;
      });
    return () => {
      annulee = true;
      if (creee) URL.revokeObjectURL(creee);
    };
  });
</script>

{#if source}
  <img src={source} alt={titre} title={titre} draggable="false" />
{:else}
  <span class="vide" title={titre}></span>
{/if}

<style>
  img,
  .vide {
    display: block;
    width: 96px;
    height: 96px;
    border: 1px solid var(--ligne);
    object-fit: cover;
  }
</style>
