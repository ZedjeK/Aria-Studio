<!--
  Scénario → « 🧭 Me guider » (ateliers Image et Clip), comme en v2 (web/guide.js) et la maquette : un déroulé fixe, sans
  LLM, une question à la fois (« Question 2 sur 6 · Le décor »). Les questions s'enchaînent (le sujet, le décor et
  l'ambiance, la mise en scène) et les réponses vont dans les mêmes choix que les compléments. À chaque question : une
  pastille par réponse, la liste complète, ta propre idée, 🎲 Surprends-moi, Revenir ou Passer. « Utiliser dans la
  conversation » range les réponses dans le champ du 💬 ; elles restent rappelées en puces.
-->
<script lang="ts">
  import type { StructureScenario, SujetScenario } from '../lib/api';
  import {
    PUCES_MAX,
    SECTIONS,
    candidatesHasard,
    etapeCourante,
    etapesGuide,
    guideVide,
    lignesGuide,
    nombreDans,
    numeroQuestion,
    parcours,
    texteReponse,
    type EtapeGuide,
    type EtatGuide,
    type GenreReponse,
    type OptionsGuide,
    type ReponseGuide,
  } from '../lib/guide';
  import type { ContexteListes } from '../lib/scenario';
  import Bouton from '../lib/composants/Bouton.svelte';

  interface Props {
    structure: StructureScenario | null;
    ctx: ContexteListes | null;
    sujets: SujetScenario[];
    options: OptionsGuide;
    /** Ce que le guide dit en tête. */
    aide: string;
    guide: EtatGuide;
    choisirSujet: (id: string) => Promise<void>;
    /** Pose (ou retire, avec une valeur vide) un choix des listes. */
    poser: (bloc: string, sousListe: string, valeur: string) => void;
    poserTexte: (texte: string) => void;
    poserAge: (age: number | null) => void;
    /** Range les réponses dans le champ de la conversation. */
    utiliser: (lignes: string[]) => void;
  }

  let { structure, ctx, sujets, options, aide, guide = $bindable(), choisirSujet, poser, poserTexte, poserAge, utiliser }: Props =
    $props();

  let libre = $state('');

  const etapes = $derived(etapesGuide(structure, guide, ctx, options));
  const etape = $derived(etapeCourante(etapes, guide));
  const choix = $derived(ctx?.choix ?? {});
  const lignes = $derived(lignesGuide(guide, choix, sujets, options.sansSujet));
  const parcoursCourant = $derived(etape?.type === 'liste' ? parcours(etape.valeurs, etape.id, guide) : null);
  /** Les valeurs montrées : celles du parcours, une fois les catégories choisies. */
  const vues = $derived(parcoursCourant && parcoursCourant.prochaines.length === 0 ? parcoursCourant.restantes : []);
  const parCategorie = $derived(
    etape?.type === 'liste' && guide.toute[etape.id] === true && etape.valeurs.some((v) => v.categorie),
  );

  async function repondre(e: EtapeGuide, genre: GenreReponse, valeur = '', texte = '') {
    if (e.type === 'sujet') {
      if (genre === 'valeur') await choisirSujet(valeur);
    } else if (e.type === 'texte') {
      poserTexte(genre === 'libre' ? texte : '');
    } else {
      poser(e.bloc, e.sousListe.nom, genre === 'valeur' || genre === 'hasard' ? valeur : '');
      const age = Number(texte);
      // Un âge précis remplace la tranche.
      if (
        genre === 'libre' &&
        e.bloc === 'personnage' &&
        e.sousListe.nom === 'Âge' &&
        structure &&
        Number.isInteger(age) &&
        age >= structure.agePrecis[0] &&
        age <= structure.agePrecis[1]
      ) {
        poserAge(age);
      }
    }
    const reponse: ReponseGuide = {
      id: e.id,
      onglet: e.onglet,
      titre: e.titre,
      groupe: e.groupe,
      bloc: e.type === 'liste' ? e.bloc : null,
      sousListe: e.type === 'liste' ? e.sousListe.nom : null,
      genre,
      valeur,
      texte,
    };
    const { [e.id]: _chemin, ...chemin } = guide.chemin;
    const { [e.id]: _toute, ...toute } = guide.toute;
    guide = { fait: [...guide.fait, reponse], chemin, toute };
    libre = '';
  }

  function effacer(f: ReponseGuide) {
    if (f.id === 'texte') poserTexte('');
    else if (f.bloc && f.sousListe) poser(f.bloc, f.sousListe, '');
  }

  function revenir() {
    const f = guide.fait.at(-1);
    if (!f) return;
    effacer(f);
    const { [f.id]: _chemin, ...chemin } = guide.chemin;
    const { [f.id]: _toute, ...toute } = guide.toute;
    guide = { fait: guide.fait.slice(0, -1), chemin, toute };
  }

  function refaire() {
    for (const f of [...guide.fait].reverse()) effacer(f);
    guide = guideVide();
  }

  function hasard(e: EtapeGuide) {
    const candidates = candidatesHasard(e, guide, choix);
    const tiree = candidates[Math.floor(Math.random() * candidates.length)];
    void repondre(e, tiree === undefined ? 'passe' : 'hasard', tiree ?? '');
  }

  function categorie(e: EtapeGuide, nom: string) {
    guide = { ...guide, chemin: { ...guide.chemin, [e.id]: [...(guide.chemin[e.id] ?? []), nom] } };
  }

  function retourCategorie(e: EtapeGuide) {
    const chemin = guide.chemin[e.id] ?? [];
    if (chemin.length > 0) {
      guide = { ...guide, chemin: { ...guide.chemin, [e.id]: chemin.slice(0, -1) } };
    } else {
      const { [e.id]: _toute, ...toute } = guide.toute;
      guide = { ...guide, toute };
    }
  }

  function toute(e: EtapeGuide) {
    const { [e.id]: _chemin, ...chemin } = guide.chemin;
    guide = { ...guide, chemin, toute: { ...guide.toute, [e.id]: true } };
  }

  function valider(e: EtapeGuide) {
    const t = libre.trim();
    if (t !== '') void repondre(e, 'libre', '', t);
  }

  const indice = (e: EtapeGuide): string =>
    e.type === 'sujet'
      ? 'Ou décris-le : un dragon, une moto…'
      : e.type === 'texte'
        ? 'Ex. ARIA, écrit en néon'
        : e.sousListe.nom === 'Âge'
          ? 'Ou un âge précis : 34…'
          : 'Ou écris ta propre idée…';
