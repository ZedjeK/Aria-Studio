<!--
  La fenêtre d'Aria Studio, sur le modèle d'Aria Studio et d'Aria Cockpit : menu latéral à gauche (marque, familles
  de pages, Quitter, version), et à droite la zone des pages sur son décor (nuit profonde, grille fine, halos, fond
  animé) : barre de titre avec le chemin et l'état en direct, bandeau de mise à jour, la page, puis la ligne d'état.
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import {
    ecouter,
    enregistrerReglages,
    identite as lireIdentite,
    lireReglages,
    messageErreur,
    miseAJourDisponible,
    quitter,
    type Identite,
    type InfosMiseAJour,
    type Reglages,
  } from './lib/api';
  import type { Pastille } from './lib/menu';
  import Bandeau from './lib/composants/Bandeau.svelte';
  import BarreTitre from './lib/composants/BarreTitre.svelte';
  import Bouton from './lib/composants/Bouton.svelte';
  import EtatDirect from './lib/composants/EtatDirect.svelte';
  import FondAnime from './lib/composants/FondAnime.svelte';
  import MenuLateral from './lib/composants/MenuLateral.svelte';
  import DialogueMiseAJour from './dialogues/MiseAJour.svelte';
  import Accueil from './pages/Accueil.svelte';
  import Journal from './pages/Journal.svelte';
  import Modeles from './pages/Modeles.svelte';
  import Parametres from './pages/Parametres.svelte';
  import Plateau from './pages/Plateau.svelte';
  import Retouche from './pages/Retouche.svelte';
  import PageReglages from './pages/Reglages.svelte';

  type Page = 'accueil' | 'plateau' | 'retouche' | 'modeles' | 'reglages' | 'journal' | 'parametres';

  // Les familles de pages : le travail de l'appli d'abord, puis l'appli elle-même. À adapter.
  const familles: { titre: string; pages: { id: Page; titre: string }[] }[] = [
    {
      titre: 'Travail',
      pages: [
        { id: 'accueil', titre: 'Accueil' },
        { id: 'plateau', titre: 'Plateau' },
        { id: 'retouche', titre: 'Retouche' },
      ],
    },
    {
      titre: 'Préparation',
      pages: [
        { id: 'modeles', titre: 'Modèles' },
        { id: 'reglages', titre: 'Réglages' },
      ],
    },
    {
      titre: 'Appli',
      pages: [
        { id: 'journal', titre: 'Journal' },
        { id: 'parametres', titre: 'Paramètres' },
      ],
    },
  ];

  let identite = $state<Identite | null>(null);
  let reglages = $state<Reglages | null>(null);
  let page = $state<Page>('accueil');
  let fraction = $state<number | null>(null);
  let mesure = $state('');
  let miseAJour = $state<InfosMiseAJour | null>(null);
  let bandeauRange = $state(false);
  let dialogueMiseAJour = $state(false);
  let etat = $state('');
  let erreur = $state(false);

  const nomCourt = $derived(identite?.nom.replace(/^Aria\s+/, '') ?? '');
  const famille = $derived(familles.find((f) => f.pages.some((p) => p.id === page)));
  const titrePage = $derived(famille?.pages.find((p) => p.id === page)?.titre ?? '');
  const pastilles: Pastille[] = $derived([
    {
      nom: 'Disque',
      detail: fraction === null ? 'à tester' : `${Math.round(fraction * 100)} % libre`,
      ton: fraction === null ? 'repos' : fraction < 0.1 ? 'attention' : 'actif',
    },
    {
      nom: 'Mises à jour',
      detail: miseAJour
        ? `${miseAJour.version} disponible`
        : reglages?.chercherMisesAJour === false
          ? 'désactivées'
          : 'aucune connue',
      ton: miseAJour ? 'information' : 'repos',
    },
  ]);

  function signaler(texte: string, estErreur = false) {
    etat = texte;
    erreur = estErreur;
  }

  onMount(() => {
    void (async () => {
      try {
        [identite, reglages] = await Promise.all([lireIdentite(), lireReglages()]);
        miseAJour ??= await miseAJourDisponible();
      } catch (e) {
        signaler(`Démarrage incomplet : ${messageErreur(e)}`, true);
      }
    })();
    // « Paramètres » et « Voir la mise à jour » dans le menu de l'icône.
    const arretAllerA = ecouter<string>('aller-a', (cible) => {
      if (cible === 'parametres') page = 'parametres';
      if (cible === 'mise-a-jour' && miseAJour) dialogueMiseAJour = true;
    });
    // Une version plus récente trouvée au lancement (la recherche ne bloque pas l'appli).
    const arretMiseAJour = ecouter<string>('mise-a-jour', () => {
      void miseAJourDisponible().then((infos) => (miseAJour = infos));
    });
    return () => {
      void arretAllerA.then((f) => f());
      void arretMiseAJour.then((f) => f());
    };
  });

  async function enregistrer() {
    if (!reglages) return;
    try {
      reglages = await enregistrerReglages($state.snapshot(reglages));
      signaler('Réglages enregistrés.');
    } catch (e) {
      signaler(messageErreur(e), true);
    }
  }
