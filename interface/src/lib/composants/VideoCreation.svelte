<!--
  La vidéo d'un clip, lue par Rust et montrée par une adresse blob:, libérée quand elle change ou disparaît.
-->
<script lang="ts">
  import { videoCreation } from '../api';

  interface Props {
    id: string;
  }

  let { id }: Props = $props();

  let source = $state<string | null>(null);
  let erreur = $state(false);

  $effect(() => {
    const demande = id;
    let annulee = false;
    let creee: string | null = null;
    erreur = false;
    void videoCreation(demande)
      .then((octets) => {
        if (annulee) return;
        creee = URL.createObjectURL(new Blob([octets], { type: 'video/mp4' }));
        source = creee;
      })
      .catch(() => {
        erreur = true;
      });
    return () => {
      annulee = true;
      if (creee) URL.revokeObjectURL(creee);
    };
  });
</script>

{#if source}
  <!-- svelte-ignore a11y_media_has_caption -->
  <video src={source} controls preload="metadata"></video>
{:else if erreur}
  <p class="discret">Vidéo illisible.</p>
{:else}
  <p class="discret">Chargement…</p>
{/if}

<style>
  video {
    width: 100%;
    max-height: 420px;
    background: #000;
  }
</style>
