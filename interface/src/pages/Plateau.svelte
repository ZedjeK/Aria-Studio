<!--
  Le Plateau : créer une image depuis une idée réécrite par le LLM, ou depuis un prompt écrit tel quel ; suivre la
  file, voir les dernières créations. Le LLM qui voit peut vérifier les images à la fin, puis les comparer pour
  retenir les meilleures (nouveauté de la v3). Le bouton orange (Créer) agit sur le panneau accentué.
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import {
    annulerCommande,
    choisirMeilleures,
    commanderImage,
    ecrirePromptScenario,
    estAnnulation,
    etatVision,
    lireCommandes,
    messageErreur,
    optionsPlateau,
    optionsRetouche,
    FORMAT_IMAGE,
    IMAGES_MAX_VISION,
    validerCheckpoint,
    verifierImages,
    type Commande,
    type EtatVision,
    type EnvoiPlateau,
    type ModeReference,
    type OptionsPlateau,
  } from '../lib/api';
  import {
    estActive,
    formatInitial,
    lireSeed,
    llmInitial,
    LIBERTE,
    rushesMax,
    workflowsPour,
    modeleInitial,
    nomCheckpoint,
    texteAvancement,
    texteCommande,
    texteStepsSuggeres,
  } from '../lib/plateau';
  import ASavoir from '../lib/composants/ASavoir.svelte';
  import Bouton from '../lib/composants/Bouton.svelte';
  import CarteChoix from '../lib/composants/CarteChoix.svelte';
  import Dialogue from '../lib/composants/Dialogue.svelte';
  import ExpliquerErreur from '../lib/composants/ExpliquerErreur.svelte';
  import RangerDans from '../lib/composants/RangerDans.svelte';
  import ChoixLoras from '../lib/composants/ChoixLoras.svelte';
  import { lignesPour, lorasChoisies, type LigneLora } from '../lib/loras';
  import Panneau from '../lib/composants/Panneau.svelte';
  import VignetteCreation from '../lib/composants/VignetteCreation.svelte';
  import ChoixRushes from './ChoixRushes.svelte';
  import PanneauRushes from './PanneauRushes.svelte';
  import AnnulerEcriture from '../lib/composants/AnnulerEcriture.svelte';
  import { peutChoisir, texteChoix, texteVerifications } from '../lib/vision';

  /** La carte « Références » choisie : aucune, mélanger des images (inspirer) ou redessiner une image (variation). */
  type Depart = 'aucune' | ModeReference;

  interface Props {
    signaler: (texte: string, erreur?: boolean) => void;
    /** Un prompt écrit au Scénario : il part tel quel, avec son modèle cible et ses images de départ. */
    recu?: EnvoiPlateau | null;
    /** Le prompt reçu est posé. */
    recuLu?: () => void;
  }

  let { signaler, recu = null, recuLu }: Props = $props();

  let options = $state<OptionsPlateau | null>(null);
  let commandes = $state<Commande[]>([]);
  // Change quand de nouvelles images sont enregistrées : les Rushes se relisent.
  let rafraichir = $state(0);

  let workflowId = $state('');
  let prompt = $state('');
  let modele = $state('');
  let format = $state('');
  let nombre = $state(1);
  let seedTexte = $state('');
  let reecrire = $state(true);
  let llmChoisi = $state('');
  let premiereLecture = true;
  // Partir de rushes : s'en inspirer (workflows de retouche à références), ou en redessiner un.
  let modeReference = $state<ModeReference | null>(null);
  let carte = $state<Depart>('aucune');
  let references = $state<string[]>([]);
  /** Le dialogue « Quel rush choisir ? » est ouvert. */
  let choixRush = $state(false);
  /** « Écrire avec le LLM » : l'idée d'avant, que « Retrouver mon texte » remet ; le LLM écrit en ce moment. */
  let texteOrigine = $state<string | null>(null);
  let ecriture = $state(false);
  let liberte = $state(LIBERTE.defaut);
  let optionsInspirer = $state<OptionsPlateau | null>(null);
  let stepsSaisis = $state<Record<string, number>>({});
  let envoi = $state(false);
  /** « Ranger dans » : le dossier des prochaines créations. */
  let dossier = $state<string | null>(null);
  /** « Vérifier les images à la fin » : le LLM qui voit compare chaque image à l'intention. */
  let verifierALaFin = $state(false);
  let vision = $state<EtatVision | null>(null);
  /** La commande dont les images sont en cours de vérification ou de comparaison. */
  let lotEnCours = $state<string | null>(null);

  const proposes = $derived(options ? workflowsPour(modeReference, options, optionsInspirer) : []);
  const workflow = $derived(proposes.find((w) => w.id === workflowId));
  /** Les LoRA de la famille du workflow : cochées, force, mots déclencheurs. */
  let lignesLoras = $state<Record<string, LigneLora>>({});
  const maxRushes = $derived(rushesMax(modeReference, workflow));
  const seed = $derived(lireSeed(seedTexte));
  const nombreValide = $derived(Number.isInteger(nombre) && nombre >= 1 && nombre <= (options?.nombreMax ?? 1));
  const peutCreer = $derived(
    !envoi &&
      workflow !== undefined &&
      modele !== '' &&
      prompt.trim() !== '' &&
      seed !== undefined &&
      nombreValide &&
      (!reecrire || llmChoisi !== '') &&
      (modeReference === null || (references.length >= 1 && references.length <= maxRushes)),
  );

  function choisirWorkflow(id: string) {
    workflowId = id;
    const choisi = proposes.find((w) => w.id === id);
    modele = modeleInitial(choisi);
    lignesLoras = lignesPour(choisi?.loras ?? [], lignesLoras);
    if (options) {
      format = modeReference === 'variation' ? FORMAT_IMAGE : formatInitial(options, choisi);
      llmChoisi = llmInitial(modeReference === 'inspirer' && optionsInspirer ? optionsInspirer : options, id);
    }
    for (const checkpoint of choisi?.aValider ?? []) {
      stepsSaisis[checkpoint] ??= choisi?.stepsSuggeres[0] ?? 20;
    }
  }

  async function relireOptions() {
    try {
      options = await optionsPlateau();
      // Sans LLM installé, la case part décochée : le prompt est alors envoyé tel quel.
      if (premiereLecture) reecrire = options.llm.length > 0;
      premiereLecture = false;
      optionsInspirer = await optionsRetouche();
      const liste = workflowsPour(modeReference, options, optionsInspirer);
      const garde = liste.some((w) => w.id === workflowId);
      const voulu = modeReference === 'inspirer' ? options.defautReferences : options.defaut;
      const defaut = liste.find((w) => w.id === voulu) ?? liste[0];
      choisirWorkflow(garde ? workflowId : (defaut?.id ?? ''));
      // Un prompt du Scénario : il part tel quel, avec son modèle cible s'il est prêt.
      if (recu) {
        prompt = recu.prompt;
        reecrire = false;
        // S'inspirer ou redessiner : le départ, puis le modèle cible parmi ceux de ce départ, puis les images.
        const depart = recu.modeReference;
        if (depart !== modeReference) changerDepart(depart);
        const cible = recu.workflow;
        if (cible && workflowsPour(depart, options, optionsInspirer).some((w) => w.id === cible)) choisirWorkflow(cible);
        if (depart) references = [...recu.references];
        recuLu?.();
        signaler('Prompt du Scénario reçu : il partira tel quel.');
      }
    } catch (e) {
      signaler(messageErreur(e), true);
    }
  }

  function changerDepart(mode: ModeReference | null) {
    modeReference = mode;
    carte = mode ?? 'aucune';
    if (!options) return;
    const liste = workflowsPour(mode, options, optionsInspirer);
    const voulu = mode === 'inspirer' ? options.defautReferences : options.defaut;
    choisirWorkflow((liste.find((w) => w.id === voulu) ?? liste[0])?.id ?? '');
  }

  /** ✨ Écrire avec le LLM : le prompt s'écrit ici, dans le champ, avant la création ; il partira tel quel. */
  async function ecrire() {
    if (!workflow || ecriture || prompt.trim() === '') return;
    ecriture = true;
    try {
      const ecrit = await ecrirePromptScenario({
        atelier: 'image',
        idee: prompt,
        complements: '',
        workflow: workflow.id,
        llm: llmChoisi || null,
        images: modeReference ? references : [],
        role: modeReference,
      });
      texteOrigine = prompt;
      prompt = ecrit.prompt;
      reecrire = false;
      signaler(`Prompt écrit par ${ecrit.llm} pour ${workflow.nom} (${ecrit.mots} mots, ${ecrit.dureeS} s) : il partira tel quel.`);
    } catch (e) {
      signaler(estAnnulation(e) ? 'Écriture annulée.' : messageErreur(e), !estAnnulation(e));
    } finally {
      ecriture = false;
    }
  }

  /** ↩ Retrouver mon texte : l'idée d'avant l'écriture revient, et le LLM la réécrira à la création. */
  function retrouver() {
    if (texteOrigine === null) return;
    prompt = texteOrigine;
    texteOrigine = null;
    reecrire = options !== null && options.llm.length > 0;
    signaler('Ton texte d\'origine est revenu.');
  }

  function retirerReference(id: string) {
    references = references.filter((r) => r !== id);
  }

  async function relireCommandes() {
    try {
      const avant = commandes.reduce((total, c) => total + c.images.length, 0);
      commandes = await lireCommandes();
      const apres = commandes.reduce((total, c) => total + c.images.length, 0);
      if (apres !== avant) rafraichir += 1;
    } catch (e) {
      signaler(messageErreur(e), true);
    }
  }

  async function creer() {
    if (!workflow || seed === undefined) return;
    envoi = true;
    try {
      await commanderImage({
        prompt,
        reecrire,
        llm: reecrire ? llmChoisi || null : null,
        workflow: workflow.id,
        format,
        seed,
        nombre,
        modele: modele || null,
        references: modeReference ? references : [],
        modeReference,
        liberte,
        loras: lorasChoisies(workflow.loras, lignesLoras),
        dossier,
        verifier: verifierALaFin && Boolean(vision?.llm),
      });
      signaler(nombre > 1 ? `${nombre} images en file.` : 'Image en file.');
      await relireCommandes();
    } catch (e) {
      signaler(messageErreur(e), true);
    } finally {
      envoi = false;
    }
  }

  async function verifierLot(c: Commande) {
    lotEnCours = c.id;
    try {
      signaler(texteVerifications(await verifierImages(c.images)));
      rafraichir += 1;
    } catch (e) {
      signaler(messageErreur(e), true);
    } finally {
      lotEnCours = null;
    }
  }

  async function choisirLot(c: Commande) {
    lotEnCours = c.id;
    try {
      signaler(texteChoix(await choisirMeilleures(c.images.slice(0, IMAGES_MAX_VISION))));
      rafraichir += 1;
    } catch (e) {
      signaler(messageErreur(e), true);
    } finally {
      lotEnCours = null;
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

  async function valider(checkpoint: string) {
    if (!workflow) return;
    const steps = stepsSaisis[checkpoint] ?? 0;
    try {
      await validerCheckpoint(workflow.id, checkpoint, steps);
      signaler(`${nomCheckpoint(checkpoint)} : ${steps} steps validés.`);
      await relireOptions();
    } catch (e) {
      signaler(messageErreur(e), true);
    }
  }

  async function relireVision() {
    try {
      vision = await etatVision();
    } catch {
      vision = null;
    }
  }

  onMount(() => {
    void relireOptions();
    void relireCommandes();
    void relireVision();
    const minuterie = setInterval(() => {
      if (commandes.some(estActive)) void relireCommandes();
    }, 1000);
    // Un modèle déposé à la main pendant que la page est ouverte : relu au retour dans la fenêtre.
    const auRetour = () => void relireOptions();
    window.addEventListener('focus', auRetour);
    return () => {
      clearInterval(minuterie);
      window.removeEventListener('focus', auRetour);
    };
  });
</script>

<h1>Plateau</h1>
<p class="sous-titre">Créer une image à partir d'une idée.</p>
<ASavoir>
  Écris ton idée en français : le LLM la réécrit en prompt pour le modèle choisi, puis l'image est générée. Décoche la
  réécriture pour envoyer ton texte tel quel (en anglais de préférence). L'idée et le prompt passent par le filtre
  AS-18. Les images sont rangées dans data\creations, avec leur fiche, comme en version 2.
</ASavoir>

{#if options === null}
  <p class="discret">Lecture…</p>
{:else if options.workflows.length === 0}
  <Panneau titre="Aucun modèle d'image prêt">
    <p>
      Il manque des fichiers à {options.incomplets.join(', ') || 'tous les workflows'}. Le téléchargement des modèles
      arrive dans une prochaine version : en attendant, ceux déjà téléchargés par la version 2 sont repris, et un
      fichier déposé à la main dans le dossier des modèles compte dès que tu reviens sur cette page.
    </p>
  </Panneau>
{:else}
  <Panneau titre="Créer une image" accentue>
    <div class="ligne reecriture">
      <label>
        <input type="checkbox" bind:checked={reecrire} disabled={options.llm.length === 0} />
        Réécrire mon idée
      </label>
      {#if reecrire}
        <label class="avec">
          <span class="discret">avec</span>
          <select bind:value={llmChoisi} aria-label="LLM qui réécrit">
            {#each options.llm as choix (choix.fichier)}
              <option value={choix.fichier}>{choix.nom}{choix.specialiste ? ' (spécialiste)' : ''}</option>
            {/each}
          </select>
        </label>
      {/if}
    </div>
    {#if options.llm.length === 0}
      <p class="discret">
        Aucun LLM dans modeles\llm : ton texte part tel quel. Les LLM téléchargés par la version 2 sont repris.
      </p>
    {/if}

    <label class="prompt">
      <span class="etiquette">{reecrire ? 'Idée' : 'Prompt'}</span>
      <textarea
        rows="4"
        bind:value={prompt}
        placeholder={reecrire
          ? 'Un renard roux assis dans la neige fraîche, lumière dorée du soir'
          : 'A red fox sitting in fresh snow, golden hour, 35 mm photo'}
      ></textarea>
    </label>
    {#if seed === undefined}
      <p class="erreur">La seed est un entier positif, ou rien pour la tirer au hasard.</p>
    {/if}
    <div class="actions-gauche">
      {#if ecriture}
        <span class="discret">Le LLM écrit…</span>
        <AnnulerEcriture />
      {:else}
        <Bouton petit desactive={!workflow || options.llm.length === 0 || prompt.trim() === ''} onclick={() => void ecrire()}>
          ✨ Écrire avec le LLM
        </Bouton>
        <Bouton genre="discret" petit desactive={texteOrigine === null} onclick={retrouver}>↩ Retrouver mon texte</Bouton>
      {/if}
    </div>

    <div class="grille">
      <label>
        <span class="etiquette">Modèle</span>
        <select value={workflowId} onchange={(e) => choisirWorkflow(e.currentTarget.value)}>
          {#each proposes as choix (choix.id)}
            <option value={choix.id}>{choix.nom}</option>
          {/each}
        </select>
      </label>
      <label>
        <span class="etiquette">Checkpoint</span>
        <select bind:value={modele} disabled={!workflow || workflow.modeles.length === 0}>
          {#each workflow?.modeles ?? [] as fichier (fichier)}
            <option value={fichier}>{nomCheckpoint(fichier)}</option>
          {/each}
        </select>
      </label>
      <label>
        <span class="etiquette">Format</span>
        <select bind:value={format}>
          {#if modeReference === 'variation'}
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
      <label>
        <span class="etiquette">Images</span>
        <input type="number" min="1" max={options.nombreMax} bind:value={nombre} />
      </label>
      <RangerDans genre="images" bind:dossier />
    </div>

    {#if workflow}
      <ChoixLoras
        famille={workflow.famille}
        proposees={workflow.loras}
        forceMax={options.loraForceMax}
        bind:lignes={lignesLoras}
      />
    {/if}

    {#if workflow && workflow.aValider.length > 0}
      <div class="a-valider">
        {#each workflow.aValider as checkpoint (checkpoint)}
          <p>
            <b>{nomCheckpoint(checkpoint)}</b> attend que ses steps soient validés avant de servir.
            {texteStepsSuggeres(workflow.stepsSuggeres)}
          </p>
          <div class="ligne">
            <label>Steps <input type="number" min="1" max="150" bind:value={stepsSaisis[checkpoint]} /></label>
            <Bouton petit onclick={() => void valider(checkpoint)}>Valider</Bouton>
          </div>
        {/each}
      </div>
    {/if}

    <p class="etiquette titre-champ">＋ Références</p>
    <div class="cartes">
      <CarteChoix groupe="references" valeur="aucune" titre="Aucune" bind:choisie={carte} onchange={() => changerDepart(null)}>
        Seul le texte décide.
      </CarteChoix>
      <CarteChoix groupe="references" valeur="inspirer" titre="🧩 Mélanger des images" bind:choisie={carte} onchange={() => changerDepart('inspirer')}>
        1 à {options.referencesMax} références nourrissent l'image.
      </CarteChoix>
      <CarteChoix groupe="references" valeur="variation" titre="🎨 Redessiner une image" bind:choisie={carte} onchange={() => changerDepart('variation')}>
        Une image sert de base, tu règles la liberté.
      </CarteChoix>
    </div>
    {#if modeReference}
      {#if proposes.length === 0}
        <p class="discret">
          {modeReference === 'inspirer'
            ? "Aucun modèle prêt ne reçoit d'images de référence : il faut Qwen-Image 2.1 Edit."
            : 'Aucun modèle prêt ne sait redessiner une image.'}
        </p>
      {:else}
        <p class="discret">
          {modeReference === 'inspirer'
            ? `Choisis de 1 à ${maxRushes} rushes, dans l'ordre : le premier est l'image 1. Écris ce qui est nouveau (« le chat de l'image 1 sur le canapé de l'image 2 »).`
            : 'Choisis le rush à redessiner, puis dis ce qui change.'}
        </p>
        <div class="ligne cases">
          {#each references as id, rang (id)}
            <div class="case">
              <VignetteCreation {id} titre={`Image ${rang + 1}`} />
              {#if modeReference === 'inspirer'}<span class="rang">{rang + 1}</span>{/if}
              <button type="button" class="retire" aria-label={`Retirer l'image ${rang + 1}`} onclick={() => retirerReference(id)}>✕</button>
            </div>
          {/each}
          {#if references.length < maxRushes}
            <button type="button" class="case vide-case" aria-label="Choisir un rush" onclick={() => (choixRush = true)}>＋</button>
          {/if}
          {#if modeReference === 'variation'}
            <label class="liberte">
              Liberté <input type="range" min={LIBERTE.min} max={LIBERTE.max} step="0.05" bind:value={liberte} />
              <span class="code">{liberte.toFixed(2)}</span>
              <span class="discret">0 : copie ; 1 : libre</span>
            </label>
          {/if}
        </div>
      {/if}
    {/if}

    <div class="actions">
      <Bouton genre="principal" desactive={!peutCreer} onclick={() => void creer()}>Créer</Bouton>
    </div>
  </Panneau>
{/if}

<Panneau titre="Rendu">
  {#if commandes.length === 0}
    <p class="discret">Rien n'est en cours. Tes dernières créations apparaissent ici.</p>
  {:else}
    <ul class="file">
      {#each commandes as c (c.id)}
        <li class:erreur={c.etat === 'erreur'}>
          <div class="texte">
            <span class="prompt-court">{texteCommande(c)}</span>
            <span class="discret">{texteAvancement(c)}</span>
            {#if c.etat === 'erreur' && c.erreur}
              <ExpliquerErreur erreur={c.erreur} origine={'plateau'} commande={c.id} contexte={c.etape} />
            {/if}
            {#if c.note}
              <span class="note-reecriture">⚠ {c.note}</span>
            {/if}
            {#if c.etat === 'en_cours'}
              <progress max="1" value={c.progression}></progress>
            {/if}
            {#if c.type === 'creation' && c.etat === 'termine' && c.images.length > 0}
              <div class="vignettes">
                {#each c.images as image (image)}
                  <VignetteCreation id={image} titre={texteCommande(c)} />
                {/each}
              </div>
            {/if}
          </div>
          {#if estActive(c) && !c.annulee}
            <Bouton genre="discret" petit onclick={() => void annuler(c.id)}>✕ Annuler</Bouton>
          {:else if c.type === 'creation' && c.etat === 'termine' && c.images.length > 0 && vision?.llm}
            {#if lotEnCours === c.id}
              <span class="discret">Le LLM regarde…</span>
              <AnnulerEcriture />
            {:else}
              <Bouton petit desactive={lotEnCours !== null} onclick={() => void verifierLot(c)}>
                {c.images.length > 1 ? 'Vérifier les images' : 'Vérifier l’image'}
              </Bouton>
              {#if peutChoisir(c.images.length)}
                <Bouton petit desactive={lotEnCours !== null} onclick={() => void choisirLot(c)}>
                  ★ Choisir les meilleures
                </Bouton>
              {/if}
            {/if}
          {/if}
        </li>
      {/each}
    </ul>
  {/if}
  <div class="ligne verification">
    <label>
      <input type="checkbox" bind:checked={verifierALaFin} disabled={!vision?.llm} />
      Vérifier les images à la fin
    </label>
    <span class="discret">
      {vision?.llm
        ? `Le LLM qui voit (${vision.llm}) compare chaque image à ton intention et propose un prompt corrigé.`
        : (vision?.explication ?? '')}
    </span>
  </div>
</Panneau>

<PanneauRushes {signaler} {rafraichir} enFile={() => void relireCommandes()} />

{#if choixRush}
  <Dialogue titre="Quel rush choisir ?" fermer={() => (choixRush = false)}>
    <ChoixRushes
      max={maxRushes}
      bind:selection={references}
      {signaler}
      apresChoix={() => {
        if (maxRushes <= 1) choixRush = false;
      }}
    />
    {#snippet pied()}
      <Bouton genre="discret" onclick={() => (choixRush = false)}>✕ Fermer</Bouton>
    {/snippet}
  </Dialogue>
{/if}

<style>
  .grille {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(150px, 1fr));
    gap: 12px 16px;
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
    margin-top: 14px;
  }

  .reecriture {
    margin-bottom: 10px;
  }

  .avec {
    flex-direction: row;
    align-items: center;
  }

  .actions-gauche {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px;
    margin: 10px 0 14px;
  }

  .titre-champ {
    margin: 14px 0 8px;
  }

  .cartes {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(210px, 1fr));
    gap: 12px;
    margin-bottom: 10px;
  }

  .cases {
    flex-wrap: wrap;
    align-items: flex-end;
    gap: 12px;
  }

  .case {
    position: relative;
    display: block;
    width: 96px;
    height: 96px;
  }

  .vide-case {
    display: grid;
    place-items: center;
    font: inherit;
    font-size: 22px;
    color: var(--texte-2);
    background: var(--fond);
    border: 1px dashed var(--ligne);
    cursor: pointer;
  }

  .vide-case:hover {
    color: var(--cyan);
    border-color: var(--cyan);
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

  .rang {
    position: absolute;
    bottom: 2px;
    left: 2px;
    padding: 0 6px;
    font-size: 12px;
    font-weight: 600;
    color: var(--fond);
    background: var(--orange);
  }

  .liberte {
    flex-direction: row;
    align-items: center;
    gap: 10px;
  }

  .vignettes {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  .verification {
    flex-wrap: wrap;
    margin-top: 14px;
  }

  textarea {
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

  .erreur {
    color: var(--rouge);
  }

  .a-valider {
    margin-top: 14px;
    padding: 10px 12px;
    border-left: 2px solid var(--ambre);
  }

  .a-valider p {
    margin: 0 0 8px;
  }

  .ligne {
    display: flex;
    flex-direction: row;
    align-items: center;
    gap: 12px;
  }

  .ligne label {
    flex-direction: row;
    align-items: center;
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    margin-top: 14px;
  }

  .file {
    display: flex;
    flex-direction: column;
    gap: 10px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .file li {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .file .texte {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
  }

  .file li.erreur .discret {
    color: var(--rouge);
  }

  .prompt-court {
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }

  progress {
    width: 100%;
    height: 4px;
    accent-color: var(--cyan);
  }




  .note-reecriture {
    color: var(--orange);
    font-size: 12px;
  }
</style>
