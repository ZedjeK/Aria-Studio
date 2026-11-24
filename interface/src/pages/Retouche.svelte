<!--
  La Retouche, comme en v2 : une image des Rushes (ou importée), retouchée au pinceau ou agrandie, puis comparée à
  l'original. La consigne de retouche part telle quelle, ou traduite par le LLM ; la précision d'un agrandissement est
  traduite d'office quand un LLM est installé. Le bouton orange agit sur le panneau accentué.
-->
<script lang="ts">
  import { onMount, tick } from 'svelte';
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
    type EnvoiRetouche,
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
    texteCadre,
    versImage,
  } from '../lib/retouche';
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

  interface Props {
    signaler: (texte: string, erreur?: boolean) => void;
    /** Une consigne écrite au Scénario, avec son image. */
    recu?: EnvoiRetouche | null;
    recuLu?: () => void;
    /** Ouvre le Scénario (le bouton n'apparaît que si la page peut y aller). */
    ouvrirScenario?: () => void;
  }

  let { signaler, recu = null, recuLu = () => {}, ouvrirScenario }: Props = $props();

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
  /** « Ranger dans » : le dossier des prochaines créations. */
  let dossier = $state<string | null>(null);
  /** Change quand de nouvelles images sont enregistrées : les Rushes se relisent. */
  let rafraichir = $state(0);
  /** Le dialogue « Quel rush choisir ? » et le rush qu'on y touche. */
  let choixRush = $state(false);
  let choisie = $state<string[]>([]);
  /** Avant · après : la variante regardée, le mode côte à côte, la place du curseur (en %). */
  let choisiResultat = $state<string | null>(null);
  let cote = $state(false);
  let curseur = $state(50);

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
  /** La variante comparée à l'original : celle qu'on a cliquée, sinon la première du dernier résultat. */
  const resultatId = $derived(
    dernierResultat && choisiResultat && dernierResultat.images.includes(choisiResultat)
      ? choisiResultat
      : (dernierResultat?.images[0] ?? null),
  );
  const resultatFiche = $derived(resultatId ? rushes.find((r) => r.id === resultatId) : undefined);
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
        rafraichir += 1;
        await relireRushes();
        for (const c of commandes) {
          for (const id of c.images) await vignette(id);
          if (c.parent) await vignette(c.parent);
        }
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

  /** Le rush touché dans le dialogue devient l'image de départ. */
  async function prendreChoisi() {
    await tick();
    const id = choisie[0];
    if (!id) return;
    if (!rushes.some((r) => r.id === id)) await relireRushes();
    const rush = rushes.find((r) => r.id === id);
    choisie = [];
    choixRush = false;
    if (rush) prendre(rush);
  }

  /** ↻ Repartir de ce résultat : la variante regardée devient l'image de départ. */
  function repartir() {
    if (resultatFiche) {
      prendre(resultatFiche);
      signaler('Nouveau départ depuis ce résultat.');
    }
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
          const fiche = await importerImage(lecteur.result as string, fichier.name, dossier);
          signaler(`Image importée : ${fiche.largeur}×${fiche.hauteur}.`);
          await relireRushes();
          rafraichir += 1;
          prendre(fiche);
          choixRush = false;
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
        dossier,
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
    void (async () => {
      await Promise.all([relireOptions(), relireRushes()]);
      // Une consigne écrite au Scénario : son image, sa consigne (déjà en anglais) et son modèle.
      if (recu) {
        const envoi = recu;
        recuLu();
        const rush = rushes.find((r) => r.id === envoi.source);
        if (rush) prendre(rush);
        changerMode('retouche');
        consigne = envoi.instruction;
        reecrire = false;
        if (envoi.workflow && options?.workflows.some((w) => w.id === envoi.workflow)) choisirWorkflow(envoi.workflow);
        signaler(envoi.instruction === '' ? 'Image de départ posée : peins la zone à changer et décris le changement.' : 'Consigne du Scénario posée : vérifie le masque et lance la retouche.');
      }
    })();
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

<div class="cartes" role="radiogroup" aria-label="Action">
  <CarteChoix groupe="action" valeur="retouche" titre="🖌 Retoucher" bind:choisie={mode} onchange={() => changerMode('retouche')}>
    Peins la zone à changer, décris le changement.
  </CarteChoix>
  <CarteChoix groupe="action" valeur="agrandissement" titre="↔ Agrandir" bind:choisie={mode} onchange={() => changerMode('agrandissement')}>
    Étends l'image au-delà de son cadre.
  </CarteChoix>
</div>

<Panneau titre={mode === 'retouche' ? 'Retoucher une image' : 'Agrandir une image'} accentue>
  {#if options === null}
    <p class="discret">Lecture…</p>
  {:else if options.workflows.length === 0}
    <p>Aucun modèle de retouche prêt : il manque des fichiers à {options.incomplets.join(', ') || 'Qwen-Image 2.1 Edit'}.</p>
  {:else}
    <div class="deux">
      <div class="gauche">
        {#if source}
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
          <div class="ligne">
            {#if mode === 'retouche'}
              <div class="pastilles" role="group" aria-label="Outil du masque">
                <button type="button" class:pris={!gomme} aria-pressed={!gomme} onclick={() => (gomme = false)}>🖌 Pinceau</button>
                <button type="button" class:pris={gomme} aria-pressed={gomme} onclick={() => (gomme = true)}>🧽 Gomme</button>
              </div>
              <label>Taille <input type="range" min="5" max="200" bind:value={pinceau} /> <span class="code">{pinceau} px</span></label>
              <Bouton genre="discret" petit onclick={effacerMasque}>Effacer le masque</Bouton>
            {:else}
              <span class="discret">{texteCadre(source, largeur, hauteur, ancrage)}</span>
            {/if}
            <Bouton genre="discret" petit onclick={() => (choixRush = true)}>Changer d'image</Bouton>
          </div>
          {#if mode === 'retouche' && peint && format !== FORMAT_IMAGE}
            <p class="erreur">Un format ne se combine pas avec un masque : efface le masque, ou garde le format de l'image.</p>
          {/if}
          {#if mode === 'agrandissement' && probleme}
            <p class="erreur">{probleme}</p>
          {/if}
        {:else}
          <div class="atelier-vide">
            <p class="discret">Aucune image choisie : prends un rush, ou importe la tienne.</p>
            <Bouton onclick={() => (choixRush = true)}>Choisir une image</Bouton>
          </div>
        {/if}
      </div>

      <div class="droite">
        {#if mode === 'agrandissement' && source}
          <div class="grille">
            <label>
              <span class="etiquette">Largeur</span>
              <input type="number" min={source.largeur} max="4096" bind:value={largeur} />
            </label>
            <label>
              <span class="etiquette">Hauteur</span>
              <input type="number" min={source.hauteur} max="4096" bind:value={hauteur} />
            </label>
          </div>
          <p class="etiquette titre-champ">Ancrage</p>
          <div class="ancrages" role="radiogroup" aria-label="Place de l'image">
            {#each ANCRAGES as a (a.id)}
              <button type="button" role="radio" aria-checked={ancrage === a.id} class:pris={ancrage === a.id} title={a.titre} onclick={() => (ancrage = a.id)}>
                {a.fleche}
              </button>
            {/each}
          </div>
        {/if}

        <label class="consigne">
          <span class="etiquette">{mode === 'retouche' ? 'Consigne' : 'Précision (facultative)'}</span>
          <textarea
            rows="4"
            bind:value={consigne}
            placeholder={mode === 'retouche'
              ? reecrire
                ? 'En français : donne-lui une veste en cuir rouge'
                : 'Telle qu’elle partira au modèle, en anglais : give her a red leather jacket'
              : 'Ajoute la mer à droite'}
          ></textarea>
        </label>
        {#if ouvrirScenario}
          <div class="ligne">
            <Bouton petit onclick={ouvrirScenario}>✍️ L'écrire au Scénario</Bouton>
          </div>
        {/if}
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
            <span class="etiquette">Fondu <span class="code">{fondu} px</span></span>
            <input type="range" min="0" max={FONDU_MAX} bind:value={fondu} />
          </label>
          {#if mode === 'retouche'}
            <label>
              <span class="etiquette">Format</span>
              <select bind:value={format} disabled={peint}>
                <option value={FORMAT_IMAGE}>Celui de l'image</option>
                {#each options.formats as f (f)}
                  <option value={f}>{f}</option>
                {/each}
              </select>
            </label>
          {/if}
          <label>
            <span class="etiquette">Seed</span>
            <input type="text" inputmode="numeric" placeholder="au hasard" bind:value={seedTexte} class:invalide={seed === undefined} />
          </label>
          <label>
            <span class="etiquette">Variantes</span>
            <input type="number" min="1" max={options.nombreMax} bind:value={nombre} />
          </label>
        </div>
        <RangerDans genre="images" cle="aria.destination.retouche" vide="À côté de la source" bind:dossier />
        {#if workflow && workflow.modeles.length === 0}
          <p class="discret">
            Les steps de ce checkpoint attendent d'être validés : fais-le depuis le Plateau ou Modèles → Ma collection.
          </p>
        {/if}
      </div>
    </div>

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

<Panneau titre="Résultat">
  {#if resultats.length === 0}
    <p class="discret">Rien à comparer pour l'instant : le résultat d'une retouche ou d'un agrandissement apparaît ici, avec l'original.</p>
  {:else}
    <ul class="file">
      {#each resultats as c (c.id)}
        <li class:erreur={c.etat === 'erreur'}>
          <div class="texte">
            <span class="prompt-court">{c.type === 'agrandissement' ? 'Agrandissement' : 'Retouche'} · {texteCommande(c) || 'sans précision'}</span>
            <span class="discret">{texteAvancement(c)}</span>
            {#if c.etat === 'erreur' && c.erreur}
              <ExpliquerErreur erreur={c.erreur} origine={'retouche'} commande={c.id} contexte={c.etape} />
            {/if}
            {#if c.note}
              <span class="note-reecriture">⚠ {c.note}</span>
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
  {/if}

  {#if dernierResultat && dernierResultat.parent && resultatId}
    {@const parent = dernierResultat.parent}
    <h4 class="sous-titre-panneau">Avant · après</h4>
    {#if cote}
      <div class="comparaison">
        <figure>
          {#if vignettes[parent]}
            <img src={vignettes[parent]} alt="Original" />
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
    {:else}
      <div
        class="compare"
        style:aspect-ratio="{resultatFiche?.largeur ?? 16} / {resultatFiche?.hauteur ?? 10}"
        style:--pos="{curseur}%"
        style:--reste="{100 - curseur}%"
      >
        {#if vignettes[resultatId]}
          <img class="apres" src={vignettes[resultatId]} alt="Résultat" />
        {/if}
        {#if vignettes[parent]}
          <img class="avant" src={vignettes[parent]} alt="Original" />
        {/if}
        <span class="lib g">Avant</span>
        <span class="lib d">Après</span>
        <div class="poignee"></div>
        <input type="range" min="0" max="100" bind:value={curseur} aria-label="Comparer avant et après" />
      </div>
    {/if}
    <div class="actions-gauche">
      <Bouton genre="discret" petit onclick={() => (cote = !cote)}>{cote ? 'Passer au curseur' : 'Passer en côte à côte'}</Bouton>
      <Bouton petit desactive={!resultatFiche} onclick={repartir}>↻ Repartir de ce résultat</Bouton>
    </div>
    {#if dernierResultat.images.length > 1}
      <div class="variantes" role="group" aria-label="Variantes du résultat">
        {#each dernierResultat.images as id (id)}
          <button type="button" class="variante" class:choisi={id === resultatId} aria-pressed={id === resultatId} onclick={() => (choisiResultat = id)}>
            <VignetteCreation {id} titre="Variante" />
          </button>
        {/each}
      </div>
    {/if}
  {/if}
</Panneau>

<PanneauRushes {signaler} {rafraichir} />

{#if choixRush}
  <Dialogue titre="Quel rush choisir ?" fermer={() => (choixRush = false)}>
    <ChoixRushes max={1} bind:selection={choisie} {signaler} apresChoix={() => void prendreChoisi()} />
    {#snippet pied()}
      <label class="bouton-fichier">
        Une image de mon disque…
        <input type="file" accept="image/png,image/jpeg,image/webp" onchange={importer} />
      </label>
      <Bouton genre="discret" onclick={() => (choixRush = false)}>✕ Fermer</Bouton>
    {/snippet}
  </Dialogue>
{/if}

<style>
  .cartes {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 12px;
    max-width: 560px;
  }

  .deux {
    display: grid;
    grid-template-columns: minmax(0, 1.4fr) minmax(0, 1fr);
    gap: 18px;
    align-items: start;
  }

  .gauche,
  .droite {
    display: flex;
    flex-direction: column;
    gap: 10px;
    min-width: 0;
  }

  .ligne {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px 16px;
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
    color: var(--texte-2);
    cursor: pointer;
  }

  .bouton-fichier input {
    position: absolute;
    inset: 0;
    opacity: 0;
    cursor: pointer;
  }

  .pastilles {
    display: flex;
    gap: 8px;
  }

  .pastilles button,
  .ancrages button {
    font: inherit;
    font-size: 13px;
    color: var(--texte);
    background: var(--fond);
    border: 1px solid var(--ligne);
    padding: 6px 14px;
    cursor: pointer;
  }

  .pastilles button:hover,
  .ancrages button:hover {
    border-color: var(--cyan);
  }

  .pastilles button.pris,
  .ancrages button.pris {
    color: var(--cyan);
    border-color: var(--cyan);
    background: color-mix(in srgb, var(--cyan) 10%, var(--fond));
  }

  .ancrages {
    display: grid;
    grid-template-columns: repeat(3, 40px);
    gap: 4px;
  }

  .ancrages button {
    padding: 6px 0;
  }

  .titre-champ {
    margin: 4px 0 0;
  }

  .atelier {
    position: relative;
    max-width: 100%;
    border: 1px solid var(--ligne);
    background: var(--fond);
  }

  .atelier-vide {
    display: grid;
    gap: 12px;
    justify-items: start;
    align-content: center;
    min-height: 220px;
    padding: 18px;
    border: 1px dashed var(--ligne);
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

  .grille {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(130px, 1fr));
    gap: 12px 16px;
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

  .invalide {
    border-color: var(--rouge);
  }

  .erreur {
    color: var(--rouge);
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    margin-top: 14px;
  }

  .actions-gauche {
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
    margin-top: 12px;
  }

  .file {
    display: flex;
    flex-direction: column;
    gap: 10px;
    margin: 0 0 14px;
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

  .sous-titre-panneau {
    margin: 0 0 8px;
    color: var(--texte-2);
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.14em;
    text-transform: uppercase;
  }

  /* Avant · après au curseur : l'image d'avant est coupée à gauche de la poignée. */
  .compare {
    position: relative;
    max-width: 760px;
    overflow: hidden;
    border: 1px solid var(--ligne);
    background: var(--fond);
    user-select: none;
  }

  .compare img {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: contain;
  }

  .compare .avant {
    clip-path: inset(0 var(--reste, 50%) 0 0);
  }

  .compare .poignee {
    position: absolute;
    top: 0;
    bottom: 0;
    left: var(--pos, 50%);
    width: 2px;
    background: var(--cyan);
  }

  .compare .poignee::after {
    content: '⇆';
    position: absolute;
    top: 50%;
    left: -13px;
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    margin-top: -14px;
    color: var(--cyan);
    background: var(--fond);
    border: 1px solid var(--cyan);
  }

  .compare .lib {
    position: absolute;
    top: 8px;
    padding: 1px 8px;
    font-size: 10.5px;
    letter-spacing: 0.12em;
    text-transform: uppercase;
    background: color-mix(in srgb, var(--fond) 80%, transparent);
  }

  .compare .lib.g {
    left: 8px;
  }

  .compare .lib.d {
    right: 8px;
  }

  .compare input[type='range'] {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    padding: 0;
    border: 0;
    opacity: 0;
    cursor: ew-resize;
  }

  .compare input[type='range']:focus-visible {
    opacity: 0.001;
    outline: 2px solid var(--ambre);
    outline-offset: -2px;
  }

  .variantes {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin-top: 12px;
  }

  .variante {
    padding: 2px;
    background: transparent;
    border: 1px solid transparent;
    cursor: pointer;
  }

  .variante.choisi {
    border-color: var(--cyan);
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

  .note-reecriture {
    color: var(--orange);
    font-size: 12px;
  }

  @media (max-width: 900px) {
    .deux {
      grid-template-columns: 1fr;
    }
  }
</style>
