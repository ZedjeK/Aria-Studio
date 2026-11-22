<!--
  Les Rushes du Plateau et les Bobines du Montage, comme en v2 (mêmes gestes, dossiers à part) : onglets (Tous, Non
  rangés, dossiers, Archives, Corbeille), sélection au clic, actions sur la sélection, dossiers créés, renommés ou
  supprimés. Rien n'est effacé pour de bon sauf « Vider la corbeille », confirmé dans un dialogue. Dans les Bobines,
  un clip choisi seul se regarde. `rafraichir` change quand le Plateau a enregistré de nouvelles créations.
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import {
    agirRushes,
    creerDossier,
    etatRushes,
    imageCreation,
    lireRushes,
    messageErreur,
    renommerDossier,
    supprimerDossier,
    viderCorbeille,
    type ActionRushes,
    type Creation,
    type EtatRushes,
    type GenreRushes,
  } from '../lib/api';
  import { accord, actions, basculer, compteRendu, images, libelleAction, ongletDe, onglets } from '../lib/rushes';
  import Bouton from '../lib/composants/Bouton.svelte';
  import Dialogue from '../lib/composants/Dialogue.svelte';
  import Panneau from '../lib/composants/Panneau.svelte';
  import VideoCreation from '../lib/composants/VideoCreation.svelte';

  interface Props {
    signaler: (texte: string, erreur?: boolean) => void;
    rafraichir: number;
    /** `images` : les Rushes du Plateau ; `clips` : les Bobines du Montage. */
    genre?: GenreRushes;
  }

  let { signaler, rafraichir, genre = 'images' }: Props = $props();

  const bobines = $derived(genre === 'clips');
  /** Le clip regardé (Bobines). */
  let regarde = $state<string | null>(null);

  let etat = $state<EtatRushes | null>(null);
  let rushes = $state<Creation[]>([]);
  let vignettes = $state<Record<string, string>>({});
  let cle = $state('tous');
  let selection = $state<string[]>([]);
  let rangerDans = $state('');
  let nouveauDossier = $state('');
  let nouveauNom = $state('');
  let confirmerVidage = $state(false);
  let occupe = $state(false);

  const liste = $derived(etat ? onglets(etat) : []);
  const onglet = $derived(ongletDe(liste, cle));
  const proposees = $derived(actions(onglet.vue));

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
      if (regarde && !visibles.has(regarde)) regarde = null;
      await Promise.all(rushes.map((r) => chargerVignette(r.id)));
    } catch (e) {
      signaler(messageErreur(e), true);
    }
  }

  function choisirOnglet(nouvelle: string) {
    cle = nouvelle;
    selection = [];
    nouveauNom = ongletDe(liste, nouvelle).dossier ?? '';
    void relire();
  }

  async function agir(action: ActionRushes) {
    const dossier = action === 'ranger' ? rangerDans || null : null;
    occupe = true;
    try {
      const nombre = await agirRushes(selection, action, dossier);
      signaler(compteRendu(action, nombre, dossier, genre));
      selection = [];
      await relire();
    } catch (e) {
      signaler(messageErreur(e), true);
    } finally {
      occupe = false;
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

  // Le Plateau a enregistré de nouvelles images.
  $effect(() => {
    if (rafraichir > 0) void relire();
  });

  onMount(() => {
    void relire();
    return () => {
      for (const adresse of Object.values(vignettes)) URL.revokeObjectURL(adresse);
    };
  });
</script>

<Panneau titre={bobines ? 'Bobines' : 'Rushes'}>
  <div class="onglets" role="tablist" aria-label={bobines ? 'Rangement des clips' : 'Rangement des rushes'}>
    {#each liste as o (o.cle)}
      <button type="button" role="tab" class="onglet" class:actif={o.cle === cle} aria-selected={o.cle === cle} onclick={() => choisirOnglet(o.cle)}>
        {o.titre}
      </button>
    {/each}
  </div>

  <div class="barre">
    {#if selection.length > 0}
      <span class="discret">{images(selection.length, genre)} sélectionné{accord(selection.length, genre)}</span>
      {#if bobines && selection.length === 1}
        <Bouton petit onclick={() => (regarde = regarde === selection[0] ? null : (selection[0] ?? null))}>
          {regarde === selection[0] ? 'Masquer' : 'Regarder'}
        </Bouton>
      {/if}
      {#each proposees as action (action)}
        {#if action === 'ranger'}
          <select bind:value={rangerDans} aria-label="Dossier où ranger">
            <option value="">Non rangés</option>
            {#each etat?.dossiers ?? [] as d (d.nom)}
              <option value={d.nom}>{d.nom}</option>
            {/each}
          </select>
        {/if}
        <Bouton petit desactive={occupe} onclick={() => void agir(action)}>{libelleAction(action)}</Bouton>
      {/each}
      <Bouton genre="discret" petit onclick={() => (selection = [])}>Désélectionner</Bouton>
    {:else if rushes.length > 0}
      <span class="discret">Clique sur des {bobines ? 'clips' : 'images'} pour les sélectionner.</span>
      <Bouton genre="discret" petit onclick={() => (selection = rushes.map((r) => r.id))}>Tout sélectionner</Bouton>
    {/if}
    {#if onglet.vue === 'corbeille' && rushes.length > 0}
      <Bouton petit desactive={occupe} onclick={() => (confirmerVidage = true)}>Vider la corbeille</Bouton>
    {/if}
  </div>

  {#if regarde}
    <VideoCreation id={regarde} />
  {/if}

  {#if rushes.length === 0}
    <p class="discret">{bobines && onglet.vue === 'tous' && !onglet.dossier ? "Aucun clip pour l'instant : silence, on tourne !" : "Rien ici pour l'instant."}</p>
  {:else}
    <div class="galerie">
      {#each rushes as rush (rush.id)}
        <button
          type="button"
          class="rush"
          class:choisi={selection.includes(rush.id)}
          aria-pressed={selection.includes(rush.id)}
          onclick={() => (selection = basculer(selection, rush.id))}
        >
          {#if vignettes[rush.id]}
            <img src={vignettes[rush.id]} alt={rush.prompt ?? ''} width={rush.largeur} height={rush.hauteur} />
          {:else}
            <span class="vide" aria-hidden="true"></span>
          {/if}
          <span class="legende">
            <span class="prompt-court">{rush.idee || rush.prompt || 'Sans prompt'}</span>
            <span class="discret code">{rush.largeur}×{rush.hauteur}{rush.seed != null ? ` · seed ${rush.seed}` : ''}</span>
          </span>
        </button>
      {/each}
    </div>
  {/if}

  <div class="dossiers">
    <label>
      Nouveau dossier
      <input type="text" maxlength="40" bind:value={nouveauDossier} />
    </label>
    <Bouton petit desactive={nouveauDossier.trim() === ''} onclick={() => void creer()}>Créer</Bouton>
    {#if onglet.dossier}
      <label>
        Renommer « {onglet.dossier} »
        <input type="text" maxlength="40" bind:value={nouveauNom} />
      </label>
      <Bouton petit desactive={nouveauNom.trim() === '' || nouveauNom.trim() === onglet.dossier} onclick={() => void renommer()}>
        Renommer
      </Bouton>
      <Bouton genre="discret" petit onclick={() => void supprimer()}>Supprimer le dossier</Bouton>
    {/if}
  </div>
</Panneau>

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
  .onglets {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-bottom: 12px;
  }

  .onglet {
    font: inherit;
    font-size: 13px;
    color: var(--texte-2);
    background: transparent;
    border: 1px solid var(--ligne);
    padding: 4px 10px;
    cursor: pointer;
  }

  .onglet.actif {
    color: var(--cyan);
    border-color: var(--cyan);
  }

  .barre,
  .dossiers {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 12px;
    min-height: 32px;
    margin-bottom: 12px;
  }

  .dossiers {
    margin: 16px 0 0;
  }

  .dossiers label {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--texte-2);
    font-size: 13px;
  }

  .galerie {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(170px, 1fr));
    gap: 14px;
  }

  .rush {
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-width: 0;
    padding: 4px;
    font: inherit;
    color: var(--texte);
    text-align: left;
    background: transparent;
    border: 1px solid transparent;
    cursor: pointer;
  }

  .rush.choisi {
    border-color: var(--orange);
  }

  img,
  .vide {
    display: block;
    width: 100%;
    height: auto;
    aspect-ratio: 1;
    object-fit: cover;
    border: 1px solid var(--ligne);
    background: var(--fond);
  }

  .legende {
    display: flex;
    flex-direction: column;
    gap: 2px;
    font-size: 12px;
  }

  .prompt-court {
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
</style>
