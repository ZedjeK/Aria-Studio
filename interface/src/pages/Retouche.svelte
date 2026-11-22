<!--
  La Retouche, comme en v2 : une image des Rushes (ou importée), retouchée au pinceau ou agrandie, puis comparée à
  l'original. La consigne de retouche part telle quelle, ou traduite par le LLM ; la précision d'un agrandissement est
  traduite d'office quand un LLM est installé. Le bouton orange agit sur le panneau accentué.
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import {
    FORMAT_IMAGE,
    annulerCommande,
    commanderAgrandissement,
    commanderRetouche,
    imageCreation,
    importerImage,
    lireCommandes,
    lireRushes,
    messageErreur,
    optionsRetouche,
    type Commande,
    type Creation,
    type OptionsPlateau,
  } from '../lib/api';
  import { estActive, lireSeed, llmInitial, modeleInitial, nomCheckpoint, texteAvancement, texteCommande } from '../lib/plateau';
  import {
    ANCRAGES,
    FONDU_AGRANDISSEMENT,
    FONDU_MAX,
    FONDU_RETOUCHE,
    pointsDuTrait,
    problemeTaille,
    tailleProposee,
    versImage,
  } from '../lib/retouche';
  import ASavoir from '../lib/composants/ASavoir.svelte';
  import Bouton from '../lib/composants/Bouton.svelte';
  import ChoixLoras from '../lib/composants/ChoixLoras.svelte';
  import { lignesPour, lorasChoisies, type LigneLora } from '../lib/loras';
  import Panneau from '../lib/composants/Panneau.svelte';

  interface Props {
    signaler: (texte: string, erreur?: boolean) => void;
  }

  let { signaler }: Props = $props();

  type Mode = 'retouche' | 'agrandissement';

  let options = $state<OptionsPlateau | null>(null);
  let rushes = $state<Creation[]>([]);
  let vignettes = $state<Record<string, string>>({});
  let commandes = $state<Commande[]>([]);
  let source = $state<Creation | null>(null);
  let mode = $state<Mode>('retouche');

  let workflowId = $state('');
  let modele = $state('');
  let llmChoisi = $state('');
  let consigne = $state('');
  let reecrire = $state(false);
  let fondu = $state(FONDU_RETOUCHE);
  let format = $state(FORMAT_IMAGE);
  let nombre = $state(1);
  let seedTexte = $state('');
  let largeur = $state(0);
  let hauteur = $state(0);
  let ancrage = $state('centre');
  let envoi = $state(false);

  // Le masque : un canevas à la taille de l'image, peint en blanc opaque sur fond transparent.
  let canevas = $state<HTMLCanvasElement | null>(null);
  let pinceau = $state(40);
  let gomme = $state(false);
  let peint = $state(false);
  let dernier: { x: number; y: number } | null = null;

  const workflow = $derived(options?.workflows.find((w) => w.id === workflowId));
  /** Les LoRA de la famille du workflow : cochées, force, mots déclencheurs. */
  let lignesLoras = $state<Record<string, LigneLora>>({});
  const seed = $derived(lireSeed(seedTexte));
  const probleme = $derived(source && mode === 'agrandissement' ? problemeTaille(source, largeur, hauteur) : null);
  const resultats = $derived(commandes.filter((c) => c.type !== 'creation'));
  const dernierResultat = $derived(resultats.find((c) => c.images.length > 0));
  const peutLancer = $derived(
    !envoi &&
      source !== null &&
      workflow !== undefined &&
      modele !== '' &&
      seed !== undefined &&
      Number.isInteger(nombre) &&
      nombre >= 1 &&
      nombre <= (options?.nombreMax ?? 1) &&
      (!reecrire || llmChoisi !== '') &&
      (mode === 'agrandissement' ? probleme === null : consigne.trim() !== ''),
  );

  async function vignette(id: string): Promise<void> {
    if (vignettes[id]) return;
    try {
      const octets = await imageCreation(id);
      vignettes[id] = URL.createObjectURL(new Blob([octets], { type: 'image/png' }));
    } catch {
      // Une image effacée entre-temps : la fiche reste, sans vignette.
    }
  }

  function choisirWorkflow(id: string) {
    workflowId = id;
    const choisi = options?.workflows.find((w) => w.id === id);
    modele = modeleInitial(choisi);
    lignesLoras = lignesPour(choisi?.loras ?? [], lignesLoras);
    if (options) llmChoisi = llmInitial(options, id);
  }

  async function relireOptions() {
    try {
      options = await optionsRetouche();
      const garde = options.workflows.some((w) => w.id === workflowId);
      const defaut = options.workflows.find((w) => w.id === options?.defaut) ?? options.workflows[0];
      choisirWorkflow(garde ? workflowId : (defaut?.id ?? ''));
    } catch (e) {
      signaler(messageErreur(e), true);
    }
  }

  async function relireRushes() {
    try {
      rushes = await lireRushes('tous', null);
      await Promise.all(rushes.slice(0, 60).map((r) => vignette(r.id)));
    } catch (e) {
      signaler(messageErreur(e), true);
    }
  }

  async function relireCommandes() {
    try {
      const avant = commandes.reduce((total, c) => total + c.images.length, 0);
      commandes = await lireCommandes();
      const apres = commandes.reduce((total, c) => total + c.images.length, 0);
      if (apres !== avant) {
        await relireRushes();
        for (const c of commandes) for (const id of c.images) await vignette(id);
      }
    } catch (e) {
      signaler(messageErreur(e), true);
    }
  }

  function effacerMasque() {
    const ctx = canevas?.getContext('2d');
    if (ctx && canevas) ctx.clearRect(0, 0, canevas.width, canevas.height);
    peint = false;
  }

  function prendre(rush: Creation) {
    source = rush;
    void vignette(rush.id);
    effacerMasque();
    const propose = tailleProposee(rush);
    largeur = propose.largeur;
    hauteur = propose.hauteur;
    format = FORMAT_IMAGE;
  }

  function changerMode(nouveau: Mode) {
    mode = nouveau;
    fondu = nouveau === 'agrandissement' ? FONDU_AGRANDISSEMENT : FONDU_RETOUCHE;
    // Une précision d'agrandissement se traduit d'office quand un LLM est là ; une consigne de retouche part telle quelle.
    reecrire = nouveau === 'agrandissement' && (options?.llm.length ?? 0) > 0;
  }

  function importer(evenement: Event & { currentTarget: HTMLInputElement }) {
    const fichier = evenement.currentTarget.files?.[0];
    evenement.currentTarget.value = '';
    if (!fichier) return;
    const lecteur = new FileReader();
    lecteur.onload = () => {
      if (typeof lecteur.result !== 'string') return;
      void (async () => {
        try {
          const fiche = await importerImage(lecteur.result as string, fichier.name);
          signaler(`Image importée : ${fiche.largeur}×${fiche.hauteur}.`);
          await relireRushes();
          prendre(fiche);
        } catch (e) {
          signaler(messageErreur(e), true);
        }
      })();
    };
    lecteur.readAsDataURL(fichier);
  }

  function peindre(x: number, y: number) {
    const ctx = canevas?.getContext('2d');
    if (!ctx || !canevas || !source) return;
    const point = versImage(x, y, canevas.getBoundingClientRect(), source);
    const rayon = pinceau / 2;
    ctx.globalCompositeOperation = gomme ? 'destination-out' : 'source-over';
    ctx.fillStyle = '#ffffff';
    for (const p of dernier ? pointsDuTrait(dernier, point, rayon) : [point]) {
      ctx.beginPath();
      ctx.arc(p.x, p.y, rayon, 0, Math.PI * 2);
      ctx.fill();
    }
    dernier = point;
    if (!gomme) peint = true;
  }

  function appuyer(e: PointerEvent) {
    if (!canevas) return;
    canevas.setPointerCapture(e.pointerId);
    dernier = null;
    peindre(e.clientX, e.clientY);
  }

  function glisser(e: PointerEvent) {
    if (dernier && canevas?.hasPointerCapture(e.pointerId)) peindre(e.clientX, e.clientY);
  }

  function relacher() {
    dernier = null;
  }

  async function lancer() {
    if (!source || !workflow || seed === undefined) return;
    envoi = true;
    try {
      const commun = {
        source: source.id,
        reecrire,
        llm: reecrire ? llmChoisi || null : null,
        workflow: workflow.id,
        fondu,
        seed,
        modele: modele || null,
        nombre,
        loras: lorasChoisies(workflow.loras, lignesLoras),
      };
      if (mode === 'retouche') {
        const masque = peint && canevas ? canevas.toDataURL('image/png') : null;
        await commanderRetouche({ ...commun, instruction: consigne, masque, format: format === FORMAT_IMAGE ? null : format });
      } else {
        await commanderAgrandissement({ ...commun, largeur, hauteur, ancrage, precision: consigne });
      }
      signaler(mode === 'retouche' ? 'Retouche en file.' : 'Agrandissement en file.');
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
    void relireRushes();
    void relireCommandes();
    const minuterie = setInterval(() => {
      if (commandes.some(estActive)) void relireCommandes();
    }, 1000);
    const auRetour = () => void relireOptions();
    window.addEventListener('focus', auRetour);
    return () => {
      clearInterval(minuterie);
      window.removeEventListener('focus', auRetour);
      for (const adresse of Object.values(vignettes)) URL.revokeObjectURL(adresse);
    };
  });
