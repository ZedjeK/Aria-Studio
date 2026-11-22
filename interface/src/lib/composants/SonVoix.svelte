<!--
  Le son d'une voix de la bibliothèque, lu par Rust et joué par une adresse blob:, libérée quand il change ou disparaît.
-->
<script lang="ts">
  import { sonVoix } from '../api';

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
    void sonVoix(demande)
      .then((octets) => {
        if (annulee) return;
        creee = URL.createObjectURL(new Blob([octets]));
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
  <audio src={source} controls></audio>
{:else if erreur}
  <span class="discret">Son illisible.</span>
{/if}
