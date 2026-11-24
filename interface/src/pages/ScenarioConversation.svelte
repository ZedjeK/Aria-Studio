<!--
  Scénario → « 💬 M'aider à préciser », comme en v2 : le prompt s'écrit en conversation. Le LLM pose les questions
  essentielles avec des réponses à cliquer, tient une fiche de ce qu'il a compris, puis écrit le prompt et l'ajuste.
  Tous les ateliers : image, retouche (le LLM voit l'image), clip (le rush à animer, la durée), le fil d'une séquence,
  et un plan d'une séquence (ce que sait le découpage). Chaque conversation est gardée dans ce navigateur ;
  « Utiliser ce prompt » le pose où il va.
-->
<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import {
    conversationScenario,
    estAnnulation,
    messageErreur,
    type DemandeDecoupage,
    type FormeConversation,
  } from '../lib/api';
  import AnnulerEcriture from '../lib/composants/AnnulerEcriture.svelte';
  import { ajouterLignes } from '../lib/guide';
  import {
    ECRIRE,
    cleConversation,
    comprisDernier,
    conversationGardee,
    demandeRetrait,
    echanges,
    pourModifier,
    pourReessayer,
    reponse,
    texteACopier,
    unTourEnArriere,
    type MessageConversation,
  } from '../lib/conversation';
  import Bouton from '../lib/composants/Bouton.svelte';
  import ScenarioAlerte from './ScenarioAlerte.svelte';

  interface Props {
    workflow: string | null;
    llm: string | null;
    /** Ce que les compléments imposent déjà. */
    contexte: string;
    sujet: string;
    /** Le LLM est-il là ? */
    disponible: boolean;
    signaler: (texte: string, erreur?: boolean) => void;
    /** Pose ce prompt là où il va (le prompt final, le fil de l'histoire, le prompt du plan), avec ses alertes. */
    utiliser: (prompt: string, alertes: string[]) => void;
    /** L'atelier (absent : l'image). */
    forme?: FormeConversation;
    /** L'image à retoucher, le rush à animer, ou les images de référence. */
    images?: string[];
    role?: string | null;
    duree?: number | null;
    format?: string | null;
    /** Clip : sa réplique, mot pour mot, et sa musique d'ambiance. */
    replique?: string | null;
    musique?: string | null;
    sansSujet?: boolean;
    /** 💬 d'un plan : la demande du découpage, avec le numéro du plan. */
    sequence?: DemandeDecoupage | null;
    /** Le libellé du bouton qui pose le prompt. */
    libelleUtiliser?: string;
    /** Des lignes à ajouter au champ (les réponses du 🧭 guide) ; `amorceLue` les retire une fois posées. */
    amorce?: string;
    amorceLue?: () => void;
  }

  let {
    workflow,
    llm,
    contexte,
    sujet,
    disponible,
    signaler,
    utiliser,
    forme = 'image',
    images = [],
    role = null,
    duree = null,
    format = null,
    replique = null,
    musique = null,
    sansSujet = false,
    sequence = null,
    libelleUtiliser = 'Utiliser ce prompt',
    amorce = '',
    amorceLue,
  }: Props = $props();

  const CLE = $derived(cleConversation(forme, sequence?.plan ?? null));

  let messages = $state<MessageConversation[]>([]);
  let saisie = $state('');

  // Les réponses du guide arrivent dans le champ, sans doublon, prêtes à envoyer.
  $effect(() => {
    const lignes = amorce;
    if (lignes.trim() === '') return;
    untrack(() => {
      saisie = ajouterLignes(saisie, lignes.split('\n'));
      amorceLue?.();
    });
  });
  let enCours = $state(false);

  function garder() {
    try {
      localStorage.setItem(CLE, JSON.stringify(messages));
    } catch {
      // stockage indisponible : la conversation vaut pour cette fois
    }
  }

  /** Envoie un message. `base` : la conversation d'où il repart (↻ Réessayer revient avant la dernière réponse). */
  async function envoyer(texte: string, base: MessageConversation[] = messages) {
    const propre = texte.trim();
    if (propre === '' || enCours) return;
    const avant = messages;
    messages = [...base, { role: 'user', texte: propre }];
    saisie = '';
    enCours = true;
    try {
      const tour = await conversationScenario({
        echanges: echanges(messages),
        forme,
        workflow,
        contexte,
        llm,
        format,
        replique,
        musique,
        sujet,
        images,
        role,
        duree,
        sansSujet,
        sequence,
      });
      messages = [...messages, { role: 'assistant', tour }];
      garder();
    } catch (e) {
      // La conversation reste comme avant ; le message refusé revient dans la saisie.
      messages = avant;
      if (base === avant) saisie = propre === ECRIRE ? '' : propre;
      signaler(estAnnulation(e) ? 'Réponse annulée.' : messageErreur(e), !estAnnulation(e));
    } finally {
      enCours = false;
    }
  }

  function reessayer() {
    const r = pourReessayer(messages);
    if (r) void envoyer(r.texte, r.avant);
  }

  function modifier(i: number) {
    const r = pourModifier(messages, i);
    if (!r || enCours) return;
    messages = r.avant;
    saisie = r.texte;
    garder();
  }

  function enArriere() {
    messages = unTourEnArriere(messages);
    garder();
  }

  async function copier(m: MessageConversation) {
    try {
      await navigator.clipboard.writeText(texteACopier(m));
      signaler('Copié.');
    } catch {
      signaler('Impossible de copier : le presse-papiers est refusé.', true);
    }
  }

  function recommencer() {
    messages = [];
    saisie = '';
    garder();
  }

  onMount(() => {
    try {
      const brut = localStorage.getItem(CLE);
      if (brut) messages = conversationGardee(JSON.parse(brut));
    } catch {
      // conversation illisible ou stockage indisponible : on repart de zéro
    }
  });
</script>

{#if messages.length > 0}
  {@const compris = comprisDernier(messages)}
  <div class="entete">
    <div class="puces">
      {#if compris.length > 0}
        <span class="petit-texte">Compris :</span>
        {#each compris as c (c.nom)}
          <span class="puce" title={c.nom}>
            {c.valeur}
            <button
              type="button"
              aria-label="Retirer : {c.nom}"
              disabled={enCours || !disponible}
              onclick={() => void envoyer(demandeRetrait(c.nom, c.valeur))}>✕</button
            >
          </span>
        {/each}
      {/if}
    </div>
    <div class="actions">
      <Bouton genre="discret" petit desactive={enCours} onclick={enArriere}>↩️ Un tour en arrière</Bouton>
      <Bouton genre="discret" petit desactive={enCours} onclick={recommencer}>🔄 Recommencer</Bouton>
    </div>
  </div>
{/if}

<div class="fil">
  {#each messages as m, i (i)}
    {#if m.role === 'user'}
      <div class="bulle moi">
        {m.texte === ECRIRE ? 'Écris le prompt.' : m.texte}
        <div class="outils">
          {#if m.texte !== ECRIRE}
            <button type="button" disabled={enCours} onclick={() => modifier(i)}>✎ Modifier</button>
          {/if}
          <button type="button" onclick={() => void copier(m)}>⧉ Copier</button>
        </div>
      </div>
    {:else}
      <div class="bulle">
        <b class="petit-texte">LLM</b>
        <p>{m.tour.message}</p>
        {#if i === messages.length - 1}
          {#each m.tour.questions as q (q.question)}
            <p class="question">{q.question}</p>
            <div class="propositions">
              {#each q.choix as c (c)}
                <button type="button" disabled={enCours} onclick={() => void envoyer(reponse(q.question, c))}>{c}</button>
              {/each}
            </div>
          {/each}
        {/if}
        {#if m.tour.prompt}
          {@const ecrit = m.tour.prompt}
          {@const alertes = m.tour.alertes}
          <blockquote>{ecrit}</blockquote>
          {#each alertes as a (a)}
            <ScenarioAlerte>{a}</ScenarioAlerte>
          {/each}
          <Bouton petit onclick={() => utiliser(ecrit, alertes)}>{libelleUtiliser}</Bouton>
        {/if}
        <div class="outils">
          {#if i === messages.length - 1}
            <button type="button" disabled={enCours} onclick={reessayer}>↻ Réessayer</button>
          {/if}
          <button type="button" onclick={() => void copier(m)}>⧉ Copier</button>
          <span>{m.tour.modele} · {m.tour.dureeS} s</span>
        </div>
      </div>
    {/if}
  {/each}
  {#if enCours}
    <p class="bulle attente" role="status">Le LLM réfléchit…</p>
  {/if}
</div>

<div class="ligne">
  <textarea
    rows="2"
    maxlength="4000"
    placeholder={messages.length === 0
      ? 'Décris ton idée, même vague : le LLM te pose les bonnes questions.'
      : 'Ta réponse (Entrée envoie, Maj+Entrée va à la ligne)'}
    bind:value={saisie}
    disabled={!disponible}
    onkeydown={(e) => {
      if (e.key === 'Enter' && !e.shiftKey) {
        e.preventDefault();
        void envoyer(saisie);
      }
    }}
  ></textarea>
  <Bouton petit desactive={!disponible || enCours || saisie.trim() === ''} onclick={() => void envoyer(saisie)}>Envoyer</Bouton>
  <Bouton petit desactive={!disponible || enCours || messages.length === 0} onclick={() => void envoyer(ECRIRE)}>
    ✍️ Écris-le
  </Bouton>
  {#if enCours}
    <AnnulerEcriture />
  {/if}
</div>

<style>
  .entete {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 8px 16px;
    margin-bottom: 10px;
  }

  .puces,
  .actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
  }

  .petit-texte {
    font-size: 12px;
    color: var(--texte-2);
  }

  .puce {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 2px 10px;
    border: 1px solid var(--cyan);
    color: var(--cyan);
    font-size: 11.5px;
    letter-spacing: 0.04em;
  }

  .puce button {
    padding: 0;
    border: 0;
    background: none;
    color: inherit;
    font-size: 12px;
    cursor: pointer;
  }

  .fil {
    display: flex;
    flex-direction: column;
    gap: 10px;
    max-height: 300px;
    padding: 4px 2px;
    overflow-y: auto;
  }

  .bulle {
    align-self: flex-start;
    max-width: 82%;
    margin: 0;
    padding: 10px 14px;
    border: 1px solid var(--ligne);
    background: var(--fond);
    font-size: 13.5px;
  }

  .bulle p {
    margin: 2px 0;
  }

  .moi {
    align-self: flex-end;
    border-color: var(--bleu);
    background: color-mix(in srgb, var(--bleu) 12%, var(--fond));
    white-space: pre-wrap;
  }

  .attente {
    color: var(--texte-2);
  }

  .question {
    margin-top: 8px;
  }

  .propositions {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin-top: 8px;
  }

  .propositions button {
    padding: 3px 12px;
    border: 1px solid var(--cyan);
    background: none;
    color: var(--cyan);
    font-size: 12.5px;
    cursor: pointer;
  }

  .propositions button:hover:not(:disabled) {
    background: color-mix(in srgb, var(--cyan) 12%, transparent);
  }

  .outils {
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
    margin-top: 6px;
    color: var(--texte-2);
    font-size: 12px;
  }

  .outils button {
    padding: 0;
    border: 0;
    background: none;
    color: inherit;
    font-size: inherit;
    cursor: pointer;
  }

  .outils button:hover:not(:disabled) {
    color: var(--texte);
  }

  button:disabled {
    opacity: 0.5;
    cursor: default;
  }

  blockquote {
    margin: 8px 0;
    padding-left: 10px;
    border-left: 2px solid var(--cyan);
  }

  .ligne {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px;
    margin-top: 10px;
  }

  textarea {
    flex: 1;
    min-width: 220px;
  }
</style>
