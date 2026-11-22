<!--
  Scénario, comme en v2 (l'atelier Image) : ton idée, que le LLM écrit en prompt pour le modèle cible (✨ Écrire le
  prompt), et des compléments : listes à cliquer rangées en trois onglets (Sujet, Décor et ambiance, Mise en scène),
  âge précis, texte dans l'image et précisions traduites par le LLM. Le prompt assemblé suit les compléments en
  direct (règles des listes et filtre AS-18) ; le prompt final, modifiable, part tel quel au Plateau. Les règles sont
  appliquées ici pour guider ; c'est le cœur qui fait foi.
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import {
    assemblerPrompt,
    ecrirePromptScenario,
    messageErreur,
    optionsPlateau,
    structureScenario,
    sujetsScenario,
    traduirePrecisions,
    type Assemblage,
    type ChoixLlm,
    type ChoixListes,
    type ChoixWorkflow,
    type OngletScenario,
    type StructureScenario,
    type SujetScenario,
  } from '../lib/api';
  import {
    ONGLETS,
    choixDansOnglet,
    choixDuBrouillon,
    contexteListes,
    incompatible,
    poserChoix,
    sousListeVisible,
    valeursProposees,
  } from '../lib/scenario';
  import ASavoir from '../lib/composants/ASavoir.svelte';
  import Bouton from '../lib/composants/Bouton.svelte';
  import Panneau from '../lib/composants/Panneau.svelte';
  import SousOnglets from '../lib/composants/SousOnglets.svelte';

  interface Props {
    signaler: (texte: string, erreur?: boolean) => void;
    /** Envoie le prompt final au Plateau, avec le modèle cible. */
    envoyer: (prompt: string, workflow: string | null) => void;
  }

  let { signaler, envoyer }: Props = $props();

  const BROUILLON = 'aria-scenario';

  let sujets = $state<SujetScenario[]>([]);
  let sujet = $state('personnage');
  let structure = $state<StructureScenario | null>(null);
  /** Les choix de chaque sujet. */
  let choixParSujet = $state<Record<string, ChoixListes>>({});
  let age = $state<number | null>(null);
  let texte = $state('');
  let toutes = $state(false);
  let workflows = $state<ChoixWorkflow[]>([]);
  let workflowId = $state<string | null>(null);
  let llms = $state<ChoixLlm[]>([]);
  let llmParWorkflow = $state<Record<string, string>>({});
  /** Le LLM choisi ; vide : celui du modèle cible. */
  let llmChoisi = $state('');
  let idee = $state('');
  let precisions = $state('');
  /** La traduction des précisions, et le texte qu'elle traduit (elle ne vaut plus s'il change). */
  let traduction = $state<{ de: string; en: string } | null>(null);
  /** Le LLM travaille (écriture ou traduction). */
  let ecriture = $state<'ecrire' | 'traduire' | null>(null);
  let assemblage = $state<Assemblage | null>(null);
  let erreurAssemblage = $state<string | null>(null);
  let final = $state('');
  /** Le prompt final a été retouché à la main : l'assemblage ne l'écrase plus. */
  let finalRetouche = $state(false);
  let minuteur: ReturnType<typeof setTimeout> | undefined;

  const choix = $derived(choixParSujet[sujet] ?? {});
  const ctx = $derived(structure ? contexteListes(structure, choix, age, toutes) : null);
  const ongletsAvecCompte = $derived(
    ONGLETS.map((o) => {
      const n = structure ? choixDansOnglet(structure, choix, o.id) : 0;
      return { id: o.id, titre: n > 0 ? `${o.titre} (${n})` : o.titre };
    }),
  );
  let ongletChoisi = $state<string>('sujet');
  const onglet = $derived<OngletScenario>(
    ongletChoisi === 'decor' || ongletChoisi === 'scene' ? ongletChoisi : 'sujet',
  );

  function garder() {
    try {
      localStorage.setItem(
        BROUILLON,
        JSON.stringify({ sujet, choixParSujet, age, texte, toutes, workflowId, idee, precisions, traduction }),
      );
    } catch {
      // stockage indisponible : le brouillon vaut pour cette fois
    }
  }

  function relireBrouillon() {
    try {
      const brut = localStorage.getItem(BROUILLON);
      if (!brut) return;
      const d: unknown = JSON.parse(brut);
      if (typeof d !== 'object' || d === null) return;
      if ('sujet' in d && typeof d.sujet === 'string') sujet = d.sujet;
      if ('choixParSujet' in d) choixParSujet = choixDuBrouillon(d.choixParSujet);
      if ('age' in d && typeof d.age === 'number') age = d.age;
      if ('texte' in d && typeof d.texte === 'string') texte = d.texte;
      if ('toutes' in d && typeof d.toutes === 'boolean') toutes = d.toutes;
      if ('workflowId' in d && typeof d.workflowId === 'string') workflowId = d.workflowId;
      if ('idee' in d && typeof d.idee === 'string') idee = d.idee;
      if ('precisions' in d && typeof d.precisions === 'string') precisions = d.precisions;
      if (
        'traduction' in d &&
        typeof d.traduction === 'object' &&
        d.traduction !== null &&
        'de' in d.traduction &&
        'en' in d.traduction &&
        typeof d.traduction.de === 'string' &&
        typeof d.traduction.en === 'string'
      ) {
        traduction = { de: d.traduction.de, en: d.traduction.en };
      }
    } catch {
      // brouillon illisible ou stockage indisponible : on repart de zéro
    }
  }

  async function choisirSujet(id: string) {
    sujet = id;
    try {
      structure = await structureScenario(id);
    } catch (e) {
      signaler(messageErreur(e), true);
    }
    planifier();
  }

  function changer(bloc: string, sousListe: string, valeur: string) {
    if (!structure) return;
    choixParSujet = { ...choixParSujet, [sujet]: poserChoix(structure, choix, bloc, sousListe, valeur) };
    planifier();
  }

  function toutEffacer() {
    choixParSujet = { ...choixParSujet, [sujet]: {} };
    age = null;
    texte = '';
    precisions = '';
    traduction = null;
    finalRetouche = false;
    planifier();
  }

  /** Chaque changement repasse par le cœur, un peu après la dernière frappe. */
  function planifier() {
    clearTimeout(minuteur);
    garder();
    minuteur = setTimeout(() => void assembler(), 250);
  }

  async function assembler() {
    try {
      const r = await assemblerPrompt({
        sujet,
        choix,
        age,
        texte,
        precisions: traductionValide ? (traduction?.en ?? '') : '',
        workflow: workflowId,
        action: '',
        replique: '',
        dialogue: null,
        musique: '',
        avecMusique: null,
        sansSujet: false,
      });
      assemblage = r;
      erreurAssemblage = null;
      if (!finalRetouche) final = r.prompt;
    } catch (e) {
      erreurAssemblage = messageErreur(e);
    }
  }

  /** Le LLM de l'atelier : celui choisi, sinon celui du modèle cible. */
  const llmEffectif = $derived(llmChoisi || (workflowId ? (llmParWorkflow[workflowId] ?? null) : null));
  const traductionValide = $derived(traduction !== null && traduction.de === precisions.trim());
  /** Ce que les compléments imposent : le prompt assemblé, s'il y a un choix. */
  const complements = $derived(
    Object.values(choix).some((b) => Object.keys(b).length > 0) || age !== null || texte.trim() !== '' || traductionValide
      ? (assemblage?.prompt ?? '')
      : '',
  );

  async function traduire() {
    const de = precisions.trim();
    if (de === '' || ecriture) return;
    ecriture = 'traduire';
    try {
      traduction = { de, en: await traduirePrecisions(de, llmEffectif) };
      planifier();
    } catch (e) {
      signaler(messageErreur(e), true);
    } finally {
      ecriture = null;
    }
  }

  async function ecrire() {
    if (ecriture) return;
    ecriture = 'ecrire';
    try {
      const r = await ecrirePromptScenario({
        atelier: 'image',
        idee,
        complements,
        workflow: workflowId,
        llm: llmChoisi || null,
      });
      final = r.prompt;
      finalRetouche = true;
      signaler(`Prompt écrit par ${r.llm} en ${r.dureeS} s.`);
    } catch (e) {
      signaler(messageErreur(e), true);
    } finally {
      ecriture = null;
    }
  }

  function retoucherFinal(valeur: string) {
    final = valeur;
    finalRetouche = valeur !== (assemblage?.prompt ?? '');
  }

  function reprendreAssemblage() {
    finalRetouche = false;
    final = assemblage?.prompt ?? '';
  }

  onMount(() => {
    relireBrouillon();
    void (async () => {
      try {
        sujets = await sujetsScenario();
        const options = await optionsPlateau();
        workflows = options.workflows;
        llms = options.llm;
        llmParWorkflow = options.llmParWorkflow;
        if (!workflows.some((w) => w.id === workflowId)) workflowId = options.defaut;
      } catch (e) {
        signaler(messageErreur(e), true);
      }
      await choisirSujet(sujets.some((s) => s.id === sujet) ? sujet : 'personnage');
    })();
    return () => clearTimeout(minuteur);
  });
