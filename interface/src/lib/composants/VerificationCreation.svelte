<!--
  Vérifier l'image (nouveauté de la v3) : le LLM qui voit compare une création (une image, ou la première image d'un
  clip) à l'intention lue sur sa fiche. Il rend un verdict, les écarts et un prompt corrigé, qu'on peut retoucher puis
  relancer en un clic (la même demande, avec ce prompt). Sans LLM qui voit, le bouton reste grisé et dit quoi
  installer.
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import {
    etatVision,
    messageErreur,
    relancerAvecPrompt,
    verifierImages,
    type Creation,
    type EtatVision,
    type Verification,
  } from '../api';
  import { pasRelancable } from '../vision';
  import AnnulerEcriture from './AnnulerEcriture.svelte';
  import Bouton from './Bouton.svelte';

  interface Props {
    creation: Creation;
    signaler: (texte: string, erreur?: boolean) => void;
    /** La fiche a changé (vérification écrite) : la liste se relit. */
    apres?: (() => void) | undefined;
    /** Une commande vient d'entrer dans la file. */
    enFile?: (() => void) | undefined;
  }

  let { creation, signaler, apres, enFile }: Props = $props();

  let vision = $state<EtatVision | null>(null);
  let occupe = $state(false);
  let relance = $state(false);
  let faite = $state<Verification | null>(null);
  let promptCorrige = $state('');

  const verification = $derived(faite ?? creation.verification ?? null);
  const raisonPasRelancable = $derived(pasRelancable(creation));

  // Une autre création choisie : la vérification affichée est la sienne.
  $effect(() => {
    const v = creation.verification ?? null;
    faite = null;
    promptCorrige = v?.prompt ?? '';
  });

  async function verifier() {
    occupe = true;
    try {
      const bilan = await verifierImages([creation.id]);
      const r = bilan.resultats[0];
      if (r?.verification) {
        faite = r.verification;
        promptCorrige = r.verification.prompt ?? '';
        signaler(r.verification.fidele ? 'Image fidèle à l’intention.' : 'Écarts trouvés : prompt corrigé proposé.');
        apres?.();
      } else {
        signaler(r?.erreur ?? 'Image non vérifiée', true);
      }
    } catch (e) {
      signaler(messageErreur(e), true);
    } finally {
      occupe = false;
    }
  }

  async function relancer() {
    relance = true;
    try {
      await relancerAvecPrompt(creation.id, promptCorrige);
      signaler('Relancée avec le prompt corrigé : en file.');
      enFile?.();
    } catch (e) {
      signaler(messageErreur(e), true);
    } finally {
      relance = false;
    }
  }

  onMount(() => {
    void (async () => {
      try {
        vision = await etatVision(creation.workflow ?? null);
      } catch {
        vision = { llm: null, explication: 'État de la vision illisible' };
      }
    })();
  });
</script>

<div class="verification">
  <div class="ligne">
    <Bouton petit desactive={occupe || !vision?.llm} onclick={() => void verifier()}>
      {verification ? 'Vérifier à nouveau' : 'Vérifier l’image'}
    </Bouton>
    {#if occupe}
      <span class="discret">Le LLM regarde l’image…</span>
      <AnnulerEcriture />
    {:else if vision && !vision.llm}
      <span class="discret">{vision.explication}</span>
    {:else if vision?.llm}
      <span class="discret">par {vision.llm}</span>
    {/if}
  </div>

  {#if verification}
    <p class="verdict" class:fidele={verification.fidele} class:ecarts={!verification.fidele}>
      <strong>{verification.fidele ? '✓ Fidèle' : '⚠ Écarts'}</strong> · {verification.verdict}
    </p>
    {#if verification.ecarts.length > 0}
      <ul>
        {#each verification.ecarts as ecart, i (i)}
          <li>{ecart}</li>
        {/each}
      </ul>
    {/if}
    {#each verification.alertes as alerte, i (i)}
      <p class="discret">{alerte}</p>
    {/each}
    {#if verification.prompt}
      <label class="corrige">
        <span class="etiquette">Prompt corrigé (tu peux le retoucher)</span>
        <textarea rows="4" bind:value={promptCorrige}></textarea>
      </label>
      {#if raisonPasRelancable}
        <p class="discret">{raisonPasRelancable}</p>
      {:else}
        <div class="ligne">
          <Bouton genre="principal" petit desactive={relance || promptCorrige.trim() === ''} onclick={() => void relancer()}>
            Relancer avec ce prompt
          </Bouton>
        </div>
      {/if}
    {/if}
    <p class="discret">Vérifiée par {verification.llm} le {verification.date.replace('T', ' à ')}</p>
  {/if}
</div>

<style>
  .verification {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin: 8px 0;
  }

  .ligne {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px;
  }

  .verdict {
    margin: 4px 0 0;
  }

  .verdict.fidele strong {
    color: var(--cyan);
  }

  .verdict.ecarts strong {
    color: var(--orange);
  }

  ul {
    margin: 0;
    padding-left: 20px;
  }

  .corrige {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .etiquette {
    font-size: 12px;
    color: var(--texte-2);
  }

  textarea {
    width: 100%;
  }
</style>
