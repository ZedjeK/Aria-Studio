<!--
  La vignette d'un modèle de Civitai, lue par Rust (la page ne charge rien d'ailleurs) et montrée par une adresse
  blob:, libérée quand elle change ou disparaît. Floue tant que l'image adulte n'est pas demandée. Sans aperçu, le cadre
  reste là, vide : une ligne de résultat garde toujours sa vignette.
-->
<script lang="ts">
  import { vignetteModele } from '../api';

  interface Props {
    url: string | null;
    floue: boolean;
  }

  let { url, floue }: Props = $props();

  let source = $state<string | null>(null);

  $effect(() => {
    const demandee = url;
    if (demandee === null) {
      source = null;
      return;
    }
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
    width: 64px;
    height: 64px;
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
