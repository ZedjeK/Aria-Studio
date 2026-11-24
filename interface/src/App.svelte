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
    etatAssistant,
    etatRepriseV2,
    identite as lireIdentite,
    lireMoteurs,
    lireReglages,
    messageErreur,
    miseAJourDisponible,
    quitter,
    repriseV2Vue,
    type Creation,
    type EtatMoteur,
    type EtatReprise,
    type Identite,
    type InfosMiseAJour,
    type Reglages,
    type EnvoiClip,
    type EnvoiPlateau,
    type EnvoiRetouche,
    type Sequence,
  } from './lib/api';
  import { pastillesMoteurs, type Pastille } from './lib/menu';
  import { texteBandeau } from './lib/reprise';
  import type { VersPage } from './lib/rushes';
  import Bandeau from './lib/composants/Bandeau.svelte';
  import BarreTitre from './lib/composants/BarreTitre.svelte';
  import Bouton from './lib/composants/Bouton.svelte';
  import EtatDirect from './lib/composants/EtatDirect.svelte';
  import FondAnime from './lib/composants/FondAnime.svelte';
  import MenuLateral from './lib/composants/MenuLateral.svelte';
  import DialogueAssistant from './dialogues/Assistant.svelte';
  import DialogueMiseAJour from './dialogues/MiseAJour.svelte';
  import Casting from './pages/Casting.svelte';
  import Journal from './pages/Journal.svelte';
  import Modeles from './pages/Modeles.svelte';
  import Montage from './pages/Montage.svelte';
  import Parametres from './pages/Parametres.svelte';
  import Plateau from './pages/Plateau.svelte';
  import Retouche from './pages/Retouche.svelte';
  import Scenario from './pages/Scenario.svelte';
  import PageReglages from './pages/Reglages.svelte';

  type Page = 'scenario' | 'plateau' | 'retouche' | 'montage' | 'modeles' | 'casting' | 'reglages' | 'journal' | 'parametres';

  // Les familles de pages : ce qu'on crée, ce qu'on prépare, puis l'appli elle-même. Aria Studio s'ouvre sur le Plateau.
  const familles: { titre: string; pages: { id: Page; titre: string }[] }[] = [
    {
      titre: 'Création',
      pages: [
        { id: 'scenario', titre: 'Scénario' },
        { id: 'plateau', titre: 'Plateau' },
        { id: 'retouche', titre: 'Retouche' },
        { id: 'montage', titre: 'Montage' },
      ],
    },
    {
      titre: 'Préparation',
      pages: [
        { id: 'modeles', titre: 'Modèles' },
        { id: 'casting', titre: 'Casting' },
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
  let page = $state<Page>('plateau');
  /** L'assistant de premier lancement : ouvert tout seul tant qu'il manque de quoi générer, ou depuis Réglages. */
  let assistant = $state(false);
  /** Change quand l'assistant a fini : les pages relisent leurs listes (favoris, moteurs). */
  let apresAssistant = $state(0);
  /** Un prompt écrit au Scénario, en route vers le Plateau. */
  let versPlateau = $state<EnvoiPlateau | null>(null);
  /** Une séquence écrite au Scénario, à ouvrir au Montage ; une séquence du Montage, à retravailler au Scénario. */
  let versMontage = $state<string | null>(null);
  let versScenario = $state<Sequence | null>(null);
  /** Une consigne de retouche, ou le prompt d'un clip, écrits au Scénario. */
  let versRetouche = $state<EnvoiRetouche | null>(null);
  let versClip = $state<EnvoiClip | null>(null);
  /** Change quand un rush envoyé d'une page à l'autre doit être repris : la page d'arrivée repart de zéro avec lui. */
  let reprisePage = $state(0);

  /** Un rush de la galerie part vers une autre page : son prompt au Plateau, comme référence, animé ou retouché. */
  function depuisRush(action: VersPage, rush: Creation) {
    reprisePage += 1;
    if (action === 'reutiliser') {
      versPlateau = {
        prompt: rush.prompt || rush.idee || '',
        workflow: rush.workflow ?? null,
        references: [],
        modeReference: null,
      };
      page = 'plateau';
    } else if (action === 'reference') {
      versPlateau = { prompt: '', workflow: null, references: [rush.id], modeReference: 'inspirer' };
      page = 'plateau';
    } else if (action === 'animer') {
      versClip = { prompt: '', mode: 'animer', source: rush.id, duree: 5 };
      page = 'montage';
    } else {
      versRetouche = { source: rush.id, instruction: '', workflow: null };
      page = 'retouche';
    }
  }
  /** L'état des moteurs, relu toutes les quelques secondes pour les pastilles du haut. */
  let moteurs = $state<EtatMoteur[] | null>(null);
  let miseAJour = $state<InfosMiseAJour | null>(null);
  let bandeauRange = $state(false);
  /** La reprise d'une v2 pour un essai portable : annoncée une fois au premier lancement. */
  let reprise = $state<EtatReprise | null>(null);
  let repriseRangee = $state(false);
  const bandeauReprise = $derived(reprise && !repriseRangee ? texteBandeau(reprise) : null);
  /** L'onglet des Réglages à ouvrir d'abord, et de quoi rouvrir la page quand il change. */
  let ongletReglages = $state<string | undefined>(undefined);
  let ouvertureReglages = $state(0);
  let dialogueMiseAJour = $state(false);
  let etat = $state('');
  let erreur = $state(false);

  const nomCourt = $derived(identite?.nom.replace(/^Aria\s+/, '') ?? '');
  const famille = $derived(familles.find((f) => f.pages.some((p) => p.id === page)));
  const titrePage = $derived(famille?.pages.find((p) => p.id === page)?.titre ?? '');
  const pastilles: Pastille[] = $derived(pastillesMoteurs(moteurs));

  /** Relit l'état des moteurs ; une lecture ratée garde la précédente (la page Moteurs, elle, dit l'erreur). */
  async function relireMoteurs() {
    try {
      moteurs = (await lireMoteurs()).moteurs;
    } catch {
      // Les pastilles gardent leur dernier état.
    }
  }

  function signaler(texte: string, estErreur = false) {
    etat = texte;
    erreur = estErreur;
  }

  onMount(() => {
    void (async () => {
      try {
        [identite, reglages] = await Promise.all([lireIdentite(), lireReglages()]);
        miseAJour ??= await miseAJourDisponible();
        if ((await etatAssistant()).ouvrir) assistant = true;
        reprise = await etatRepriseV2();
      } catch (e) {
        signaler(`Démarrage incomplet : ${messageErreur(e)}`, true);
      }
    })();
    // Les pastilles ComfyUI et LLM : une lecture au lancement, puis toutes les 5 secondes tant que la fenêtre est visible.
    void relireMoteurs();
    const minuteurMoteurs = setInterval(() => {
      if (!document.hidden) void relireMoteurs();
    }, 5000);
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
      clearInterval(minuteurMoteurs);
      void arretAllerA.then((f) => f());
      void arretMiseAJour.then((f) => f());
    };
  });

  /** Le bandeau de la reprise vu : il ne revient plus (une v2 lancée, lui, revient tant qu'elle tourne). */
  function rangerReprise(voir: boolean) {
    repriseRangee = true;
    if (reprise?.bilan && !reprise.bilan.annonce) {
      void repriseV2Vue().catch((e: unknown) => signaler(messageErreur(e), true));
    }
    if (voir) {
      ongletReglages = 'lieux';
      ouvertureReglages += 1;
      page = 'reglages';
    }
  }

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

    {#if bandeauReprise}
      <Bandeau etiquette={bandeauReprise.etiquette} texte={bandeauReprise.texte}>
        <Bouton genre="discret" petit onclick={() => rangerReprise(false)}>Compris</Bouton>
        <Bouton petit onclick={() => rangerReprise(true)}>Voir dans Emplacements</Bouton>
      </Bandeau>
    {/if}

    <div class="page">
      {#if page === 'scenario'}
        <Scenario
          {signaler}
          envoyer={(prompt, workflow, references, modeReference) => {
            versPlateau = { prompt, workflow, references, modeReference };
            page = 'plateau';
          }}
          versMontage={(id) => {
            versMontage = id;
            page = 'montage';
          }}
          recuSequence={versScenario}
          recuLu={() => (versScenario = null)}
          envoyerRetouche={(envoi) => {
            versRetouche = envoi;
            page = 'retouche';
          }}
          envoyerClip={(envoi) => {
            versClip = envoi;
            page = 'montage';
          }}
        />
      {:else if page === 'plateau'}
        {#key reprisePage}
          <Plateau
            {signaler}
            recu={versPlateau}
            recuLu={() => (versPlateau = null)}
            ouvrirScenario={() => (page = 'scenario')}
            versPage={depuisRush}
          />
        {/key}
      {:else if page === 'retouche'}
        {#key reprisePage}
          <Retouche
            {signaler}
            recu={versRetouche}
            recuLu={() => (versRetouche = null)}
            ouvrirScenario={() => (page = 'scenario')}
          />
        {/key}
      {:else if page === 'montage'}
        {#key reprisePage}
        <Montage
          {signaler}
          ouvrirScenario={() => (page = 'scenario')}
          recu={versMontage}
          recuLu={() => (versMontage = null)}
          recuClip={versClip}
          recuClipLu={() => (versClip = null)}
          retravailler={(s) => {
            versScenario = s;
            page = 'scenario';
          }}
        />
        {/key}
      {:else if page === 'modeles'}
        <Modeles {signaler} />
      {:else if page === 'casting'}
        <Casting {signaler} />
      {:else if page === 'reglages'}
        {#key `${apresAssistant}-${ouvertureReglages}`}
          <PageReglages {signaler} ouvrirAssistant={() => (assistant = true)} ongletInitial={ongletReglages} />
        {/key}
      {:else if page === 'journal'}
        <Journal {signaler} />
      {:else if identite && reglages}
        <Parametres {identite} bind:reglages {enregistrer} {signaler} />
      {/if}

      <p class="etat" class:erreur role="status">{etat}</p>
    </div>
  </main>
</div>

{#if assistant}
  <DialogueAssistant
    fermer={(termine) => {
      assistant = false;
      if (termine) apresAssistant += 1;
    }}
  />
{/if}

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