</script>

<div class="fenetre">
  <MenuLateral
    nom={nomCourt}
    version={identite?.version ?? ''}
    {familles}
    bind:page
    quitter={identite?.enFond ? () => void quitter() : undefined}
  />

  <main class="zone">
    <!-- Décor : nuit profonde, grille fine, halo bleu au centre, halo orange en bas à droite. -->
    <div class="decor" aria-hidden="true"></div>
    {#if reglages}
      <FondAnime reglages={reglages.fond} />
    {/if}

    <BarreTitre>
      <span class="chemin" data-tauri-drag-region>{famille?.titre ?? ''} <b>/ {titrePage}</b></span>
      <EtatDirect {pastilles} />
    </BarreTitre>

    {#if miseAJour && !bandeauRange}
      <Bandeau
        etiquette="Mise à jour disponible"
        texte={`La version ${miseAJour.version} est publiée (en cours : ${miseAJour.actuelle}).`}
      >
        <Bouton genre="discret" petit onclick={() => (bandeauRange = true)}>Plus tard</Bouton>
        <Bouton petit onclick={() => (dialogueMiseAJour = true)}>Voir</Bouton>
      </Bandeau>
    {/if}

    <div class="page">
      {#if page === 'accueil'}
        <Accueil {identite} bind:fraction bind:mesure {signaler} />
      {:else if page === 'plateau'}
        <Plateau {signaler} />
      {:else if page === 'retouche'}
        <Retouche {signaler} />
      {:else if page === 'modeles'}
        <Modeles {signaler} />
      {:else if page === 'reglages'}
        <PageReglages {signaler} />
      {:else if page === 'journal'}
        <Journal {signaler} />
      {:else if identite && reglages}
        <Parametres {identite} bind:reglages {enregistrer} {signaler} />
      {/if}

      <p class="etat" class:erreur role="status">{etat}</p>
    </div>
  </main>
</div>

{#if dialogueMiseAJour && miseAJour}
  <DialogueMiseAJour infos={miseAJour} fermer={() => (dialogueMiseAJour = false)} />
{/if}

<style>
  .fenetre {
    display: grid;
    grid-template-columns: 220px minmax(0, 1fr);
    height: 100%;
  }

  .zone {
    position: relative;
    display: flex;
    flex-direction: column;
    min-width: 0;
    overflow: hidden;
    background: linear-gradient(180deg, var(--fond) 0%, #061026 55%, var(--fond) 100%);
  }

  .decor {
    position: absolute;
    inset: 0;
    pointer-events: none;
    background:
      radial-gradient(ellipse 550px 450px at 50% 50%, rgb(92 139 255 / 18%), transparent 70%),
      radial-gradient(ellipse 450px 350px at 100% 100%, rgb(255 138 31 / 9%), transparent 70%),
      linear-gradient(rgb(125 249 255 / 6%) 1px, transparent 1px) 0 0 / 40px 40px,
      linear-gradient(90deg, rgb(125 249 255 / 6%) 1px, transparent 1px) 0 0 / 40px 40px;
  }

  .chemin {
    flex: 1;
    color: var(--texte-2);
    font-size: 11px;
    font-weight: 500;
    letter-spacing: 0.16em;
    text-transform: uppercase;
  }

  .chemin b {
    color: var(--cyan);
    font-weight: 500;
  }

  .page {
    position: relative;
    z-index: 1;
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: 16px;
    width: 100%;
    max-width: 1100px;
    margin: 0 auto;
    overflow-y: auto;
    padding: 18px 28px 28px;
  }

  .page :global(h1) {
    margin: 0;
    font-size: 26px;
    font-weight: 600;
    letter-spacing: 0.02em;
  }

  .page :global(.sous-titre) {
    margin: -10px 0 0;
    color: var(--texte-2);
  }

  .page :global(.discret) {
    color: var(--texte-2);
  }

  .etat {
    min-height: 1.5em;
    margin: auto 0 0;
    color: var(--texte-2);
  }

  .etat.erreur {
    color: var(--rouge);
  }
</style>
