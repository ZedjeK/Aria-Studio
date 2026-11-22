<!--
  Scénario → « 💬 M'aider à préciser », comme en v2 : le prompt s'écrit en conversation. Le LLM pose les questions
  essentielles avec des réponses à cliquer, tient une fiche de ce qu'il a compris, puis écrit le prompt et l'ajuste.
  La conversation est gardée dans ce navigateur ; « Utiliser ce prompt » le pose dans le prompt final.
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import { conversationScenario, messageErreur } from '../lib/api';
  import {
    ECRIRE,
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
    /** Pose ce prompt dans le prompt final. */
    utiliser: (prompt: string) => void;
  }

  let { workflow, llm, contexte, sujet, disponible, signaler, utiliser }: Props = $props();

  const CLE = 'aria-scenario-conversation';

  let messages = $state<MessageConversation[]>([]);
  let saisie = $state('');
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
        forme: 'image',
        workflow,
        contexte,
        llm,
        format: null,
        sujet,
      });
      messages = [...messages, { role: 'assistant', tour }];
      garder();
    } catch (e) {
      // Le message refusé revient dans la saisie, la conversation reste comme avant.
      messages = avant;
      saisie = propre === ECRIRE ? '' : propre;
      signaler(messageErreur(e), true);
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
          <blockquote>{ecrit}</blockquote>
          <Bouton petit onclick={() => utiliser(ecrit)}>Utiliser ce prompt</Bouton>
        {/if}
        <span class="discret">{m.tour.modele} · {m.tour.dureeS} s</span>
      </div>
    {/if}
  {/each}
  {#if enCours}
    <p class="discret" role="status">Le LLM réfléchit…</p>
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
</style>
