<!--
  Les Rushes du Plateau, comme en v2 : onglets (Tous, Non rangés, dossiers, Archives, Corbeille), sélection au clic,
  actions sur la sélection, dossiers créés, renommés ou supprimés. Rien n'est effacé pour de bon sauf « Vider la
  corbeille », confirmé dans un dialogue. `rafraichir` change quand le Plateau a enregistré de nouvelles images.
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
  } from '../lib/api';
  import { actions, basculer, compteRendu, images, libelleAction, ongletDe, onglets } from '../lib/rushes';
  import Bouton from '../lib/composants/Bouton.svelte';
  import Dialogue from '../lib/composants/Dialogue.svelte';
  import Panneau from '../lib/composants/Panneau.svelte';

  interface Props {
    signaler: (texte: string, erreur?: boolean) => void;
    rafraichir: number;
  }

  let { signaler, rafraichir }: Props = $props();

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
      etat = await etatRushes();
      const actif = ongletDe(onglets(etat), cle);
      cle = actif.cle;
      rushes = await lireRushes(actif.vue, actif.dossier);
      const visibles = new Set(rushes.map((r) => r.id));
      selection = selection.filter((id) => visibles.has(id));
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
      signaler(compteRendu(action, nombre, dossier));
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
      const nombre = await viderCorbeille();
      signaler(`${images(nombre)} supprimée${nombre > 1 ? 's' : ''} définitivement.`);
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
      const nom = await creerDossier(nouveauDossier);
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
      const nom = await renommerDossier(ancien, nouveauNom);
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
      const liberes = await supprimerDossier(nom);
      cle = 'tous';
      signaler(`Dossier « ${nom} » supprimé : ${images(liberes)} retournée${liberes > 1 ? 's' : ''} dans Non rangés.`);
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

<Panneau titre="Rushes">
  <div class="onglets" role="tablist" aria-label="Rangement des rushes">
    {#each liste as o (o.cle)}
      <button type="button" role="tab" class="onglet" class:actif={o.cle === cle} aria-selected={o.cle === cle} onclick={() => choisirOnglet(o.cle)}>
        {o.titre}
      </button>
    {/each}
  </div>

  <div class="barre">
    {#if selection.length > 0}
      <span class="discret">{images(selection.length)} sélectionnée{selection.length > 1 ? 's' : ''}</span>
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
      <span class="discret">Clique sur des images pour les sélectionner.</span>
      <Bouton genre="discret" petit onclick={() => (selection = rushes.map((r) => r.id))}>Tout sélectionner</Bouton>
    {/if}
    {#if onglet.vue === 'corbeille' && rushes.length > 0}
      <Bouton petit desactive={occupe} onclick={() => (confirmerVidage = true)}>Vider la corbeille</Bouton>
    {/if}
  </div>

  {#if rushes.length === 0}
    <p class="discret">Rien ici pour l'instant.</p>
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
      {images(etat?.corbeille ?? 0)} de la corbeille {(etat?.corbeille ?? 0) > 1 ? 'vont être supprimées' : 'va être supprimée'}
      pour de bon : image et fiche. Elles ne pourront pas être récupérées.
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
