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
    ecrirePromptScenario,
    estAnnulation,
    estimerTournage,
    FORMAT_IMAGE,
    lireCommandes,
    messageErreur,
    optionsMontage,
    type ActionClip,
    type Commande,
    type Creation,
    type EstimationTournage,
    type OptionsMontage,
    type EnvoiClip,
    type Sequence,
  } from '../lib/api';
  import { lignesPour, lorasChoisies, type LigneLora } from '../lib/loras';
  import { dureeLisible, estimationClips, texteEstimation } from '../lib/montage';
  import {
    estActive,
    lireSeed,
    llmInitial,
    modeleInitial,
    nomCheckpoint,
    texteAvancement,
    texteCommande,
  } from '../lib/plateau';
  import AnnulerEcriture from '../lib/composants/AnnulerEcriture.svelte';
  import ASavoir from '../lib/composants/ASavoir.svelte';
  import Bouton from '../lib/composants/Bouton.svelte';
  import CarteChoix from '../lib/composants/CarteChoix.svelte';
  import ExpliquerErreur from '../lib/composants/ExpliquerErreur.svelte';
  import RangerDans from '../lib/composants/RangerDans.svelte';
  import ChoixLoras from '../lib/composants/ChoixLoras.svelte';
  import Panneau from '../lib/composants/Panneau.svelte';
  import SousOnglets from '../lib/composants/SousOnglets.svelte';
  import VignetteCreation from '../lib/composants/VignetteCreation.svelte';
  import DialogueRush from './DialogueRush.svelte';
  import MontageSequences from './MontageSequences.svelte';
  import MontageVoix from './MontageVoix.svelte';
  import PanneauRushes from './PanneauRushes.svelte';

  interface Props {
    signaler: (texte: string, erreur?: boolean) => void;
    /** Une séquence écrite au Scénario, à ouvrir ici. */
    recu: string | null;
    recuLu: () => void;
    /** Une séquence repart au Scénario pour être retravaillée. */
    retravailler: (s: Sequence) => void;
    /** Le prompt d'un clip écrit au Scénario, à tourner ici. */
    recuClip?: EnvoiClip | null;
    recuClipLu?: () => void;
    /** Ouvre le Scénario (le bouton n'apparaît que si la page peut y aller). */
    ouvrirScenario?: () => void;
  }

  let { signaler, recu, recuLu, retravailler, recuClip = null, recuClipLu = () => {}, ouvrirScenario }: Props = $props();

  const ONGLETS = [
    { id: 'tourner', titre: '🎬 Tourner' },
    { id: 'sequences', titre: '📽️ Séquences' },
    { id: 'voix', titre: '🎙️ Voix' },
    { id: 'bobines', titre: '🎞️ Bobines' },
  ];
  /** Les cartes du départ : animer un rush, une séquence (elle se tourne dans son onglet) ou un texte. */
  type Depart = 'animer' | 'sequence' | 'texte';

  let onglet = $state('tourner');
  let depart = $state<Depart>('animer');
  /** Le dialogue « Quel rush choisir ? » est ouvert. */
  let choixRush = $state(false);
  /** « Écrire avec le LLM » : le prompt d'avant, que « Retrouver mon texte » remet ; le LLM écrit en ce moment. */
  let texteOrigine = $state<string | null>(null);
  let ecriture = $state(false);
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
  /** La séquence à ouvrir, demandée depuis un plan ou un film des Bobines. */
  let sequenceAOuvrir = $state<string | null>(null);
  let formulaire = $state<HTMLElement | null>(null);
  let envoi = $state(false);
  /** « Ranger dans » : le dossier des prochaines créations. */
  let dossier = $state<string | null>(null);

  const proposes = $derived(options ? (mode === 'animer' ? options.animer : options.texte) : null);
  const workflow = $derived(proposes?.workflows.find((w) => w.id === proposes.defaut) ?? proposes?.workflows[0]);
  const seed = $derived(lireSeed(seedTexte));
  const choisie = $derived(options?.qualites.find((q) => q.cle === qualite));
  /** Le calcul du cœur (VRAM libre, steps et poids du checkpoint), pour les réglages qui l'ont demandé. */
  let calcul = $state<{ cle: string; resultat: EstimationTournage } | null>(null);
  let derniereDemande = 0;
  const cleCalcul = $derived([mode, modele, qualite, duree, nombre].join('|'));
  const estimation = $derived(
    options && choisie
      ? texteEstimation(
          calcul?.cle === cleCalcul ? calcul.resultat : null,
          estimationClips(choisie, duree, nombre, options.exposantDuree),
          choisie.nom,
          duree,
        )
      : '',
  );

  // Les réglages changent : le cœur refait le calcul ; seule la dernière demande compte (le tableau en attendant).
  $effect(() => {
    const cle = cleCalcul;
    if (!options) return;
    const rang = ++derniereDemande;
    estimerTournage(mode, qualite, duree, nombre, modele || null)
      .then((resultat) => {
        if (rang === derniereDemande) calcul = { cle, resultat };
      })
      .catch(() => {
        if (rang === derniereDemande) calcul = null;
      });
  });
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

  function choisirDepart(d: Depart) {
    depart = d;
    if (d !== 'sequence') choisirMode(d);
  }

  function choisirMode(m: 'animer' | 'texte') {
    mode = m;
    depart = m;
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

  /** Le format d'un clip pour le retourner : celui du texte, ou le recadrage de son rush (sinon celui de l'image). */
  function formatDe(clip: Creation, m: 'animer' | 'texte'): string {
    if (m === 'texte') return clip.format ?? options?.formats[0] ?? '';
    return clip.cadrage ?? FORMAT_IMAGE;
  }

  /** Bobines : Variante (le même clip, une autre seed), Réutiliser le prompt (ses réglages ici), Ouvrir la séquence. */
  async function agirClip(clip: Creation, action: ActionClip) {
    if (action === 'sequence') {
      sequenceAOuvrir = clip.sequence ?? null;
      onglet = 'sequences';
      return;
    }
    const m = clip.mode_video === 'texte' ? 'texte' : 'animer';
    if (action === 'variante') {
      try {
        await commanderClip({
          idee: clip.idee ?? '',
          prompt: clip.prompt ?? '',
          mode: m,
          source: m === 'animer' ? (clip.parent ?? null) : null,
          qualite: clip.qualite ?? qualite,
          duree: clip.duree ?? duree,
          format: formatDe(clip, m),
          seed: null,
          nombre: 1,
          modele: clip.modele ?? null,
          loras: clip.loras ?? [],
          ecritPar: clip.llm ?? null,
          dossier: clip.dossier ?? null,
        });
        signaler('Variante en file.');
        await relireCommandes();
      } catch (e) {
        signaler(messageErreur(e), true);
      }
      return;
    }
    onglet = 'tourner';
    choisirMode(m);
    prompt = clip.prompt ?? '';
    if (m === 'animer' && clip.parent) source = [clip.parent];
    if (clip.qualite) qualite = clip.qualite;
    if (clip.duree) duree = clip.duree;
    format = formatDe(clip, m);
    if (clip.modele && workflow?.modeles.includes(clip.modele)) modele = clip.modele;
    const avant: Record<string, LigneLora> = {};
    for (const l of clip.loras ?? []) avant[l.fichier] = { actif: true, force: l.force, declencheurs: l.declencheurs };
    lignesLoras = lignesPour(workflow?.loras ?? [], avant);
    formulaire?.scrollIntoView({ behavior: 'smooth', block: 'start' });
  }

  /** ✨ Écrire avec le LLM : le prompt du clip s'écrit ici, dans le champ ; il sera tourné tel quel. */
  async function ecrire() {
    if (!workflow || !proposes || ecriture || prompt.trim() === '') return;
    ecriture = true;
    try {
      const ecrit = await ecrirePromptScenario({
        atelier: 'clip',
        idee: prompt,
        complements: '',
        workflow: workflow.id,
        llm: llmInitial(proposes, workflow.id) || null,
        images: mode === 'animer' ? source : [],
        role: mode === 'animer' ? 'animer' : null,
        duree,
      });
      texteOrigine = prompt;
      prompt = ecrit.prompt;
      const coupes = ecrit.alertes.length > 0 ? ` · ⚠ ${ecrit.alertes.join(' ')}` : '';
      signaler(`Prompt écrit par ${ecrit.llm} (${ecrit.mots} mots, ${ecrit.dureeS} s) : il sera tourné tel quel.${coupes}`, ecrit.alertes.length > 0);
    } catch (e) {
      signaler(estAnnulation(e) ? 'Écriture annulée.' : messageErreur(e), !estAnnulation(e));
    } finally {
      ecriture = false;
    }
  }

  /** ↩ Retrouver mon texte : le prompt d'avant l'écriture revient. */
  function retrouver() {
    if (texteOrigine === null) return;
    prompt = texteOrigine;
    texteOrigine = null;
    signaler("Ton texte d'origine est revenu.");
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
    if (recu) {
      sequenceAOuvrir = recu;
      onglet = 'sequences';
      recuLu();
    }
    void (async () => {
      await relireOptions();
      // Un clip écrit au Scénario : son prompt, son mode, son rush et sa durée.
      if (recuClip) {
        const envoi = recuClip;
        recuClipLu();
        choisirMode(envoi.mode);
        prompt = envoi.prompt;
        source = envoi.source ? [envoi.source] : [];
        if (options?.durees.includes(envoi.duree)) duree = envoi.duree;
        signaler('Prompt posé : vérifie les réglages et tourne.');
      }
    })();
    void relireCommandes();
    const minuterie = setInterval(() => {
      if (commandes.some(estActive)) void relireCommandes();
    }, 1000);
    return () => clearInterval(minuterie);
  });