</script>

<h1>Scénario</h1>
<p class="sous-titre">Compose ton prompt avec les listes guidées, puis envoie-le au Plateau.</p>
<ASavoir>
  Chaque liste ajoute sa traduction anglaise au prompt. Les règles (genre, âge, tenues qui se remplacent,
  incompatibilités) guident le choix ici ; le prompt est contrôlé par le filtre AS-18 avant d'être proposé.
  L'écriture par le LLM, l'aide à préciser et les ateliers Retouche, Clip et Séquence viennent avec la suite du
  portage.
</ASavoir>

<Panneau titre="Sujet et modèle">
  <div class="ligne">
    <div class="pastilles" role="group" aria-label="Sujet">
      {#each sujets as s (s.id)}
        <button type="button" class="pastille" aria-pressed={sujet === s.id} onclick={() => void choisirSujet(s.id)}>
          {s.nom}
        </button>
      {/each}
    </div>
    <label>
      <span class="etiquette">Modèle cible</span>
      <select
        value={workflowId ?? ''}
        onchange={(e) => {
          workflowId = e.currentTarget.value || null;
          planifier();
        }}
      >
        {#each workflows as w (w.id)}
          <option value={w.id}>{w.nom}</option>
        {/each}
      </select>
    </label>
  </div>
</Panneau>

<Panneau titre="Ton idée">
  <textarea
    rows="3"
    maxlength="2000"
    placeholder="Un renard roux assis dans la neige fraîche, lumière dorée du soir"
    bind:value={idee}
    oninput={garder}
  ></textarea>
  <div class="ligne">
    <label>
      <span class="etiquette">LLM</span>
      <select bind:value={llmChoisi} disabled={llms.length === 0}>
        <option value="">Celui du modèle cible</option>
        {#each llms as l (l.fichier)}
          <option value={l.fichier}>{l.specialiste ? '🎯 ' : ''}{l.nom}{l.favori ? ' ★' : ''}</option>
        {/each}
      </select>
    </label>
    <Bouton
      desactive={ecriture !== null || llms.length === 0 || (idee.trim() === '' && complements === '')}
      onclick={() => void ecrire()}
    >
      {ecriture === 'ecrire' ? 'Le LLM écrit…' : '✨ Écrire le prompt'}
    </Bouton>
  </div>
  {#if llms.length === 0}
    <p class="discret">Aucun LLM dans modeles\llm : compose le prompt avec les compléments, il part tel quel.</p>
  {:else}
    <p class="discret">
      Le LLM écrit le prompt pour le modèle cible, d'après ton idée et les compléments, qu'il garde tels quels. Il
      prend la carte graphique le temps d'écrire.
    </p>
  {/if}
</Panneau>

{#if structure && ctx}
  <Panneau titre="Compléments">
    <SousOnglets onglets={ongletsAvecCompte} bind:actif={ongletChoisi} />
    {#if onglet === 'sujet' && sujet === 'personnage'}
      <div class="ligne">
        <label>
          <span class="etiquette">Âge précis</span>
          <input
            type="number"
            min={structure.agePrecis[0]}
            max={structure.agePrecis[1]}
            placeholder="facultatif"
            value={age ?? ''}
            oninput={(e) => {
              const n = Number.parseInt(e.currentTarget.value, 10);
              age = Number.isNaN(n) ? null : n;
              planifier();
            }}
          />
        </label>
        <label class="case">
          <input type="checkbox" bind:checked={toutes} onchange={planifier} />
          Toutes les options (sans le filtre par origine)
        </label>
      </div>
    {/if}
    {#each structure.blocs.filter((b) => !b.video) as b (b.id)}
      {#each b.groupes.filter((g) => g.onglet === onglet) as g (g.nom)}
        {@const visibles = g.sousListes.filter((s) => sousListeVisible(b.id, s, ctx))}
        {#if visibles.length > 0}
          <details open={visibles.some((s) => choix[b.id]?.[s.nom])}>
            <summary>{g.nom}</summary>
            <div class="grille">
              {#each visibles as s (s.nom)}
                {@const actuelle = choix[b.id]?.[s.nom] ?? ''}
                {@const proposees = valeursProposees(b.id, s, ctx)}
                <label title={s.note}>
                  <span class="etiquette">{s.nom}</span>
                  <select value={actuelle} onchange={(e) => changer(b.id, s.nom, e.currentTarget.value)}>
                    <option value="">—</option>
                    {#if actuelle && !proposees.some((v) => v.nom === actuelle)}
                      <option value={actuelle}>{actuelle} (hors filtre)</option>
                    {/if}
                    {#each proposees as v (v.nom)}
                      <option value={v.nom} disabled={incompatible(structure, choix, b.id, s.nom, v.nom)}>
                        {v.categorie ? `${v.categorie} · ` : ''}{v.nom}
                      </option>
                    {/each}
                  </select>
                  {#if actuelle}
                    {@const d = s.valeurs.find((v) => v.nom === actuelle)?.description}
                    {#if d}<span class="discret">{d}</span>{/if}
                  {/if}
                </label>
              {/each}
            </div>
          </details>
        {/if}
      {/each}
    {/each}
    {#if onglet === 'decor'}
      <label class="texte">
        <span class="etiquette">Texte dans l'image</span>
        <input type="text" maxlength="200" bind:value={texte} oninput={planifier} placeholder="facultatif, mot pour mot" />
      </label>
      <label class="texte">
        <span class="etiquette">Précisions (traduites par le LLM, puis ajoutées au prompt)</span>
        <textarea rows="2" maxlength="500" bind:value={precisions} oninput={planifier}></textarea>
      </label>
      {#if precisions.trim() !== ''}
        <div class="ligne">
          {#if traductionValide}
            <span class="discret">→ {traduction?.en}</span>
          {:else}
            <Bouton petit desactive={ecriture !== null || llms.length === 0} onclick={() => void traduire()}>
              {ecriture === 'traduire' ? 'Traduction…' : 'Traduire et ajouter'}
            </Bouton>
          {/if}
        </div>
      {/if}
    {/if}
    <div class="actions">
      <Bouton genre="discret" petit onclick={toutEffacer}>Tout réinitialiser</Bouton>
    </div>
  </Panneau>
{:else}
  <p class="discret">Lecture…</p>
{/if}

<Panneau titre="Prompt final" accentue>
  {#if erreurAssemblage}
    <p class="alerte">{erreurAssemblage}</p>
  {/if}
  {#each assemblage?.alertes ?? [] as a (a)}
    <p class="discret">⚠ {a}</p>
  {/each}
  <textarea rows="6" value={final} oninput={(e) => retoucherFinal(e.currentTarget.value)}></textarea>
  <div class="ligne">
    {#if assemblage}
      <span class="discret">
        {final.split(/\s+/).filter((m) => m !== '').length} mots{assemblage.motsMax ? ` · ${assemblage.motsMax} au plus pour ce modèle` : ''}
      </span>
    {/if}
    {#if finalRetouche}
      <Bouton genre="discret" petit onclick={reprendreAssemblage}>Reprendre l'assemblage</Bouton>
    {/if}
  </div>
  <div class="actions">
    <Bouton
      genre="principal"
      desactive={final.trim() === '' || erreurAssemblage !== null}
      onclick={() => envoyer(final.trim(), workflowId)}
    >
      Envoyer au Plateau
    </Bouton>
  </div>
</Panneau>

<style>
  .ligne,
  .actions,
  .pastilles {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px;
    margin: 8px 0;
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

  label {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .case {
    flex-direction: row;
    align-items: center;
    gap: 6px;
    font-size: 13px;
  }

  .etiquette {
    font-size: 12px;
    color: var(--texte-2);
  }

  details {
    margin: 10px 0;
  }

  summary {
    cursor: pointer;
  }

  .grille {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
    gap: 10px;
    margin-top: 8px;
  }

  .texte {
    margin: 10px 0;
  }

  textarea {
    width: 100%;
  }

  .alerte {
    color: var(--orange);
  }
</style>
