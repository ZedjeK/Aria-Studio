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
    conversationGardee,
    echanges,
    reponse,
    type MessageConversation,
  } from '../lib/conversation';
  import Bouton from '../lib/composants/Bouton.svelte';

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

  async function envoyer(texte: string) {
    const propre = texte.trim();
    if (propre === '' || enCours) return;
    const avant = messages;
    messages = [...messages, { role: 'user', texte: propre }];
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
      // Le message refusé revient dans la saisie, la conversation reste comme avant.
      messages = avant;
      saisie = propre === ECRIRE ? '' : propre;
      signaler(estAnnulation(e) ? 'Réponse annulée.' : messageErreur(e), !estAnnulation(e));
    } finally {
      enCours = false;
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

<div class="conversation">
  {#each messages as m, i (i)}
    {#if m.role === 'user'}
      <p class="bulle moi">{m.texte}</p>
    {:else}
      <div class="bulle aria">
        <p>{m.tour.message}</p>
        {#if m.tour.fiche.length > 0}
          <dl>
            {#each m.tour.fiche as r (r.nom)}
              <dt>{r.nom}</dt>
              <dd>{r.valeur}</dd>
            {/each}
          </dl>
        {/if}
        {#if i === messages.length - 1}
          {#each m.tour.questions as q (q.question)}
            <div class="question">
              <span>{q.question}</span>
              <div class="choix">
                {#each q.choix as c (c)}
                  <Bouton genre="discret" petit desactive={enCours} onclick={() => void envoyer(reponse(q.question, c))}>
                    {c}
                  </Bouton>
                {/each}
              </div>
            </div>
          {/each}
        {/if}
        {#if m.tour.prompt}
          {@const ecrit = m.tour.prompt}
          {@const alertes = m.tour.alertes}
          <blockquote>{ecrit}</blockquote>
          {#each alertes as a (a)}
            <p class="alerte">⚠ {a}</p>
          {/each}
          <Bouton petit onclick={() => utiliser(ecrit, alertes)}>{libelleUtiliser}</Bouton>
        {/if}
        <span class="discret">{m.tour.modele} · {m.tour.dureeS} s</span>
      </div>
    {/if}
  {/each}
  {#if enCours}
    <p class="discret" role="status">Le LLM réfléchit… <AnnulerEcriture /></p>
  {/if}
</div>

<textarea
  rows="2"
  maxlength="4000"
  placeholder={messages.length === 0 ? 'Décris ton idée, même vague : le LLM te pose les bonnes questions.' : 'Réponds, ou demande un ajustement.'}
  bind:value={saisie}
  disabled={!disponible}
  onkeydown={(e) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      void envoyer(saisie);
    }
  }}
></textarea>
<div class="actions">
  <Bouton desactive={!disponible || enCours || saisie.trim() === ''} onclick={() => void envoyer(saisie)}>Envoyer</Bouton>
  <Bouton genre="discret" desactive={!disponible || enCours || messages.length === 0} onclick={() => void envoyer(ECRIRE)}>
    Écris le prompt
  </Bouton>
  <Bouton genre="discret" desactive={enCours || messages.length === 0} onclick={recommencer}>Recommencer</Bouton>
</div>

<style>
  .conversation {
    display: flex;
    flex-direction: column;
    gap: 10px;
    max-height: 420px;
    margin: 8px 0;
    overflow-y: auto;
  }

  .bulle {
    max-width: 85%;
    margin: 0;
    padding: 8px 12px;
    border: 1px solid var(--ligne);
  }

  .moi {
    align-self: flex-end;
    white-space: pre-wrap;
  }

  .aria {
    align-self: flex-start;
    background: var(--panneau);
  }

  dl {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 2px 10px;
    margin: 6px 0;
    font-size: 13px;
  }

  dt {
    color: var(--texte-2);
  }

  dd {
    margin: 0;
  }

  .question {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin: 8px 0;
  }

  .choix,
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  blockquote {
    margin: 8px 0;
    padding-left: 10px;
    border-left: 2px solid var(--cyan);
  }

  textarea {
    width: 100%;
  }

  .alerte {
    margin: 4px 0;
    color: var(--orange);
    font-size: 13px;
  }
</style>
