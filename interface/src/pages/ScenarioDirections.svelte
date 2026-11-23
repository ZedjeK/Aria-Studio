<!--
  « Proposer des directions » (nouveauté de la v3) : à partir d'une envie vague, le LLM propose trois directions
  distinctes, en cartes à onglet (un choix exclusif : celle qui se coche). « Prendre cette direction » pose la carte
  cochée dans l'idée (ou le fil de l'histoire), à retoucher puis à écrire avec ✨ ; « 💬 En parler » l'emmène dans
  M'aider à préciser. Les directions vues restent tant qu'on reste dans l'atelier, et « Proposer d'autres directions »
  demande au LLM de s'en écarter. ✕ Annuler arrête l'écriture. Les boutons sont secondaires : l'orange est celui du
  prompt final. La rangée du haut reçoit ✨ Écrire de la page (snippet `avant`).
-->
<script lang="ts">
  import { estAnnulation, messageErreur, proposerDirections, type AtelierDirections, type Direction } from '../lib/api';
  import { aEcarter, alertesDe, apresProposition, type EtatDirections } from '../lib/directions';
  import type { Snippet } from 'svelte';
  import AnnulerEcriture from '../lib/composants/AnnulerEcriture.svelte';
  import Bouton from '../lib/composants/Bouton.svelte';
  import CarteChoix from '../lib/composants/CarteChoix.svelte';
  import ScenarioAlerte from './ScenarioAlerte.svelte';

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
    /** Ce que la page pose au début de la rangée de boutons (✨ Écrire le prompt). */
    avant?: Snippet;
    /** Une écriture de la page est en cours : ✕ Annuler la montre aussi. */
    ecritureEnCours?: boolean;
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
    avant,
    ecritureEnCours = false,
  }: Props = $props();

  /** La carte cochée (son rang parmi les directions proposées), vide tant qu'aucune ne l'est. */
  let choisie = $state('');
  const direction = $derived(choisie === '' ? undefined : etat.proposees[Number(choisie)]);

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
      choisie = '';
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
    {@render avant?.()}
    <Bouton genre="secondaire" petit desactive={!disponible || bloque || occupe || vide} onclick={() => void proposer()}>
      {occupe ? 'Le LLM cherche…' : '🧭 Proposer des directions'}
    </Bouton>
    {#if occupe || ecritureEnCours}
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
    <p class="etiquette">Trois directions, vraiment différentes</p>
    <div class="cartes">
      {#each etat.proposees as d, k (k)}
        <CarteChoix groupe="direction" valeur={String(k)} titre={d.titre} bind:choisie>
          {d.texte}
          {#if d.difference}
            <span class="difference">Se distingue par : {d.difference}</span>
          {/if}
        </CarteChoix>
      {/each}
    </div>
    <div class="ligne">
      <Bouton petit desactive={occupe || !direction} onclick={() => direction && prendre(direction)}>
        Prendre cette direction
      </Bouton>
      {#if parler}
        <Bouton genre="discret" petit desactive={occupe || !direction} onclick={() => direction && parler(direction)}>
          💬 En parler
        </Bouton>
      {/if}
      <Bouton genre="discret" petit desactive={!disponible || bloque || occupe || vide} onclick={() => void proposer()}>
        Proposer d'autres directions
      </Bouton>
    </div>
    {#each etat.alertes as a (a)}
      <ScenarioAlerte>{a}</ScenarioAlerte>
    {/each}
  {/if}
</div>

<style>
  .directions {
    display: flex;
    flex-direction: column;
    gap: 10px;
    margin: 10px 0;
  }

  .ligne {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px;
  }

  .etiquette {
    margin: 0;
    color: var(--texte-2);
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.14em;
    text-transform: uppercase;
  }

  .cartes {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
    gap: 12px;
    align-items: stretch;
  }

  .difference {
    display: block;
    margin-top: 6px;
    font-size: 12px;
  }
</style>
