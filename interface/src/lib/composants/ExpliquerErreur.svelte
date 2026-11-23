<!--
  « Expliquer » une erreur : la cause connue d'Aria aussitôt, sinon l'analyse du LLM, avec les remèdes concrets dans
  l'appli. Une cause connue mais large (un moteur qui ne répond pas) peut s'approfondir avec le LLM, qui lit alors la
  fin du journal du moteur. ✕ Annuler arrête le LLM. Rien de secret ne part : le cœur masque clés et chemins.
-->
<script lang="ts">
  import { expliquerErreur, messageErreur, type Explication, type OrigineErreur } from '../api';
  import { libelleCertitude, libelleSource } from '../expliquer';
  import AnnulerEcriture from './AnnulerEcriture.svelte';
  import Bouton from './Bouton.svelte';

  interface Props {
    erreur: string;
    origine: OrigineErreur;
    contexte?: string;
    commande?: string | null;
    moteur?: string | null;
  }

  let { erreur, origine, contexte = '', commande = null, moteur = null }: Props = $props();

  let explication = $state<Explication | null>(null);
  let enCours = $state(false);
  let ouvert = $state(false);
  let probleme = $state('');

  async function expliquer(approfondir = false) {
    ouvert = true;
    enCours = true;
    probleme = '';
    try {
      explication = await expliquerErreur({ erreur, origine, contexte, commande, moteur, approfondir });
    } catch (e) {
      probleme = messageErreur(e);
    } finally {
      enCours = false;
    }
  }
</script>

{#if !ouvert}
  <span class="ouvrir">
    <Bouton petit onclick={() => void expliquer()}>🩺 Expliquer</Bouton>
  </span>
{:else}
  <div class="explication" role="status">
    {#if enCours}
      <p class="ligne">
        <span class="discret">Aria cherche la cause…</span>
        <AnnulerEcriture />
      </p>
    {:else if probleme}
      <p class="ko">{probleme}</p>
    {:else if explication}
      <p class="tete">
        <strong>{libelleCertitude(explication.certitude)}</strong>
        <span class="discret">· {libelleSource(explication)}</span>
      </p>
      <p>{explication.cause}</p>
      {#if explication.remedes.length > 0}
        <ol>
          {#each explication.remedes as remede, i (i)}
            <li>{remede}</li>
          {/each}
        </ol>
      {/if}
      {#each explication.alertes as alerte, i (i)}
        <p class="discret">⚠ {alerte}</p>
      {/each}
    {/if}
    <div class="ligne">
      {#if explication?.approfondir && !enCours}
        <Bouton petit onclick={() => void expliquer(true)}>Approfondir avec le LLM</Bouton>
      {/if}
      {#if !enCours}
        <Bouton genre="discret" petit onclick={() => (ouvert = false)}>Masquer</Bouton>
      {/if}
    </div>
  </div>
{/if}

<style>
  .ouvrir {
    display: inline-flex;
    margin-top: 4px;
  }

  .explication {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-top: 6px;
    padding: 10px 12px;
    border: 1px solid var(--ligne);
    border-left: 3px solid var(--cyan);
    font-size: 13px;
    white-space: normal;
  }

  .explication p,
  .explication ol {
    margin: 0;
  }

  .explication ol {
    padding-left: 20px;
  }

  .tete {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .ligne {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }

  .ko {
    color: var(--orange);
  }
</style>
