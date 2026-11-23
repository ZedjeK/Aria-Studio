<!--
  « Proposer des directions » (nouveauté de la v3) : à partir d'une envie vague, le LLM propose trois directions
  distinctes. « Prendre » pose la direction dans l'idée (ou le fil de l'histoire), à retoucher puis à écrire avec ✨ ;
  « En parler » l'emmène dans 💬 M'aider à préciser. Les directions vues restent tant qu'on reste dans l'atelier, et
  « Proposer d'autres directions » demande au LLM de s'en écarter. ✕ Annuler arrête l'écriture.
-->
<script lang="ts">
  import { estAnnulation, messageErreur, proposerDirections, type AtelierDirections, type Direction } from '../lib/api';
  import { aEcarter, alertesDe, apresProposition, type EtatDirections } from '../lib/directions';
  import AnnulerEcriture from '../lib/composants/AnnulerEcriture.svelte';
  import Bouton from '../lib/composants/Bouton.svelte';

  interface Props {
    atelier: AtelierDirections;
    /** L'envie, telle qu'elle est écrite dans l'atelier. */
    envie: string;
    /** Ce que les compléments imposent déjà. */
    complements: string;
    workflow: string | null;
    llm: string | null;
    images?: string[];
    role?: string | null;
    duree?: number | null;
    /** Le LLM est-il là ? */
    disponible: boolean;
    /** Une autre écriture est en cours : on attend. */
    bloque?: boolean;
    /** Les directions de cet atelier, gardées par la page. */
    etat: EtatDirections;
    /** Le LLM propose : la page bloque ses autres écritures. */
    occupe?: boolean;
    signaler: (texte: string, erreur?: boolean) => void;
    /** Pose la direction choisie dans l'idée (ou le fil de l'histoire). */
    prendre: (d: Direction) => void;
    /** Emmène la direction dans 💬 M'aider à préciser. */
    parler?: ((d: Direction) => void) | null;
  }

  let {
    atelier,
    envie,
    complements,
    workflow,
    llm,
    images = [],
    role = null,
    duree = null,
    disponible,
    bloque = false,
    etat = $bindable(),
    occupe = $bindable(false),
    signaler,
    prendre,
    parler = null,
  }: Props = $props();

  const vide = $derived(envie.trim() === '' && complements.trim() === '' && images.length === 0);

  async function proposer() {
    if (occupe) return;
    occupe = true;
    try {
      const r = await proposerDirections({
        atelier,
        envie,
        complements,
        workflow,
        llm,
        images,
        role,
        duree,
        ecartees: etat.proposees.length > 0 || etat.ecartees.length > 0 ? aEcarter(etat) : [],
      });
      const alertes = alertesDe(r.redemandee, r.alertes);
      etat = apresProposition(etat, r.directions, alertes);
      const suite = alertes.length > 0 ? ` · ⚠ ${alertes.join(' ')}` : '';
      signaler(`${r.directions.length} directions proposées par ${r.llm} en ${r.dureeS} s.${suite}`, alertes.length > 0);
    } catch (e) {
      signaler(estAnnulation(e) ? 'Directions annulées.' : messageErreur(e), !estAnnulation(e));
    } finally {
      occupe = false;
    }
  }
</script>

<div class="directions">
  <div class="ligne">
    <Bouton genre="principal" desactive={!disponible || bloque || occupe || vide} onclick={() => void proposer()}>
      {occupe
        ? 'Le LLM cherche…'
        : etat.proposees.length > 0
          ? "🧭 Proposer d'autres directions"
          : '🧭 Proposer des directions'}
    </Bouton>
    {#if occupe}
      <AnnulerEcriture />
    {/if}
    {#if etat.ecartees.length > 0}
      <span class="discret">{etat.ecartees.length} déjà vue{etat.ecartees.length > 1 ? 's' : ''}, que le LLM évite</span>
    {/if}
  </div>
  {#if etat.proposees.length === 0}
    <p class="discret">
      Une envie, même vague : le LLM propose trois directions différentes. Prends-en une et retouche-la, puis ✨ ou
      💬.
    </p>
  {:else}
    <ul class="cartes">
      {#each etat.proposees as d, k (k)}
        <li class="carte">
          <span class="titre">{d.titre}</span>
          <p>{d.texte}</p>
          {#if d.difference}
            <p class="discret">{d.difference}</p>
          {/if}
          <div class="ligne">
            <Bouton petit desactive={occupe} onclick={() => prendre(d)}>Prendre cette direction</Bouton>
            {#if parler}
              <Bouton genre="discret" petit desactive={occupe} onclick={() => parler(d)}>💬 En parler</Bouton>
            {/if}
          </div>
        </li>
      {/each}
    </ul>
    {#each etat.alertes as a (a)}
      <p class="alerte">⚠ {a}</p>
    {/each}
  {/if}
</div>

<style>
  .directions {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin: 10px 0;
  }

  .ligne {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }

  .cartes {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
    gap: 10px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .carte {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 10px;
    border: 1px solid var(--ligne);
  }

  .titre {
    font-weight: 600;
  }

  .carte p {
    margin: 0;
  }

  .alerte {
    margin: 0;
    color: var(--orange);
  }
</style>
