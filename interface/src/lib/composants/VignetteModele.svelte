<!--
  La vignette d'un modèle de Civitai, lue par Rust (la page ne charge rien d'ailleurs) et montrée par une adresse
  blob:, libérée quand elle change ou disparaît. Floue tant que l'image adulte n'est pas demandée.
-->
<script lang="ts">
  import { vignetteModele } from '../api';

  interface Props {
    url: string;
    floue: boolean;
  }

  let { url, floue }: Props = $props();

  let source = $state<string | null>(null);

  $effect(() => {
    const demandee = url;
    let annulee = false;
    let creee: string | null = null;
    void vignetteModele(demandee)
      .then((octets) => {
        if (annulee) return;
        creee = URL.createObjectURL(new Blob([octets]));
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

<div class="vignette" class:floue>
  {#if source}
    <img src={source} alt="" />
  {/if}
</div>

<style>
  .vignette {
    flex: none;
    width: 96px;
    height: 96px;
    overflow: hidden;
    background: var(--fond-2, rgb(255 255 255 / 4%));
  }

  img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .floue img {
    filter: blur(14px);
  }
</style>