</script>

<h1>Montage</h1>
<p class="sous-titre">Tourne des clips avec le son, assemble des séquences, garde tes voix.</p>
<SousOnglets onglets={ONGLETS} bind:actif={onglet} />

{#if onglet === 'tourner'}
  <ASavoir>
    Le prompt est tourné tel quel : écris-le au Scénario pour qu'un LLM le prépare, ou ici avec ✨. Un seul modèle
    vidéo tourne à la fois ; l'estimation part des mesures de la version 2 (une RTX 4080) et tient compte de ta VRAM
    libre et du checkpoint.
  </ASavoir>
{:else if onglet === 'sequences'}
  <ASavoir>
    Une séquence enchaîne plusieurs plans et les assemble en un film. Chaque plan part du précédent, d'une image ou du
    texte ; les voix gardées lui servent de référence.
  </ASavoir>
{:else if onglet === 'voix'}
  <ASavoir>
    Un essai de voix se tourne, s'écoute, et se garde dans la bibliothèque : les voix gardées servent de référence aux
    séquences.
  </ASavoir>
{:else}
  <ASavoir>Les Bobines rangent tes clips comme les Rushes rangent tes images.</ASavoir>
{/if}

{#if onglet === 'tourner'}
  {#if options && !workflow}
    <Panneau titre="Aucun modèle vidéo prêt">
      <p>Télécharge MiniMax H3 dans Modèles, et valide les steps de son checkpoint dans Ma collection.</p>
      {#if proposes && proposes.incomplets.length > 0}
        <p class="discret">Incomplets : {proposes.incomplets.join(', ')}</p>
      {/if}
    </Panneau>
  {:else if options && workflow}
    <div bind:this={formulaire}></div>
    <div class="deux-groupes" role="radiogroup" aria-label="Départ du clip">
      <div class="groupe">
        <p class="groupe-titre">Avec des images</p>
        <div class="cartes">
          <CarteChoix groupe="depart" valeur="animer" titre="🎬 Animer un rush" bind:choisie={depart} onchange={() => choisirDepart('animer')}>
            Une image de ton Plateau devient un clip.
          </CarteChoix>
          <CarteChoix groupe="depart" valeur="sequence" titre="📽️ Séquence" bind:choisie={depart} onchange={() => choisirDepart('sequence')}>
            Plusieurs plans enchaînés, découpés pour toi.
          </CarteChoix>
        </div>
      </div>
      <div class="groupe">
        <p class="groupe-titre">Sans image</p>
        <div class="cartes">
          <CarteChoix groupe="depart" valeur="texte" titre="✍️ Texte → vidéo" bind:choisie={depart} onchange={() => choisirDepart('texte')}>
            Tu décris, le modèle tourne tout.
          </CarteChoix>
        </div>
      </div>
    </div>

    {#if depart === 'sequence'}
      <div class="alerte-info">
        <strong>Séquence</strong>
        <span>Les séquences se préparent et se tournent dans l'onglet Séquences.</span>
        <Bouton petit onclick={() => (onglet = 'sequences')}>Ouvrir</Bouton>
      </div>
    {:else}
      <Panneau titre="Tourner un clip" accentue>
        {#if mode === 'animer'}
          <p class="etiquette titre-champ">Image de départ</p>
          <div class="cases">
            {#each source as id (id)}
              <div class="case">
                <VignetteCreation {id} titre="Image de départ" />
                <button type="button" class="retire" aria-label="Retirer l'image de départ" onclick={() => (source = [])}>✕</button>
              </div>
            {/each}
            <Bouton petit onclick={() => (choixRush = true)}>{source.length > 0 ? 'Changer de rush' : 'Choisir un rush'}</Bouton>
          </div>
        {/if}
        <label class="prompt">
          <span class="etiquette">Prompt (tourné tel quel)</span>
          <textarea
            rows="4"
            bind:value={prompt}
            placeholder="The fox lifts its head, snow falls softly, distant wind. Camera slowly pushes in."
          ></textarea>
        </label>
        <div class="actions-gauche">
          {#if ecriture}
            <span class="discret">Le LLM écrit…</span>
            <AnnulerEcriture />
          {:else}
            <Bouton petit desactive={!proposes || proposes.llm.length === 0 || prompt.trim() === ''} onclick={() => void ecrire()}>
              ✨ Écrire avec le LLM
            </Bouton>
            <Bouton genre="discret" petit desactive={texteOrigine === null} onclick={retrouver}>↩ Retrouver mon texte</Bouton>
            {#if ouvrirScenario}
              <Bouton genre="discret" petit onclick={ouvrirScenario}>↗ Ouvrir le Scénario</Bouton>
            {/if}
          {/if}
        </div>

        <div class="grille">
          <label>
            <span class="etiquette">Checkpoint</span>
            <select bind:value={modele} disabled={workflow.modeles.length === 0}>
              {#each workflow.modeles as fichier (fichier)}
                <option value={fichier}>{nomCheckpoint(fichier)}</option>
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
            <span class="etiquette">Seed</span>
            <input type="text" inputmode="numeric" placeholder="au hasard" bind:value={seedTexte} class:invalide={seed === undefined} />
          </label>
        </div>

        <p class="etiquette titre-champ">Résolution</p>
        <div class="cartes" role="radiogroup" aria-label="Résolution">
          {#each options.qualites as q (q.cle)}
            <CarteChoix groupe="qualite" valeur={q.cle} titre={q.nom} bind:choisie={qualite}>{q.aide}</CarteChoix>
          {/each}
        </div>

        <p class="etiquette titre-champ">Durée</p>
        <div class="cartes" role="radiogroup" aria-label="Durée du clip">
          {#each options.durees as d (d)}
            <CarteChoix groupe="duree" valeur={String(d)} titre={`${d} s`} bind:choisie={() => String(duree), (v) => (duree = Number(v))}>
              {choisie ? `≈ ${dureeLisible(estimationClips(choisie, d, 1, options.exposantDuree))} par clip` : ''}
            </CarteChoix>
          {/each}
        </div>

        <div class="grille bas">
          <label>
            <span class="etiquette">Clips</span>
            <input type="number" min="1" max={options.nombreMax} bind:value={nombre} />
          </label>
          <RangerDans genre="clips" bind:dossier {rafraichir} />
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
        <div class="estimation">
          <strong>Estimation</strong>
          <span class="code">{estimation}</span>
        </div>
        <div class="actions">
          <Bouton genre="principal" desactive={!peutTourner} onclick={() => void tourner()}>Tourner</Bouton>
        </div>
      </Panneau>
    {/if}
  {:else}
    <p class="discret">Lecture…</p>
  {/if}
{/if}

<div hidden={onglet !== 'sequences'}>
  {#if options && animer}
    <MontageSequences
      {signaler}
      {options}
      workflow={animer}
      enFile={() => void relireCommandes()}
      {rafraichir}
      bind:ouvrir={sequenceAOuvrir}
      {retravailler}
    />
  {/if}
</div>

<div hidden={onglet !== 'voix'}>
  <MontageVoix {signaler} enFile={() => void relireCommandes()} {rafraichir} />
</div>

{#if onglet === 'bobines'}
  <PanneauRushes
    genre="clips"
    {signaler}
    {rafraichir}
    agirClip={(clip, action) => void agirClip(clip, action)}
    enFile={() => void relireCommandes()}
  />
{/if}

{#if clips.length > 0}
  <Panneau titre="File">
    <ul class="file">
      {#each clips as c (c.id)}
        <li class:erreur={c.etat === 'erreur'}>
          <div class="texte">
            <span class="prompt-court">{texteCommande(c)}</span>
            <span class="discret">{texteAvancement(c)}</span>
            {#if c.etat === 'erreur' && c.erreur}
              <ExpliquerErreur erreur={c.erreur} origine={'montage'} commande={c.id} contexte={c.etape} />
            {/if}
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

{#if choixRush}
  <DialogueRush
    max={1}
    bind:selection={source}
    {signaler}
    fermer={() => (choixRush = false)}
    apresChoix={() => (choixRush = false)}
    apresImport={() => (rafraichir += 1)}
  />
{/if}

<style>
  .deux-groupes {
    display: grid;
    grid-template-columns: 2fr 1fr;
    gap: 18px;
  }

  .groupe + .groupe {
    padding-left: 18px;
    border-left: 1px dashed var(--ligne);
  }

  .groupe {
    display: flex;
    flex-direction: column;
  }

  .groupe-titre {
    display: flex;
    align-items: center;
    gap: 10px;
    margin: 0 0 10px;
    color: var(--bleu);
    font: 700 11px/1 var(--police);
    letter-spacing: 0.18em;
    text-transform: uppercase;
  }

  .groupe-titre::after {
    content: '';
    flex: 1;
    height: 1px;
    background: var(--ligne);
  }

  .cartes {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(160px, 1fr));
    gap: 12px;
    align-items: stretch;
  }

  .alerte-info {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 12px;
    margin: 16px 0;
    padding: 8px 14px;
    border: 1px solid var(--bleu);
    background: color-mix(in srgb, var(--bleu) 9%, var(--panneau));
  }

  .alerte-info strong {
    color: var(--bleu);
    font-size: 11px;
    letter-spacing: 0.12em;
    text-transform: uppercase;
  }

  .alerte-info span {
    flex: 1;
  }

  .titre-champ {
    margin: 14px 0 8px;
  }

  .cases {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-end;
    gap: 12px;
  }

  .case {
    position: relative;
    width: 96px;
    height: 96px;
  }

  .retire {
    position: absolute;
    top: 2px;
    right: 2px;
    width: 22px;
    height: 22px;
    padding: 0;
    font: inherit;
    font-size: 12px;
    color: var(--texte);
    background: color-mix(in srgb, var(--fond) 86%, transparent);
    border: 1px solid var(--ligne);
    cursor: pointer;
  }

  .actions-gauche,
  .actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px;
    margin: 10px 0 14px;
  }

  .actions {
    justify-content: flex-end;
  }

  .grille {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
    gap: 10px 16px;
  }

  .grille.bas {
    grid-template-columns: repeat(auto-fit, minmax(180px, 220px));
    margin-top: 14px;
  }

  label {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .etiquette {
    color: var(--texte-2);
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.14em;
    text-transform: uppercase;
  }

  .prompt {
    margin-top: 4px;
  }

  textarea {
    width: 100%;
    font: inherit;
    color: var(--texte);
    background: var(--fond);
    border: 1px solid var(--ligne);
    padding: 8px;
    resize: vertical;
  }

  input[type='text'] {
    font-family: var(--police-code);
  }

  .invalide {
    border-color: var(--rouge);
  }

  .estimation {
    display: flex;
    flex-wrap: wrap;
    gap: 6px 16px;
    align-items: baseline;
    margin-top: 14px;
    padding: 10px 16px;
    border: 1px solid var(--ligne);
    background: var(--fond);
  }

  .estimation strong {
    flex: none;
    color: var(--texte-2);
    font-size: 12px;
    letter-spacing: 0.08em;
    text-transform: uppercase;
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

  @media (max-width: 900px) {
    .deux-groupes {
      grid-template-columns: 1fr;
    }

    .groupe + .groupe {
      padding-left: 0;
      border-left: 0;
    }
  }
</style>