</script>

<div class="guide">
  <p class="discret">{aide}</p>

  {#if lignes.length > 0}
    <div class="puces" role="group" aria-label="Déjà noté">
      <span class="petit-texte">Noté :</span>
      {#each guide.fait as f (f.id)}
        {#if f.genre !== 'passe' && !(options.sansSujet && f.onglet === 'sujet')}
          <span class="puce">{texteReponse(f, sujets)}</span>
        {/if}
      {/each}
    </div>
  {/if}

  {#if etape}
    {@const e = etape}
    {@const q = numeroQuestion(etapes, e)}
    <p class="etiquette-champ">Question {q.numero} sur {q.total} · {SECTIONS[e.onglet]}{e.groupe && e.groupe !== e.titre ? ` · ${e.groupe}` : ''}</p>
    <p class="question">{e.titre}</p>
    {#if e.type === 'liste' && e.sousListe.note}<p class="discret">{e.sousListe.note}</p>{/if}

    <div class="choix-pastilles">
      {#if e.type === 'sujet'}
        {#each sujets as s (s.id)}
          <button type="button" onclick={() => void repondre(e, 'valeur', s.id)}>{s.nom}</button>
        {/each}
      {:else if e.type === 'liste' && parcoursCourant}
        {#if parcoursCourant.chemin.length > 0}
          <span class="discret">{parcoursCourant.chemin.join(' › ')}</span>
        {/if}
        {#if parcoursCourant.prochaines.length > 0}
          {#each parcoursCourant.prochaines as c (c)}
            <button type="button" onclick={() => categorie(e, c)}>
              {c} <span class="discret">{nombreDans(parcoursCourant.restantes, parcoursCourant.chemin.length, c)}</span>
            </button>
          {/each}
          <button type="button" onclick={() => toute(e)}>Toute la liste</button>
        {:else}
          {#each vues.slice(0, PUCES_MAX) as v (v.nom)}
            <button
              type="button"
              class:pris={choix[e.bloc]?.[e.sousListe.nom] === v.nom}
              onclick={() => void repondre(e, 'valeur', v.nom)}
            >
              {v.nom}
            </button>
          {/each}
        {/if}
        {#if parcoursCourant.chemin.length > 0}
          <button type="button" onclick={() => retourCategorie(e)}>
            ↩ {parcoursCourant.chemin.length > 1 ? parcoursCourant.chemin.at(-2) : 'Autre catégorie'}
          </button>
        {:else if parCategorie}
          <button type="button" onclick={() => retourCategorie(e)}>↩ Par catégorie</button>
        {/if}
        {#if vues.length > PUCES_MAX}
          <select
            aria-label={e.titre}
            value=""
            onchange={(ev) => {
              const v = ev.currentTarget.value;
              if (v) void repondre(e, 'valeur', v);
            }}
          >
            <option value="" disabled>Toute la liste ({vues.length - PUCES_MAX} de plus)…</option>
            {#each vues.slice(PUCES_MAX) as v (v.nom)}
              <option value={v.nom}>{v.categorie ? `${v.categorie} · ` : ''}{v.nom}</option>
            {/each}
          </select>
        {/if}
      {/if}
    </div>

    <form
      class="libre"
      onsubmit={(ev) => {
        ev.preventDefault();
        valider(e);
      }}
    >
      <input type="text" maxlength="200" bind:value={libre} placeholder={indice(e)} aria-label="Ta propre idée" />
      <Bouton petit genre="discret" type="submit" desactive={libre.trim() === ''}>Valider</Bouton>
    </form>
  {:else}
    <p class="question">{lignes.length > 0 ? "Voilà ce que j'ai noté :" : "Rien de noté pour l'instant."}</p>
    {#if lignes.length > 0}
      <ul>
        {#each lignes as l (l)}<li>{l}</li>{/each}
      </ul>
    {/if}
  {/if}

  <div class="actions">
    {#if etape && etape.type === 'liste'}
      <Bouton petit onclick={() => hasard(etape)}>🎲 Surprends-moi</Bouton>
    {/if}
    <Bouton petit genre="discret" desactive={guide.fait.length === 0} onclick={revenir}>← Revenir</Bouton>
    {#if etape}
      <Bouton petit genre="discret" onclick={() => void repondre(etape, 'passe')}>Passer →</Bouton>
    {/if}
    <Bouton petit genre="discret" desactive={guide.fait.length === 0} onclick={refaire}>↺ Tout refaire</Bouton>
    <Bouton petit desactive={lignes.length === 0} onclick={() => utiliser(lignes)}>Utiliser dans la conversation</Bouton>
  </div>
</div>

<style>
  .guide {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .guide p {
    margin: 0;
  }

  .etiquette-champ {
    color: var(--texte-2);
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.14em;
    text-transform: uppercase;
  }

  .question {
    font-size: 15px;
  }

  .petit-texte {
    font-size: 12px;
    color: var(--texte-2);
  }

  .puces,
  .actions,
  .libre {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }

  .puces {
    gap: 6px;
  }

  .puce {
    padding: 2px 10px;
    border: 1px solid var(--cyan);
    color: var(--cyan);
    font-size: 11.5px;
    letter-spacing: 0.04em;
  }

  .choix-pastilles {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  .choix-pastilles button {
    padding: 6px 14px;
    border: 1px solid var(--ligne);
    background: var(--fond);
    color: var(--texte);
    font: inherit;
    font-size: 13px;
    cursor: pointer;
  }

  .choix-pastilles button:hover {
    border-color: var(--cyan);
  }

  .choix-pastilles button.pris {
    border-color: var(--cyan);
    background: color-mix(in srgb, var(--cyan) 10%, var(--fond));
    color: var(--cyan);
  }

  .libre input {
    flex: 1;
    min-width: 200px;
  }

  ul {
    margin: 0;
    padding-left: 18px;
  }
</style>
