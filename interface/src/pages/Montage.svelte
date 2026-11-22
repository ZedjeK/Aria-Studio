<!--
  Montage, comme en v2 : un clip MiniMax H3 avec le son, en animant un rush ou à partir d'un texte. Le prompt est
  tourné tel quel (il s'écrit au Scénario, ou ici). Résolution, durée, format, nombre, seed, checkpoint et LoRA de la
  famille ; estimation du temps ; file suivie en direct ; les Bobines, rangées comme les Rushes. Puis les séquences (plans
  enchaînés et assemblés en un film) et la bibliothèque des voix de référence.
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import {
    annulerCommande,
    commanderClip,
    FORMAT_IMAGE,
    lireCommandes,
    messageErreur,
    optionsMontage,
    type Commande,
    type OptionsMontage,
  } from '../lib/api';
  import { lignesPour, lorasChoisies, type LigneLora } from '../lib/loras';
  import { dureeLisible, estimationClips } from '../lib/montage';
  import { estActive, lireSeed, modeleInitial, nomCheckpoint, texteAvancement, texteCommande } from '../lib/plateau';
  import ASavoir from '../lib/composants/ASavoir.svelte';
  import Bouton from '../lib/composants/Bouton.svelte';
  import RangerDans from '../lib/composants/RangerDans.svelte';
  import ChoixLoras from '../lib/composants/ChoixLoras.svelte';
  import Panneau from '../lib/composants/Panneau.svelte';
  import ChoixRushes from './ChoixRushes.svelte';
  import MontageSequences from './MontageSequences.svelte';
  import MontageVoix from './MontageVoix.svelte';
  import PanneauRushes from './PanneauRushes.svelte';

  interface Props {
    signaler: (texte: string, erreur?: boolean) => void;
  }

  let { signaler }: Props = $props();

  let options = $state<OptionsMontage | null>(null);
  let mode = $state<'animer' | 'texte'>('animer');
  let source = $state<string[]>([]);
  let prompt = $state('');
  let qualite = $state('compromis');
  let duree = $state(5);
  let format = $state(FORMAT_IMAGE);
  let nombre = $state(1);
  let seedTexte = $state('');
  let modele = $state('');
  let lignesLoras = $state<Record<string, LigneLora>>({});
  let commandes = $state<Commande[]>([]);
  /** Change quand la file a enregistré quelque chose : la bibliothèque des voix se relit. */
  let rafraichir = $state(0);
  let envoi = $state(false);
  /** « Ranger dans » : le dossier des prochaines créations. */
  let dossier = $state<string | null>(null);

  const proposes = $derived(options ? (mode === 'animer' ? options.animer : options.texte) : null);
  const workflow = $derived(proposes?.workflows.find((w) => w.id === proposes.defaut) ?? proposes?.workflows[0]);
  const seed = $derived(lireSeed(seedTexte));
  const choisie = $derived(options?.qualites.find((q) => q.cle === qualite));
  const estimation = $derived(
    options && choisie ? dureeLisible(estimationClips(choisie, duree, nombre, options.exposantDuree)) : '',
  );
  const clips = $derived(commandes.filter((c) => c.type === 'clip' || c.type === 'voix' || c.type === 'sequence'));
  /** Le workflow qui anime un rush : celui de tous les plans des séquences. */
  const animer = $derived(
    options?.animer.workflows.find((w) => w.id === options?.animer.defaut) ?? options?.animer.workflows[0],
  );
  const peutTourner = $derived(
    !envoi &&
      workflow !== undefined &&
      prompt.trim() !== '' &&
      seed !== undefined &&
      (mode === 'texte' || source.length === 1),
  );

  function choisirMode(m: 'animer' | 'texte') {
    mode = m;
    format = m === 'animer' ? FORMAT_IMAGE : (options?.formats[1] ?? options?.formats[0] ?? '');
    preparerWorkflow();
  }

  function preparerWorkflow() {
    modele = modeleInitial(workflow);
    lignesLoras = lignesPour(workflow?.loras ?? [], lignesLoras);
  }

  async function relireOptions() {
    try {
      options = await optionsMontage();
      qualite = options.defautQualite;
      preparerWorkflow();
    } catch (e) {
      signaler(messageErreur(e), true);
    }
  }

  async function relireCommandes() {
    try {
      const avant = commandes.reduce((total, c) => total + c.images.length, 0);
      commandes = await lireCommandes();
      if (commandes.reduce((total, c) => total + c.images.length, 0) !== avant) rafraichir += 1;
    } catch (e) {
      signaler(messageErreur(e), true);
    }
  }

  async function tourner() {
    if (!workflow || seed === undefined) return;
    envoi = true;
    try {
      await commanderClip({
        idee: '',
        prompt,
        mode,
        source: mode === 'animer' ? (source[0] ?? null) : null,
        qualite,
        duree,
        format,
        seed,
        nombre,
        modele: modele || null,
        loras: lorasChoisies(workflow.loras, lignesLoras),
        ecritPar: null,
        dossier,
      });
      signaler(nombre > 1 ? `${nombre} clips en file.` : 'Clip en file.');
      await relireCommandes();
    } catch (e) {
      signaler(messageErreur(e), true);
    } finally {
      envoi = false;
    }
  }

  async function annuler(id: string) {
    try {
      await annulerCommande(id);
      await relireCommandes();
    } catch (e) {
      signaler(messageErreur(e), true);
    }
  }

  onMount(() => {
    void relireOptions();
    void relireCommandes();
    const minuterie = setInterval(() => {
      if (commandes.some(estActive)) void relireCommandes();
    }, 1000);
    return () => clearInterval(minuterie);
  });
</script>

<h1>Montage</h1>
<p class="sous-titre">Tourne des clips avec le son, en animant un rush ou à partir d'un texte.</p>
<ASavoir>
  Le prompt est tourné tel quel : écris-le au Scénario pour qu'un LLM le prépare, ou directement ici. Un clip prend la
  carte graphique plusieurs minutes ; l'estimation vient des mesures de la version 2, sur une RTX 4080. Une séquence
  enchaîne plusieurs plans et les assemble en un film ; les voix gardées lui servent de référence.
</ASavoir>

{#if options && !workflow}
  <Panneau titre="Aucun modèle vidéo prêt">
    <p>Télécharge MiniMax H3 dans Modèles, et valide les steps de son checkpoint dans Ma collection.</p>
    {#if proposes && proposes.incomplets.length > 0}
      <p class="discret">Incomplets : {proposes.incomplets.join(', ')}</p>
    {/if}
  </Panneau>
{:else if options && workflow}
  <Panneau titre="Tourner un clip" accentue>
    <div class="pastilles" role="group" aria-label="Départ">
      <button type="button" class="pastille" aria-pressed={mode === 'animer'} onclick={() => choisirMode('animer')}>
        Animer un rush
      </button>
      <button type="button" class="pastille" aria-pressed={mode === 'texte'} onclick={() => choisirMode('texte')}>
        À partir d'un texte
      </button>
    </div>
    {#if mode === 'animer'}
      <ChoixRushes max={1} bind:selection={source} {signaler} />
    {/if}
    <label class="prompt">
      <span class="etiquette">Prompt (tourné tel quel)</span>
      <textarea
        rows="4"
        bind:value={prompt}
        placeholder="The fox lifts its head, snow falls softly, distant wind. Camera slowly pushes in."
      ></textarea>
    </label>
    <div class="grille">
      <label>
        <span class="etiquette">Résolution</span>
        <select bind:value={qualite}>
          {#each options.qualites as q (q.cle)}
            <option value={q.cle} title={q.aide}>{q.nom} · {q.aide}</option>
          {/each}
        </select>
      </label>
      <label>
        <span class="etiquette">Durée</span>
        <select bind:value={duree}>
          {#each options.durees as d (d)}
            <option value={d}>{d} s</option>
          {/each}
        </select>
      </label>
      <label>
        <span class="etiquette">Format</span>
        <select bind:value={format}>
          {#if mode === 'animer'}
            <option value={FORMAT_IMAGE}>Celui de l'image</option>
          {/if}
          {#each options.formats as f (f)}
            <option value={f}>{f}</option>
          {/each}
        </select>
      </label>
      <label>
        <span class="etiquette">Checkpoint</span>
        <select bind:value={modele} disabled={workflow.modeles.length === 0}>
          {#each workflow.modeles as fichier (fichier)}
            <option value={fichier}>{nomCheckpoint(fichier)}</option>
          {/each}
        </select>
      </label>
      <label>
        <span class="etiquette">Clips</span>
        <input type="number" min="1" max={options.nombreMax} bind:value={nombre} />
      </label>
      <label>
        <span class="etiquette">Seed</span>
        <input type="text" inputmode="numeric" placeholder="au hasard" bind:value={seedTexte} />
      </label>
    </div>
    {#if workflow.modeles.length === 0}
      <p class="alerte">Les steps du checkpoint attendent d'être validés : fais-le dans Modèles → Ma collection.</p>
    {/if}
    <ChoixLoras
      famille={workflow.famille}
      proposees={workflow.loras}
      forceMax={proposes?.loraForceMax ?? 2}
      bind:lignes={lignesLoras}
    />
    {#if seed === undefined}
      <p class="alerte">La seed est un entier positif, ou rien pour la tirer au hasard.</p>
    {/if}
    <div class="actions">
      <RangerDans genre="clips" bind:dossier {rafraichir} />
      <span class="discret">≈ {estimation} sur la carte de la mesure</span>
      <Bouton genre="principal" desactive={!peutTourner} onclick={() => void tourner()}>Tourner</Bouton>
    </div>
  </Panneau>
{:else}
  <p class="discret">Lecture…</p>
{/if}

{#if clips.length > 0}
  <Panneau titre="File">
    <ul class="file">
      {#each clips as c (c.id)}
        <li class:erreur={c.etat === 'erreur'}>
          <div class="texte">
            <span class="prompt-court">{texteCommande(c)}</span>
            <span class="discret">{texteAvancement(c)}</span>
            {#if c.etat === 'en_cours'}
              <progress max="1" value={c.progression}></progress>
            {/if}
          </div>
          {#if estActive(c) && !c.annulee}
            <Bouton genre="discret" petit onclick={() => void annuler(c.id)}>✕ Annuler</Bouton>
          {/if}
        </li>
      {/each}
    </ul>
  </Panneau>
{/if}

{#if options && animer}
  <MontageSequences {signaler} {options} workflow={animer} enFile={() => void relireCommandes()} {rafraichir} />
{/if}

<MontageVoix {signaler} enFile={() => void relireCommandes()} {rafraichir} />

<PanneauRushes genre="clips" {signaler} {rafraichir} />

<style>
  .pastilles,
  .actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px;
    margin: 8px 0;
  }

  .actions {
    justify-content: flex-end;
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

  .grille {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
    gap: 10px;
    margin: 10px 0;
  }

  label {
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

  .file {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .file li {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .texte {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  .prompt-court {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  progress {
    width: 100%;
    height: 4px;
    accent-color: var(--cyan);
  }

  .erreur,
  .alerte {
    color: var(--orange);
  }
</style>
