<!--
  Les Rushes du Plateau et les Bobines du Montage, comme en v2 (mêmes gestes, dossiers à part) : un volet à gauche
  (Vues, puis Dossiers, chacun son nombre), la galerie à droite. Un clic sur une vignette ouvre sa fiche ; « Sélectionner »
  passe en sélection (barre : Tout, Aucun, Ranger dans, Archiver, Corbeille, Terminer) ; un clic droit ouvre le menu de la
  vignette ; glisser une vignette sur un dossier l'y range. Rien n'est effacé pour de bon sauf « Vider la corbeille »,
  confirmé dans un dialogue. Une création choisie dans la fiche se vérifie (le LLM qui voit la compare à l'intention) ;
  un lot de 2 à 8 se compare pour en retenir les meilleures (badge ★). Dans les Bobines, la fiche d'un clip le lit.
  `rafraichir` change quand une page a enregistré de nouvelles créations.
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import {
    agirRushes,
    choisirMeilleures,
    creerDossier,
    etatRushes,
    imageCreation,
    importerImage,
    lireRushes,
    messageErreur,
    renommerDossier,
    supprimerDossier,
    viderCorbeille,
    type ActionRushes,
    type Creation,
    type ActionClip,
    type EtatRushes,
    type GenreRushes,
  } from '../lib/api';
  import {
    accord,
    actions,
    basculer,
    compteRendu,
    images,
    libelleAction,
    ongletDe,
    onglets,
    scinderTitre,
  } from '../lib/rushes';
  import { badgeMeilleure, badgeVerification, peutChoisir, texteChoix } from '../lib/vision';
  import AnnulerEcriture from '../lib/composants/AnnulerEcriture.svelte';
  import Bouton from '../lib/composants/Bouton.svelte';
  import Dialogue from '../lib/composants/Dialogue.svelte';
  import Panneau from '../lib/composants/Panneau.svelte';
  import VerificationCreation from '../lib/composants/VerificationCreation.svelte';
  import VideoCreation from '../lib/composants/VideoCreation.svelte';

  interface Props {
    signaler: (texte: string, erreur?: boolean) => void;
    rafraichir: number;
    /** `images` : les Rushes du Plateau ; `clips` : les Bobines du Montage. */
    genre?: GenreRushes;
    /** Bobines : Variante, Réutiliser le prompt, ou Ouvrir la séquence d'un clip. */
    agirClip?: (clip: Creation, action: ActionClip) => void;
    /** Une commande vient d'entrer dans la file (relancée avec un prompt corrigé). */
    enFile?: () => void;
  }

  let { signaler, rafraichir, genre = 'images', agirClip, enFile }: Props = $props();

  /** Le LLM qui voit compare la sélection (Choisir les meilleures). */
  let compare = $state(false);

  const bobines = $derived(genre === 'clips');

  let etat = $state<EtatRushes | null>(null);
  let rushes = $state<Creation[]>([]);
  let vignettes = $state<Record<string, string>>({});
  let cle = $state('tous');
  let selection = $state<string[]>([]);
  /** Le mode sélection : un clic coche la vignette au lieu d'ouvrir sa fiche. */
  let selectionne = $state(false);
  let rangerDans = $state('');
  let nouveauDossier = $state('');
  let creation = $state(false);
  let nouveauNom = $state('');
  let renommage = $state(false);
  let confirmerVidage = $state(false);
  let occupe = $state(false);
  /** La fiche ouverte, et le menu du clic droit. */
  let fiche = $state<string | null>(null);
  let menu = $state<{ x: number; y: number; id: string } | null>(null);
  let survole = $state<string | null>(null);

  const liste = $derived(etat ? onglets(etat) : []);
  const onglet = $derived(ongletDe(liste, cle));
  const proposees = $derived(actions(onglet.vue));
  const vues = $derived(liste.filter((o) => o.dossier === null));
  const dossiers = $derived(liste.filter((o) => o.dossier !== null));
  const ficheRush = $derived(fiche ? rushes.find((r) => r.id === fiche) : undefined);
  const total = $derived(etat?.tous ?? 0);

  async function chargerVignette(id: string) {
    if (vignettes[id]) return;
    try {
      const octets = await imageCreation(id);
      vignettes[id] = URL.createObjectURL(new Blob([octets], { type: 'image/png' }));
    } catch {
      // Une image effacée entre-temps : la fiche reste, sans vignette.
    }
  }

  async function relire() {
    try {
      etat = await etatRushes(genre);
      const actif = ongletDe(onglets(etat), cle);
      cle = actif.cle;
      rushes = await lireRushes(actif.vue, actif.dossier, genre);
      const visibles = new Set(rushes.map((r) => r.id));
      selection = selection.filter((id) => visibles.has(id));
      if (fiche && !visibles.has(fiche)) fiche = null;
      await Promise.all(rushes.map((r) => chargerVignette(r.id)));
    } catch (e) {
      signaler(messageErreur(e), true);
    }
  }

  function choisirOnglet(nouvelle: string) {
    cle = nouvelle;
    selection = [];
    renommage = false;
    nouveauNom = ongletDe(liste, nouvelle).dossier ?? '';
    void relire();
  }

  function terminer() {
    selectionne = false;
    selection = [];
  }

  async function agirSur(ids: string[], action: ActionRushes, dossier: string | null) {
    occupe = true;
    try {
      const nombre = await agirRushes(ids, action, dossier);
      signaler(compteRendu(action, nombre, dossier, genre));
      selection = selection.filter((id) => !ids.includes(id));
      await relire();
    } catch (e) {
      signaler(messageErreur(e), true);
    } finally {
      occupe = false;
    }
  }

  async function agir(action: ActionRushes) {
    await agirSur(selection, action, action === 'ranger' ? rangerDans || null : null);
  }

  async function choisir() {
    compare = true;
    try {
      signaler(texteChoix(await choisirMeilleures(selection)));
      await relire();
    } catch (e) {
      signaler(messageErreur(e), true);
    } finally {
      compare = false;
    }
  }

  async function vider() {
    occupe = true;
    try {
      const nombre = await viderCorbeille(genre);
      signaler(`${images(nombre, genre)} supprimé${accord(nombre, genre)} définitivement.`);
      confirmerVidage = false;
      await relire();
    } catch (e) {
      signaler(messageErreur(e), true);
    } finally {
      occupe = false;
    }
  }

  async function creer() {
    try {
      const nom = await creerDossier(nouveauDossier, genre);
      nouveauDossier = '';
      creation = false;
      signaler(`Dossier « ${nom} » créé.`);
      await relire();
    } catch (e) {
      signaler(messageErreur(e), true);
    }
  }

  async function renommer() {
    const ancien = onglet.dossier;
    if (!ancien) return;
    try {
      const nom = await renommerDossier(ancien, nouveauNom, genre);
      cle = `dossier:${nom}`;
      renommage = false;
      signaler(`Dossier renommé en « ${nom} ».`);
      await relire();
    } catch (e) {
      signaler(messageErreur(e), true);
    }
  }

  async function supprimer() {
    const nom = onglet.dossier;
    if (!nom) return;
    try {
      const liberes = await supprimerDossier(nom, genre);
      cle = 'tous';
      signaler(
        `Dossier « ${nom} » supprimé : ${images(liberes, genre)} retourné${accord(liberes, genre)} dans Non rangés.`,
      );
      await relire();
    } catch (e) {
      signaler(messageErreur(e), true);
    }
  }

  /** Importer une image (Rushes seulement) : une image du disque devient un rush, rangée dans le dossier ouvert. */
  function importer(evenement: Event & { currentTarget: HTMLInputElement }) {
    const fichier = evenement.currentTarget.files?.[0];
    evenement.currentTarget.value = '';
    if (!fichier) return;
    const lecteur = new FileReader();
    lecteur.onload = () => {
      if (typeof lecteur.result !== 'string') return;
      void importerImage(lecteur.result, fichier.name, onglet.dossier)
        .then(async (nouvelle) => {
          signaler(`Image importée : ${nouvelle.largeur}×${nouvelle.hauteur}.`);
          await relire();
        })
        .catch((e: unknown) => signaler(messageErreur(e), true));
    };
    lecteur.readAsDataURL(fichier);
  }

  /** Un clic sur une vignette : la fiche, ou la coche en mode sélection. */
  function cliquer(id: string) {
    if (selectionne) selection = basculer(selection, id);
    else fiche = id;
  }

  function ouvrirMenu(e: MouseEvent, id: string) {
    e.preventDefault();
    menu = { x: Math.min(e.clientX, window.innerWidth - 250), y: Math.min(e.clientY, window.innerHeight - 280), id };
  }

  function depuisMenu(action: 'fiche' | 'selection' | ActionRushes | ActionClip) {
    const m = menu;
    menu = null;
    if (!m) return;
    if (action === 'fiche') fiche = m.id;
    else if (action === 'selection') {
      selectionne = true;
      if (!selection.includes(m.id)) selection = [...selection, m.id];
    } else if (action === 'variante' || action === 'reutiliser' || action === 'sequence') {
      const clip = rushes.find((r) => r.id === m.id);
      if (clip) agirClip?.(clip, action);
    } else void agirSur([m.id], action, null);
  }

  function glisser(e: DragEvent, id: string) {
    e.dataTransfer?.setData('text/plain', id);
  }

  function deposer(e: DragEvent, dossier: string) {
    e.preventDefault();
    survole = null;
    const id = e.dataTransfer?.getData('text/plain');
    if (!id) return;
    const ids = selection.includes(id) ? selection : [id];
    void agirSur(ids, 'ranger', dossier);
  }

  function agirDepuisFiche(clip: Creation, action: ActionClip) {
    fiche = null;
    agirClip?.(clip, action);
  }

  /** Le Plateau a enregistré de nouvelles images. */
  $effect(() => {
    if (rafraichir > 0) void relire();
  });

  onMount(() => {
    void relire();
    const fermerMenu = () => (menu = null);
    window.addEventListener('click', fermerMenu);
    return () => {
      window.removeEventListener('click', fermerMenu);
      for (const adresse of Object.values(vignettes)) URL.revokeObjectURL(adresse);
    };
  });
</script>

<Panneau titre={bobines ? 'Bobines' : 'Rushes'}>
  <div class="entete">
    <span class="discret fil">
      {scinderTitre(onglet.titre).nom} · {images(rushes.length, genre)}
    </span>
    <Bouton genre="discret" petit onclick={() => (selectionne ? terminer() : (selectionne = true))}>
      {selectionne ? 'Terminer' : 'Sélectionner'}
    </Bouton>
    {#if !bobines}
      <label class="bouton-fichier">
        Importer une image
        <input type="file" accept="image/png,image/jpeg,image/webp" onchange={importer} />
      </label>
    {/if}
  </div>

  {#if selectionne}
    <div class="barre" aria-label="Sélection">
      <b>{selection.length} {bobines ? 'choisi' : 'choisie'}{selection.length > 1 ? 's' : ''}</b>
      <Bouton genre="discret" petit onclick={() => (selection = rushes.map((r) => r.id))}>Tout</Bouton>
      <Bouton genre="discret" petit onclick={() => (selection = [])}>Aucun</Bouton>
      {#if peutChoisir(selection.length) && onglet.vue !== 'corbeille'}
        <Bouton petit desactive={compare} onclick={() => void choisir()}>★ Choisir les meilleures</Bouton>
        {#if compare}
          <span class="discret">Le LLM compare les images…</span>
          <AnnulerEcriture />
        {/if}
      {/if}
      {#each proposees as action (action)}
        {#if action === 'ranger'}
          <label class="ranger">
            📥 Ranger dans
            <select bind:value={rangerDans} aria-label="Dossier où ranger">
              <option value="">Aucun dossier</option>
              {#each etat?.dossiers ?? [] as d (d.nom)}
                <option value={d.nom}>{d.nom}</option>
              {/each}
            </select>
          </label>
        {/if}
        <Bouton genre="discret" petit desactive={occupe || selection.length === 0} onclick={() => void agir(action)}>
          {libelleAction(action)}
        </Bouton>
      {/each}
      <Bouton petit onclick={terminer}>Terminer</Bouton>
    </div>
  {/if}

  <div class="rushes">
    <nav class="volet" aria-label={bobines ? 'Vues et dossiers des bobines' : 'Vues et dossiers des rushes'}>
      <div class="titre">Vues</div>
      {#each vues as o (o.cle)}
        {@const t = scinderTitre(o.titre)}
        <button type="button" class:actif={o.cle === cle} aria-current={o.cle === cle} onclick={() => choisirOnglet(o.cle)}>
          {t.nom}<small>{t.nombre}</small>
        </button>
      {/each}
      <div class="titre">Dossiers</div>
      {#each dossiers as o (o.cle)}
        {@const t = scinderTitre(o.titre)}
        <button
          type="button"
          class:actif={o.cle === cle}
          class:cible={survole === o.cle}
          aria-current={o.cle === cle}
          onclick={() => choisirOnglet(o.cle)}
          ondragover={(e) => {
            e.preventDefault();
            survole = o.cle;
          }}
          ondragleave={() => (survole = null)}
          ondrop={(e) => deposer(e, o.dossier ?? '')}
        >
          📁 {t.nom}<small>{t.nombre}</small>
        </button>
      {/each}
      {#if creation}
        <div class="nouveau-champ">
          <input type="text" maxlength="40" placeholder="Nom du dossier" aria-label="Nom du nouveau dossier" bind:value={nouveauDossier} />
          <Bouton petit desactive={nouveauDossier.trim() === ''} onclick={() => void creer()}>Créer</Bouton>
        </div>
      {:else}
        <button type="button" class="nouveau" onclick={() => (creation = true)}>＋ Dossier</button>
      {/if}
    </nav>

    <div class="contenu">
      {#if onglet.dossier}
        <div class="fil-dossier discret">
          Dossier ouvert : <b>{onglet.dossier}</b>
          {#if renommage}
            <input type="text" maxlength="40" aria-label="Nouveau nom du dossier" bind:value={nouveauNom} />
            <Bouton petit desactive={nouveauNom.trim() === '' || nouveauNom.trim() === onglet.dossier} onclick={() => void renommer()}>
              Renommer
            </Bouton>
          {:else}
            · <button type="button" class="lien" onclick={() => ((renommage = true), (nouveauNom = onglet.dossier ?? ''))}>✎ Renommer</button>
            · <button type="button" class="lien" onclick={() => void supprimer()}>✕ Supprimer le dossier</button>
          {/if}
        </div>
      {/if}
      {#if onglet.vue === 'corbeille' && rushes.length > 0}
        <div class="fil-dossier">
          <Bouton petit desactive={occupe} onclick={() => (confirmerVidage = true)}>Vider la corbeille</Bouton>
        </div>
      {/if}

      {#if rushes.length === 0}
        <p class="discret">
          {bobines && onglet.vue === 'tous' && !onglet.dossier ? "Aucun clip pour l'instant : silence, on tourne !" : "Rien ici pour l'instant."}
        </p>
      {:else}
        <div class="galerie" class:selection={selectionne}>
          {#each rushes as rush (rush.id)}
            <button
              type="button"
              class="vignette"
              class:choisie={selection.includes(rush.id)}
              aria-pressed={selectionne ? selection.includes(rush.id) : undefined}
              title={rush.idee || rush.prompt || 'Sans prompt'}
              draggable="true"
              onclick={() => cliquer(rush.id)}
              oncontextmenu={(e) => ouvrirMenu(e, rush.id)}
              ondragstart={(e) => glisser(e, rush.id)}
            >
              {#if vignettes[rush.id]}
                <img src={vignettes[rush.id]} alt={rush.prompt ?? ''} width={rush.largeur} height={rush.hauteur} draggable="false" />
              {:else}
                <span class="vide" aria-hidden="true"></span>
              {/if}
              {#if selectionne}<span class="coche-sel" aria-hidden="true"></span>{/if}
              {#if bobines && rush.duree}<span class="duree">{rush.duree} s</span>{/if}
              {#if badgeMeilleure(rush) || badgeVerification(rush) || bobines}
                <span class="badges">
                  {#if bobines}<span class="badge">{rush.sequence ? 'Séquence' : 'Clip'}</span>{/if}
                  {#if badgeMeilleure(rush)}<span class="badge meilleure" title={rush.meilleure?.raison ?? ''}>{badgeMeilleure(rush)}</span>{/if}
                  {#if badgeVerification(rush)}<span class="badge" class:ecarts={!rush.verification?.fidele} title={rush.verification?.verdict ?? ''}>{badgeVerification(rush)}</span>{/if}
                </span>
              {/if}
            </button>
          {/each}
        </div>
      {/if}
    </div>
  </div>
</Panneau>

{#if menu}
  {@const cible = rushes.find((r) => r.id === menu?.id)}
  <div class="menu-contexte" role="menu" style:left="{menu.x}px" style:top="{menu.y}px">
    <button type="button" role="menuitem" onclick={() => depuisMenu('fiche')}>Ouvrir la fiche</button>
    {#if bobines && cible && agirClip && onglet.vue !== 'corbeille'}
      {#if cible.sequence}
        <button type="button" role="menuitem" onclick={() => depuisMenu('sequence')}>Ouvrir la séquence</button>
      {:else}
        <button type="button" role="menuitem" onclick={() => depuisMenu('variante')}>Variante</button>
        <button type="button" role="menuitem" onclick={() => depuisMenu('reutiliser')}>Réutiliser le prompt</button>
      {/if}
    {/if}
    <hr />
    <button type="button" role="menuitem" onclick={() => depuisMenu('selection')}>Sélectionner</button>
    {#each proposees.filter((a) => a !== 'ranger') as action (action)}
      <button type="button" role="menuitem" onclick={() => depuisMenu(action)}>
        {action === 'corbeille' ? '🗑 ' : ''}{libelleAction(action)}
      </button>
    {/each}
  </div>
{/if}

{#if ficheRush}
  {@const r = ficheRush}
  <Dialogue titre={`${bobines ? "Fiche d'un clip" : "Fiche d'un rush"} · ${r.id}`} fermer={() => (fiche = null)}>
    <div class="mini-fiche">
      <div class="visuel">
        {#if bobines}
          <VideoCreation id={r.id} />
        {:else if vignettes[r.id]}
          <img src={vignettes[r.id]} alt={r.prompt ?? ''} />
        {/if}
      </div>
      <dl class="kv">
        {#if r.idee}<dt>Idée</dt><dd>{r.idee}</dd>{/if}
        {#if r.prompt}<dt>Prompt</dt><dd class="petit">{r.prompt}</dd>{/if}
        {#if r.modele}<dt>Modèle</dt><dd class="code">{r.modele}</dd>{/if}
        {#if r.seed != null}<dt>Seed</dt><dd class="code">{r.seed}</dd>{/if}
        {#if r.loras && r.loras.length > 0}<dt>LoRA</dt><dd>{r.loras.map((l) => `${l.fichier.replace(/\.safetensors$/i, '')} · ${String(l.force).replace('.', ',')}`).join(', ')}</dd>{/if}
        {#if r.llm}<dt>LLM</dt><dd>{r.llm}</dd>{/if}
        <dt>Taille</dt><dd class="code">{r.largeur}×{r.hauteur}{r.duree ? ` · ${r.duree} s` : ''}</dd>
      </dl>
    </div>
    {#if onglet.vue !== 'corbeille'}
      {#key r.id}
        <VerificationCreation creation={r} {signaler} apres={() => void relire()} {enFile} />
      {/key}
    {/if}
    {#if bobines && agirClip && onglet.vue !== 'corbeille'}
      <div class="actions-fiche">
        {#if r.sequence}
          <Bouton petit onclick={() => agirDepuisFiche(r, 'sequence')}>Ouvrir la séquence</Bouton>
        {:else}
          <Bouton petit onclick={() => agirDepuisFiche(r, 'variante')}>Variante</Bouton>
          <Bouton petit onclick={() => agirDepuisFiche(r, 'reutiliser')}>Réutiliser le prompt</Bouton>
        {/if}
      </div>
    {/if}
    {#snippet pied()}
      {#if onglet.vue !== 'corbeille'}
        <Bouton
          genre="discret"
          desactive={occupe}
          onclick={() => {
            const id = r.id;
            fiche = null;
            void agirSur([id], 'corbeille', null);
          }}
        >
          🗑 Corbeille
        </Bouton>
      {/if}
      <Bouton genre="discret" onclick={() => (fiche = null)}>✕ Fermer</Bouton>
    {/snippet}
  </Dialogue>
{/if}

{#if confirmerVidage}
  <Dialogue titre="Vider la corbeille" fermer={() => !occupe && (confirmerVidage = false)}>
    <p>
      {images(etat?.corbeille ?? 0, genre)} de la corbeille {(etat?.corbeille ?? 0) > 1 ? 'vont être' : 'va être'}
      supprimé{accord(etat?.corbeille ?? 0, genre)} pour de bon, avec {bobines ? 'vidéo, vignette' : 'image'} et fiche. Rien ne
      pourra être récupéré.
    </p>
    {#snippet pied()}
      <Bouton genre="discret" desactive={occupe} onclick={() => (confirmerVidage = false)}>Annuler</Bouton>
      <Bouton genre="principal" desactive={occupe} onclick={() => void vider()}>Supprimer définitivement</Bouton>
    {/snippet}
  </Dialogue>
{/if}

<style>
  .entete,
  .barre {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 12px;
    margin-bottom: 10px;
  }

  .fil {
    flex: 1;
  }

  .bouton-fichier {
    position: relative;
    padding: 5px 14px;
    border: 1px solid var(--ligne);
    color: var(--texte-2);
    font-size: 12px;
    cursor: pointer;
  }

  .bouton-fichier input {
    position: absolute;
    inset: 0;
    opacity: 0;
    cursor: pointer;
  }

  .barre {
    padding: 8px 10px;
    border: 1px solid var(--cyan);
    background: color-mix(in srgb, var(--cyan) 8%, var(--panneau));
    font-size: 12.5px;
  }

  .ranger {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--texte-2);
    font-size: 12.5px;
  }

  .rushes {
    display: grid;
    grid-template-columns: 178px minmax(0, 1fr);
    gap: 16px;
  }

  .volet {
    display: flex;
    flex-direction: column;
    gap: 2px;
    align-content: start;
    padding-right: 10px;
    border-right: 1px solid var(--ligne);
  }

  .volet .titre {
    padding: 10px 8px 4px;
    color: var(--texte-2);
    font-size: 10.5px;
    font-weight: 600;
    letter-spacing: 0.16em;
    text-transform: uppercase;
  }

  .volet button {
    display: flex;
    justify-content: space-between;
    gap: 8px;
    width: 100%;
    padding: 6px 8px;
    border: 0;
    border-left: 3px solid transparent;
    background: transparent;
    color: var(--texte-2);
    font: 600 12px var(--police);
    letter-spacing: 0.08em;
    text-align: left;
    text-transform: uppercase;
    cursor: pointer;
  }

  .volet button small {
    font: 400 11.5px var(--police-code);
  }

  .volet button:hover {
    color: var(--texte);
    background: #0b1a30;
  }

  .volet button.actif {
    border-left-color: var(--cyan);
    color: var(--cyan);
    background: rgba(125, 249, 255, 0.07);
  }

  .volet button.cible {
    background: color-mix(in srgb, var(--orange) 14%, transparent);
  }

  .volet button.nouveau {
    margin-top: 6px;
    color: var(--bleu);
  }

  .nouveau-champ {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-top: 6px;
  }

  .contenu {
    min-width: 0;
  }

  .fil-dossier {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 10px;
    margin-bottom: 8px;
    font-size: 13px;
  }

  .lien {
    padding: 0;
    border: 0;
    background: none;
    color: var(--bleu);
    font: inherit;
    cursor: pointer;
  }

  .galerie {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(116px, 1fr));
    gap: 8px;
    align-content: start;
  }

  .vignette {
    position: relative;
    padding: 0;
    overflow: hidden;
    aspect-ratio: 1;
    border: 1px solid var(--ligne);
    background: var(--fond);
    cursor: pointer;
  }

  .vignette:hover {
    border-color: var(--texte-2);
  }

  .vignette.choisie {
    border-color: var(--cyan);
    box-shadow: inset 0 0 0 2px var(--cyan);
  }

  .vignette img,
  .vignette .vide {
    display: block;
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .coche-sel {
    position: absolute;
    top: 4px;
    left: 4px;
    width: 16px;
    height: 16px;
    border: 1px solid var(--texte);
    background: rgb(3 8 18 / 70%);
  }

  .vignette.choisie .coche-sel {
    border-color: var(--cyan);
    background: var(--cyan);
    box-shadow: inset 0 0 0 3px var(--fond);
  }

  .duree {
    position: absolute;
    top: 4px;
    right: 4px;
    padding: 0 5px;
    font: 500 10px var(--police-code);
    background: rgb(3 8 18 / 86%);
  }

  .badges {
    position: absolute;
    left: 4px;
    bottom: 4px;
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }

  .badge {
    padding: 1px 6px;
    border: 1px solid var(--cyan);
    background: rgb(3 8 18 / 86%);
    color: var(--cyan);
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }

  .badge.meilleure,
  .badge.ecarts {
    border-color: var(--orange);
    color: var(--orange);
  }

  .menu-contexte {
    position: fixed;
    z-index: 30;
    min-width: 220px;
    padding: 6px 0;
    background: var(--panneau);
    border: 1px solid var(--ligne);
    font-size: 13px;
  }

  .menu-contexte button {
    display: flex;
    width: 100%;
    padding: 6px 16px;
    border: 0;
    background: none;
    color: var(--texte);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  .menu-contexte button:hover {
    background: #0b1a30;
  }

  .menu-contexte hr {
    margin: 6px 0;
    border: 0;
    border-top: 1px solid var(--ligne);
  }

  .mini-fiche {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
    gap: 18px;
  }

  .mini-fiche .visuel {
    border: 1px solid var(--ligne);
  }

  .mini-fiche img {
    display: block;
    width: 100%;
    height: auto;
  }

  .kv {
    display: grid;
    grid-template-columns: 90px 1fr;
    gap: 6px 12px;
    margin: 0;
    font-size: 13px;
  }

  .kv dt {
    padding-top: 2px;
    color: var(--texte-2);
    font-size: 11px;
    letter-spacing: 0.1em;
    text-transform: uppercase;
  }

  .kv dd {
    margin: 0;
    overflow-wrap: anywhere;
  }

  .kv .petit {
    font-size: 12px;
  }

  .actions-fiche {
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
  }

  @media (max-width: 900px) {
    .rushes,
    .mini-fiche {
      grid-template-columns: 1fr;
    }

    .volet {
      flex-direction: row;
      flex-wrap: wrap;
      border-right: 0;
    }
  }
</style>
