<!--
  Le Plateau : créer une image depuis un prompt écrit tel quel, suivre la file, voir les dernières créations. La
  réécriture par le LLM, les références, la retouche et la vidéo viendront ensuite. Le bouton orange (Créer) agit
  sur le panneau accentué.
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import {
    annulerCommande,
    commanderImage,
    imageCreation,
    lireCommandes,
    lireCreations,
    messageErreur,
    optionsPlateau,
    validerCheckpoint,
    type Commande,
    type Creation,
    type OptionsPlateau,
  } from '../lib/api';
  import {
    estActive,
    formatInitial,
    lireSeed,
    modeleInitial,
    nomCheckpoint,
    texteAvancement,
    texteStepsSuggeres,
  } from '../lib/plateau';
  import ASavoir from '../lib/composants/ASavoir.svelte';
  import Bouton from '../lib/composants/Bouton.svelte';
  import Panneau from '../lib/composants/Panneau.svelte';

  interface Props {
    signaler: (texte: string, erreur?: boolean) => void;
  }

  let { signaler }: Props = $props();

  let options = $state<OptionsPlateau | null>(null);
  let commandes = $state<Commande[]>([]);
  let creations = $state<Creation[]>([]);
  // Les images affichées : une adresse blob: par création, libérée en quittant la page.
  let vignettes = $state<Record<string, string>>({});

  let workflowId = $state('');
  let prompt = $state('');
  let modele = $state('');
  let format = $state('');
  let nombre = $state(1);
  let seedTexte = $state('');
  let stepsSaisis = $state<Record<string, number>>({});
  let envoi = $state(false);

  const workflow = $derived(options?.workflows.find((w) => w.id === workflowId));
  const seed = $derived(lireSeed(seedTexte));
  const nombreValide = $derived(Number.isInteger(nombre) && nombre >= 1 && nombre <= (options?.nombreMax ?? 1));
  const peutCreer = $derived(
    !envoi && workflow !== undefined && modele !== '' && prompt.trim() !== '' && seed !== undefined && nombreValide,
  );

  function choisirWorkflow(id: string) {
    workflowId = id;
    const choisi = options?.workflows.find((w) => w.id === id);
    modele = modeleInitial(choisi);
    if (options) format = formatInitial(options, choisi);
    for (const checkpoint of choisi?.aValider ?? []) {
      stepsSaisis[checkpoint] ??= choisi?.stepsSuggeres[0] ?? 20;
    }
  }

  async function relireOptions() {
    try {
      options = await optionsPlateau();
      const garde = options.workflows.some((w) => w.id === workflowId);
      const defaut = options.workflows.find((w) => w.id === options?.defaut) ?? options.workflows[0];
      choisirWorkflow(garde ? workflowId : (defaut?.id ?? ''));
    } catch (e) {
      signaler(messageErreur(e), true);
    }
  }

  async function chargerVignette(id: string) {
    if (vignettes[id]) return;
    try {
      const octets = await imageCreation(id);
      vignettes[id] = URL.createObjectURL(new Blob([octets], { type: 'image/png' }));
    } catch {
      // Une image effacée entre-temps : la fiche reste, sans vignette.
    }
  }

  async function relireCreations() {
    try {
      creations = await lireCreations();
      await Promise.all(creations.map((c) => chargerVignette(c.id)));
    } catch (e) {
      signaler(messageErreur(e), true);
    }
  }

  async function relireCommandes() {
    try {
      const avant = commandes.reduce((total, c) => total + c.images.length, 0);
      commandes = await lireCommandes();
      const apres = commandes.reduce((total, c) => total + c.images.length, 0);
      if (apres !== avant) await relireCreations();
    } catch (e) {
      signaler(messageErreur(e), true);
    }
  }

  async function creer() {
    if (!workflow || seed === undefined) return;
    envoi = true;
    try {
      await commanderImage({ prompt, workflow: workflow.id, format, seed, nombre, modele: modele || null });
      signaler(nombre > 1 ? `${nombre} images en file.` : 'Image en file.');
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

  onMount(() => {
    void relireOptions();
    void relireCommandes();
    void relireCreations();
    const minuterie = setInterval(() => {
      if (commandes.some(estActive)) void relireCommandes();
    }, 1000);
    // Un modèle déposé à la main pendant que la page est ouverte : relu au retour dans la fenêtre.
    const auRetour = () => void relireOptions();
    window.addEventListener('focus', auRetour);
    return () => {
      clearInterval(minuterie);
      window.removeEventListener('focus', auRetour);
      for (const adresse of Object.values(vignettes)) URL.revokeObjectURL(adresse);
    };
  });
</script>

<h1>Plateau</h1>
<p class="sous-titre">Créer une image à partir d'un prompt.</p>
<ASavoir>
  Le prompt part tel quel vers le modèle, de préférence en anglais : la réécriture par le LLM arrive dans une prochaine
  version. Tout prompt passe par le filtre AS-18 avant d'entrer dans la file. Les images sont rangées dans
  data\creations, avec leur fiche, comme en version 2.
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
  <Panneau titre="Créer" accentue>
    <div class="grille">
      <label>
        <span class="etiquette">Modèle</span>
        <select value={workflowId} onchange={(e) => choisirWorkflow(e.currentTarget.value)}>
          {#each options.workflows as choix (choix.id)}
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
          {#each options.formats as f (f)}
            <option value={f}>{f}</option>
          {/each}
        </select>
      </label>
      <label>
        <span class="etiquette">Images</span>
        <input type="number" min="1" max={options.nombreMax} bind:value={nombre} />
      </label>
      <label>
        <span class="etiquette">Seed</span>
        <input type="text" inputmode="numeric" placeholder="au hasard" bind:value={seedTexte} class:invalide={seed === undefined} />
      </label>
    </div>

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

    <label class="prompt">
      <span class="etiquette">Prompt</span>
      <textarea rows="4" bind:value={prompt} placeholder="A red fox sitting in fresh snow, golden hour, 35 mm photo"></textarea>
    </label>
    {#if seed === undefined}
      <p class="erreur">La seed est un entier positif, ou rien pour la tirer au hasard.</p>
    {/if}
    <div class="actions">
      <Bouton genre="principal" desactive={!peutCreer} onclick={() => void creer()}>Créer</Bouton>
    </div>
  </Panneau>
{/if}

{#if commandes.length > 0}
  <Panneau titre="File">
    <ul class="file">
      {#each commandes as c (c.id)}
        <li class:erreur={c.etat === 'erreur'}>
          <div class="texte">
            <span class="prompt-court">{c.prompt}</span>
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

<Panneau titre="Dernières créations">
  {#if creations.length === 0}
    <p class="discret">Rien pour l'instant.</p>
  {:else}
    <div class="galerie">
      {#each creations as creation (creation.id)}
        <figure>
          {#if vignettes[creation.id]}
            <img src={vignettes[creation.id]} alt={creation.prompt ?? ''} width={creation.largeur} height={creation.hauteur} />
          {:else}
            <div class="vide" aria-hidden="true"></div>
          {/if}
          <figcaption>
            <span class="prompt-court">{creation.prompt || 'Sans prompt'}</span>
            <span class="discret code">{creation.largeur}×{creation.hauteur}{creation.seed != null ? ` · seed ${creation.seed}` : ''}</span>
          </figcaption>
        </figure>
      {/each}
    </div>
  {/if}
</Panneau>

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

  .galerie {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(170px, 1fr));
    gap: 14px;
  }

  figure {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin: 0;
    min-width: 0;
  }

  img,
  .vide {
    width: 100%;
    height: auto;
    aspect-ratio: 1;
    object-fit: cover;
    border: 1px solid var(--ligne);
    background: var(--fond);
  }

  figcaption {
    display: flex;
    flex-direction: column;
    gap: 2px;
    font-size: 12px;
  }
</style>
