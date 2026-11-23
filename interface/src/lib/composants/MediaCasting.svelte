<!--
  Une image ou un clip d'un casting (ou une image de départ), lu par Rust et montré par une adresse blob:, libérée quand
  il change ou disparaît. Un clip se lit en boucle, muet au départ.
-->
<script lang="ts">
  import { untrack } from 'svelte';
  import { estVideo } from '../casting';

  interface Props {
    /** Ce qui lit les octets. */
    lire: () => Promise<ArrayBuffer>;
    /** Sert à reconnaître un clip (.mp4) et à relire quand il change. */
    fichier: string;
    titre?: string;
  }

  let { lire, fichier, titre = '' }: Props = $props();

  let source = $state<string | null>(null);
  let erreur = $state(false);
  const video = $derived(estVideo(fichier));

  $effect(() => {
    const type = estVideo(fichier) ? 'video/mp4' : 'image/png';
    let annulee = false;
    let creee: string | null = null;
    source = null;
    erreur = false;
    // Relu quand le fichier change, pas à chaque rendu du parent (qui recrée `lire`).
    void untrack(lire)
      .then((octets) => {
        if (annulee) return;
        creee = URL.createObjectURL(new Blob([octets], { type }));
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

{#if source && video}
  <video src={source} title={titre} loop muted autoplay playsinline controls preload="metadata"></video>
{:else if source}
  <img src={source} alt={titre} title={titre} loading="lazy" />
{:else if erreur}
  <span class="discret">Illisible.</span>
{:else}
  <span class="vide" aria-hidden="true"></span>
{/if}

<style>
  img,
  video,
  .vide {
    display: block;
    width: 100%;
    aspect-ratio: 1;
    border: 1px solid var(--ligne);
    object-fit: contain;
    background: var(--fond-2, transparent);
  }
</style>