</script>

<h1>Retouche</h1>
<p class="sous-titre">Retoucher une image au pinceau, ou l'agrandir.</p>
<ASavoir>
  Choisis une image des Rushes, ou importe la tienne. Peins la zone à changer : le reste revient au pixel près, avec
  un fondu sur le bord. Sans masque, toute l'image est réinterprétée. La consigne et les textes d'origine de l'image
  passent par le filtre AS-18.
</ASavoir>

<Panneau titre="Image de départ">
  <div class="ligne">
    <label class="bouton-fichier">
      Importer une image
      <input type="file" accept="image/png,image/jpeg,image/webp" onchange={importer} />
    </label>
    {#if source}
      <span class="discret">{source.largeur}×{source.hauteur} · {source.idee || source.prompt || 'image importée'}</span>
    {/if}
  </div>
  {#if rushes.length === 0}
    <p class="discret">Aucun rush pour l'instant : crée une image au Plateau, ou importe-en une.</p>
  {:else}
    <div class="choix">
      {#each rushes.slice(0, 60) as rush (rush.id)}
        <button type="button" class="vignette" class:choisi={source?.id === rush.id} aria-pressed={source?.id === rush.id} onclick={() => prendre(rush)}>
          {#if vignettes[rush.id]}
            <img src={vignettes[rush.id]} alt={rush.prompt ?? ''} />
          {:else}
            <span class="vide" aria-hidden="true"></span>
          {/if}
        </button>
      {/each}
    </div>
  {/if}
</Panneau>

{#if source && options}
  <Panneau titre={mode === 'retouche' ? 'Retoucher' : 'Agrandir'} accentue>
    <div class="modes" role="tablist" aria-label="Action">
      <button type="button" role="tab" class:actif={mode === 'retouche'} aria-selected={mode === 'retouche'} onclick={() => changerMode('retouche')}>
        Retoucher
      </button>
      <button type="button" role="tab" class:actif={mode === 'agrandissement'} aria-selected={mode === 'agrandissement'} onclick={() => changerMode('agrandissement')}>
        Agrandir
      </button>
    </div>

    {#if options.workflows.length === 0}
      <p>Aucun modèle de retouche prêt : il manque des fichiers à {options.incomplets.join(', ') || 'Qwen-Image 2.1 Edit'}.</p>
    {:else}
      <div class="atelier" style:aspect-ratio="{source.largeur} / {source.hauteur}">
        {#if vignettes[source.id]}
          <img src={vignettes[source.id]} alt="Source de la retouche" />
        {/if}
        {#if mode === 'retouche'}
          <canvas
            bind:this={canevas}
            width={source.largeur}
            height={source.hauteur}
            aria-label="Masque : peins la zone à retoucher"
            onpointerdown={appuyer}
            onpointermove={glisser}
            onpointerup={relacher}
            onpointercancel={relacher}
          ></canvas>
        {/if}
      </div>

      {#if mode === 'retouche'}
        <div class="ligne">
          <label>Pinceau <input type="range" min="5" max="200" bind:value={pinceau} /> <span class="code">{pinceau} px</span></label>
          <label><input type="checkbox" bind:checked={gomme} /> Gomme</label>
          <Bouton genre="discret" petit onclick={effacerMasque}>Effacer le masque</Bouton>
          <label>
            Format
            <select bind:value={format} disabled={peint}>
              <option value={FORMAT_IMAGE}>Celui de l'image</option>
              {#each options.formats as f (f)}
                <option value={f}>{f}</option>
              {/each}
            </select>
          </label>
        </div>
        {#if peint && format !== FORMAT_IMAGE}
          <p class="erreur">Un format ne se combine pas avec un masque : efface le masque, ou garde le format de l'image.</p>
        {/if}
      {:else}
        <div class="ligne">
          <label>Largeur <input type="number" min={source.largeur} max="4096" bind:value={largeur} /></label>
          <label>Hauteur <input type="number" min={source.hauteur} max="4096" bind:value={hauteur} /></label>
          <div class="ancrages" role="radiogroup" aria-label="Place de l'image">
            {#each ANCRAGES as a (a.id)}
              <button type="button" role="radio" aria-checked={ancrage === a.id} class:actif={ancrage === a.id} title={a.titre} onclick={() => (ancrage = a.id)}>
                {a.fleche}
              </button>
            {/each}
          </div>
        </div>
        {#if probleme}
          <p class="erreur">{probleme}</p>
        {/if}
      {/if}

      <label class="consigne">
        <span class="etiquette">{mode === 'retouche' ? 'Consigne' : 'Précision (facultative)'}</span>
        <textarea
          rows="3"
          bind:value={consigne}
          placeholder={mode === 'retouche'
            ? reecrire
              ? 'En français : donne-lui une veste en cuir rouge'
              : 'Telle qu’elle partira au modèle, en anglais : give her a red leather jacket'
            : 'Ajoute la mer à droite'}
        ></textarea>
      </label>

      <div class="ligne">
        <label>
          <input type="checkbox" bind:checked={reecrire} disabled={options.llm.length === 0} />
          {mode === 'retouche' ? 'Traduire ma consigne avec le LLM' : 'Traduire ma précision avec le LLM'}
        </label>
        {#if reecrire}
          <select bind:value={llmChoisi} aria-label="LLM qui traduit">
            {#each options.llm as choix (choix.fichier)}
              <option value={choix.fichier}>{choix.nom}{choix.specialiste ? ' (spécialiste)' : ''}</option>
            {/each}
          </select>
        {/if}
      </div>

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
          <span class="etiquette">Fondu</span>
          <input type="range" min="0" max={FONDU_MAX} bind:value={fondu} />
          <span class="code">{fondu} px</span>
        </label>
        <label>
          <span class="etiquette">Variantes</span>
          <input type="number" min="1" max={options.nombreMax} bind:value={nombre} />
        </label>
        <label>
          <span class="etiquette">Seed</span>
          <input type="text" inputmode="numeric" placeholder="au hasard" bind:value={seedTexte} />
        </label>
      </div>
      {#if workflow && workflow.modeles.length === 0}
        <p class="discret">
          Les steps de ce checkpoint attendent d'être validés : fais-le depuis le Plateau ou Modèles → Ma collection.
        </p>
      {/if}
      {#if workflow}
        <ChoixLoras
          famille={workflow.famille}
          proposees={workflow.loras}
          forceMax={options.loraForceMax}
          bind:lignes={lignesLoras}
        />
      {/if}

      <div class="actions">
        <Bouton genre="principal" desactive={!peutLancer || (peint && format !== FORMAT_IMAGE && mode === 'retouche')} onclick={() => void lancer()}>
          {mode === 'retouche' ? 'Retoucher' : 'Agrandir'}
        </Bouton>
      </div>
    {/if}
  </Panneau>
{/if}

{#if resultats.length > 0}
  <Panneau titre="File">
    <ul class="file">
      {#each resultats as c (c.id)}
        <li class:erreur={c.etat === 'erreur'}>
          <div class="texte">
            <span class="prompt-court">{c.type === 'agrandissement' ? 'Agrandissement' : 'Retouche'} · {texteCommande(c) || 'sans précision'}</span>
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

{#if dernierResultat && dernierResultat.parent}
  <Panneau titre="Avant · après">
    <div class="comparaison">
      <figure>
        {#if vignettes[dernierResultat.parent]}
          <img src={vignettes[dernierResultat.parent]} alt="Original" />
        {/if}
        <figcaption class="discret">Original</figcaption>
      </figure>
      {#each dernierResultat.images as id (id)}
        <figure>
          {#if vignettes[id]}
            <img src={vignettes[id]} alt="Résultat" />
          {/if}
          <figcaption class="discret">Résultat</figcaption>
        </figure>
      {/each}
    </div>
  </Panneau>
{/if}

<style>
  .ligne {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px 16px;
    margin: 10px 0;
  }

  .ligne label {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .bouton-fichier {
    position: relative;
    padding: 6px 12px;
    border: 1px solid var(--ligne);
    cursor: pointer;
  }

  .bouton-fichier input {
    position: absolute;
    inset: 0;
    opacity: 0;
    cursor: pointer;
  }

  .choix {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(84px, 1fr));
    gap: 8px;
    max-height: 220px;
    overflow-y: auto;
  }

  .vignette {
    padding: 2px;
    background: transparent;
    border: 1px solid transparent;
    cursor: pointer;
  }

  .vignette.choisi {
    border-color: var(--orange);
  }

  .vignette img,
  .vignette .vide {
    display: block;
    width: 100%;
    aspect-ratio: 1;
    object-fit: cover;
    background: var(--fond);
  }

  .modes {
    display: flex;
    gap: 6px;
    margin-bottom: 12px;
  }

  .modes button,
  .ancrages button {
    font: inherit;
    color: var(--texte-2);
    background: transparent;
    border: 1px solid var(--ligne);
    padding: 4px 12px;
    cursor: pointer;
  }

  .modes button.actif,
  .ancrages button.actif {
    color: var(--cyan);
    border-color: var(--cyan);
  }

  .ancrages {
    display: grid;
    grid-template-columns: repeat(3, 32px);
    gap: 2px;
  }

  .ancrages button {
    padding: 2px 0;
  }

  .atelier {
    position: relative;
    max-width: 100%;
    margin: 0 auto;
    border: 1px solid var(--ligne);
    background: var(--fond);
  }

  .atelier img,
  .atelier canvas {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
  }

  .atelier canvas {
    opacity: 0.5;
    cursor: crosshair;
    touch-action: none;
  }

  .consigne,
  .grille label {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .consigne {
    margin-top: 10px;
  }

  .grille {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(150px, 1fr));
    gap: 12px 16px;
    margin-top: 12px;
  }

  .etiquette {
    color: var(--texte-2);
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.14em;
    text-transform: uppercase;
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

  .erreur {
    color: var(--rouge);
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

  .comparaison {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
    gap: 14px;
  }

  figure {
    margin: 0;
  }

  figure img {
    display: block;
    width: 100%;
    height: auto;
    border: 1px solid var(--ligne);
  }
</style>
