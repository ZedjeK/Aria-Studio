<!--
  Scénario → « 🧭 Me guider » (ateliers Image et Clip), comme en v2 (web/guide.js) : un déroulé fixe, en bulles, sans
  LLM. Les questions s'enchaînent (le sujet, le décor et l'ambiance, la mise en scène) et les réponses vont dans les
  mêmes choix que les compléments. À chaque étape : une puce, la liste complète, ta propre idée, Surprends-moi, Passer
  ou Revenir. À la fin, « Utiliser dans la conversation » range les réponses dans le champ du 💬.
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

  /** L'en-tête de section, quand on passe d'une section à l'autre. */
  function section(i: number, onglet: ReponseGuide['onglet']): string | null {
    const avant = i === 0 ? null : guide.fait[i - 1]?.onglet;
    return avant === onglet ? null : SECTIONS[onglet];
  }

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
  <div class="bulle llm">
    <p>{aide}</p>
    <p class="discret">Je te pose les questions une à une. À chaque étape : une réponse de la liste, ta propre idée, ou Passer.</p>
  </div>

  {#each guide.fait as f, i (f.id)}
    {@const titreSection = section(i, f.onglet)}
    {#if titreSection}<div class="section">{titreSection}</div>{/if}
    <div class="bulle llm">
      <p><strong>{f.titre}</strong>{#if f.groupe && f.groupe !== f.titre}<span class="discret"> · {f.groupe}</span>{/if}</p>
    </div>
    <div class="bulle toi"><p>{texteReponse(f, sujets)}</p></div>
  {/each}

  {#if etape}
    {@const e = etape}
    {#if section(guide.fait.length, e.onglet)}<div class="section">{SECTIONS[e.onglet]}</div>{/if}
    <div class="bulle llm">
      <p><strong>{e.titre}</strong>{#if e.groupe && e.groupe !== e.titre}<span class="discret"> · {e.groupe}</span>{/if}</p>
      {#if e.type === 'liste' && e.sousListe.note}<p class="discret">{e.sousListe.note}</p>{/if}

      <div class="puces">
        {#if e.type === 'sujet'}
          {#each sujets as s (s.id)}
            <button type="button" class="pastille" onclick={() => void repondre(e, 'valeur', s.id)}>{s.nom}</button>
          {/each}
        {:else if e.type === 'liste' && parcoursCourant}
          {#if parcoursCourant.chemin.length > 0}
            <span class="discret">{parcoursCourant.chemin.join(' › ')}</span>
          {/if}
          {#if parcoursCourant.prochaines.length > 0}
            {#each parcoursCourant.prochaines as c (c)}
              <button type="button" class="pastille" onclick={() => categorie(e, c)}>
                {c} <span class="discret">{nombreDans(parcoursCourant.restantes, parcoursCourant.chemin.length, c)}</span>
              </button>
            {/each}
            <button type="button" class="pastille" onclick={() => toute(e)}>Toute la liste</button>
          {:else}
            {#each vues.slice(0, PUCES_MAX) as v (v.nom)}
              <button
                type="button"
                class="pastille"
                aria-pressed={choix[e.bloc]?.[e.sousListe.nom] === v.nom}
                onclick={() => void repondre(e, 'valeur', v.nom)}
              >
                {v.nom}
              </button>
            {/each}
          {/if}
          {#if parcoursCourant.chemin.length > 0}
            <button type="button" class="pastille" onclick={() => retourCategorie(e)}>
              ↩ {parcoursCourant.chemin.length > 1 ? parcoursCourant.chemin.at(-2) : 'Autre catégorie'}
            </button>
          {:else if parCategorie}
            <button type="button" class="pastille" onclick={() => retourCategorie(e)}>↩ Par catégorie</button>
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
        <Bouton petit type="submit" desactive={libre.trim() === ''}>Valider</Bouton>
      </form>
      <div class="actions">
        {#if guide.fait.length > 0}
          <Bouton petit genre="discret" onclick={revenir}>← Revenir</Bouton>
        {/if}
        {#if e.type === 'liste'}
          <Bouton petit genre="discret" onclick={() => hasard(e)}>🎲 Surprends-moi</Bouton>
        {/if}
        <Bouton petit genre="discret" onclick={() => void repondre(e, 'passe')}>Passer →</Bouton>
      </div>
    </div>
  {:else}
    <div class="bulle llm">
      <p>{lignes.length > 0 ? "Voilà ce que j'ai noté :" : "Rien de noté pour l'instant."}</p>
      {#if lignes.length > 0}
        <ul>
          {#each lignes as l (l)}<li>{l}</li>{/each}
        </ul>
      {/if}
      <div class="actions">
        <Bouton petit genre="discret" onclick={revenir}>← Revenir</Bouton>
        <Bouton petit genre="discret" onclick={refaire}>↺ Tout refaire</Bouton>
        <Bouton petit genre="principal" desactive={lignes.length === 0} onclick={() => utiliser(lignes)}>
          Utiliser dans la conversation
        </Bouton>
      </div>
    </div>
  {/if}
</div>

<style>
  .guide {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin: 8px 0;
  }

  .bulle {
    max-width: 90%;
    padding: 8px 12px;
    border: 1px solid var(--ligne);
  }

  .bulle p {
    margin: 4px 0;
  }

  .llm {
    align-self: flex-start;
  }

  .toi {
    align-self: flex-end;
    border-color: var(--cyan);
  }

  .section {
    margin-top: 6px;
    font-size: 12px;
    color: var(--texte-2);
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .pastille {
    padding: 4px 12px;
    border: 1px solid var(--ligne);
    background: transparent;
    color: var(--texte-2);
    font: inherit;
    cursor: pointer;
  }

  .pastille[aria-pressed='true'] {
    border-color: var(--cyan);
    color: var(--texte);
  }

  .puces,
  .actions,
  .libre {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    margin: 6px 0;
  }

  .libre input {
    flex: 1;
    min-width: 200px;
  }

  ul {
    margin: 4px 0;
    padding-left: 18px;
  }
</style>
