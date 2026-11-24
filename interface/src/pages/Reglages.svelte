<!--
  Réglages : la première page est celle des moteurs (comme en v2) : la carte graphique, ComfyUI et llama.cpp, leur
  installation depuis leur release GitHub officielle, leur suivi, le retour à la version précédente. Puis la sécurité
  (filtre AS-18). Les autres pages des réglages (emplacements, clés, LLM, modèles image…) viennent avec leur portage.
  Pas de bouton orange ici : plusieurs moteurs, plusieurs actions de même poids ; donc aucun panneau accentué.
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import {
    annulerInstallationMoteur,
    choisirComfyui,
    installerMoteur,
    libererCarte,
    lireMoteurs,
    messageErreur,
    ouvrirDossier,
    parcourirDossier,
    revenirMoteur,
    type EtatMoteur,
    type EtatMoteurs,
  } from '../lib/api';
  import {
    detailCarte,
    fractionTache,
    libelleStatut,
    manquants,
    mesureTache,
    peutReinstaller,
    teintePastille,
    texteCarte,
    texteConfirmation,
    texteInstaller,
  } from '../lib/moteurs';
  import ASavoir from '../lib/composants/ASavoir.svelte';
  import Bouton from '../lib/composants/Bouton.svelte';
  import ExpliquerErreur from '../lib/composants/ExpliquerErreur.svelte';
  import Dialogue from '../lib/composants/Dialogue.svelte';
  import Jauge from '../lib/composants/Jauge.svelte';
  import Panneau from '../lib/composants/Panneau.svelte';
  import SousOnglets from '../lib/composants/SousOnglets.svelte';
  import ReglagesLieux from './ReglagesLieux.svelte';
  import ReglagesLlm from './ReglagesLlm.svelte';
  import ReglagesMcp from './ReglagesMcp.svelte';
  import ReglagesSecurite from './ReglagesSecurite.svelte';
  import ReglagesWorkflows from './ReglagesWorkflows.svelte';

  interface Props {
    signaler: (texte: string, erreur?: boolean) => void;
    /** Relance l'assistant de premier lancement. */
    ouvrirAssistant?: () => void;
    /** Va à la page Modèles (« Ouvrir dans Modèles », à côté du checkpoint par défaut). */
    ouvrirModeles?: () => void;
    /** L'onglet à ouvrir d'abord (le bandeau de la reprise de la v2 ouvre Emplacements). */
    ongletInitial?: string | undefined;
  }

  let { signaler, ouvrirAssistant, ouvrirModeles, ongletInitial }: Props = $props();

  const onglets = [
    { id: 'moteurs', titre: 'Moteurs' },
    { id: 'llm', titre: 'LLM' },
    { id: 'workflows', titre: 'Modèles image et vidéo' },
    { id: 'lieux', titre: 'Emplacements et clés' },
    { id: 'securite', titre: 'Sécurité' },
    { id: 'mcp', titre: 'Claude (MCP)' },
  ];
  let onglet = $state('moteurs');
  /** « Chercher un réglage » et « Modifiés seulement » : gardés d'un onglet à l'autre. */
  let recherche = $state('');
  let modifies = $state(false);
  let etat = $state<EtatMoteurs | null>(null);
  /** Ce qu'on demande de confirmer : installer, réinstaller ou revenir, pour un moteur. */
  let demande = $state<{ action: 'installer' | 'reinstaller' | 'revenir'; moteur: EtatMoteur } | null>(null);
  let minuteur: ReturnType<typeof setTimeout> | undefined;
  /** « J'ai déjà ComfyUI » : le lancement d'essai en cours. */
  let verification = $state(false);

  const enCours = $derived(etat?.tache != null && !etat.tache.fini);
  const absents = $derived(etat ? manquants(etat.moteurs) : []);

  async function relire() {
    clearTimeout(minuteur);
    try {
      etat = await lireMoteurs();
    } catch (e) {
      signaler(messageErreur(e), true);
    }
    // Pendant une installation, l'avancement se relit chaque seconde.
    if (etat?.tache && !etat.tache.fini) minuteur = setTimeout(() => void relire(), 1000);
  }

  async function confirmer() {
    const d = demande;
    demande = null;
    if (!d) return;
    try {
      if (d.action === 'installer' || d.action === 'reinstaller') {
        await installerMoteur(d.moteur.id);
      } else {
        const version = await revenirMoteur(d.moteur.id);
        signaler(`${d.moteur.nom} : retour à la version ${version ?? 'posée à la main'}.`);
      }
    } catch (e) {
      signaler(messageErreur(e), true);
    }
    await relire();
  }

  async function annuler() {
    try {
      await annulerInstallationMoteur();
    } catch (e) {
      signaler(messageErreur(e), true);
    }
    await relire();
  }

  async function changerComfyui(autre: boolean) {
    try {
      const chemin = autre ? await parcourirDossier('Dossier de ton ComfyUI portable', null) : null;
      if (autre && !chemin) return;
      verification = true;
      await choisirComfyui(chemin);
      signaler(chemin ? `Aria utilise désormais ton ComfyUI : ${chemin}.` : "Aria utilise de nouveau son ComfyUI.");
    } catch (e) {
      signaler(messageErreur(e), true);
    } finally {
      verification = false;
    }
    await relire();
  }

  async function ouvrirMoteurs() {
    try {
      await ouvrirDossier('moteurs');
    } catch (e) {
      signaler(messageErreur(e), true);
    }
  }

  async function liberer() {
    try {
      await libererCarte();
      signaler('Carte graphique libérée.');
    } catch (e) {
      signaler(messageErreur(e), true);
    }
    await relire();
  }

  onMount(() => {
    if (ongletInitial && onglets.some((o) => o.id === ongletInitial)) onglet = ongletInitial;
    void relire();
    return () => clearTimeout(minuteur);
  });
</script>

<h1>Réglages</h1>
<p class="sous-titre">Les moteurs, le LLM, les modèles, tes dossiers et tes clés.</p>
<SousOnglets {onglets} bind:actif={onglet} />
{#if onglet === 'securite'}
  <ReglagesSecurite {signaler} />
{:else if onglet === 'mcp'}
  <ReglagesMcp {signaler} />
{:else if onglet === 'lieux'}
  <ReglagesLieux {signaler} />
{:else if onglet === 'llm'}
  <ReglagesLlm {signaler} bind:recherche bind:modifies />
{:else if onglet === 'workflows'}
  <ReglagesWorkflows {signaler} {ouvrirModeles} bind:recherche bind:modifies />
{:else}
<ASavoir>
  Un seul moteur tient la carte graphique à la fois : ComfyUI se vide quand le LLM écrit, puis reprend. Les versions
  sont épinglées et testées ensemble, téléchargées depuis leur release GitHub officielle (empreinte SHA-256 vérifiée) ;
  l'ancienne reste à côté, pour revenir en arrière.
</ASavoir>

{#if etat}
  {#if absents.length > 0}
    <p class="manque" role="status">
      {absents.map((m) => m.nom).join(' et ')} : absent{absents.length > 1 ? 's' : ''}, Aria ne peut pas générer sans.
    </p>
  {/if}

  <Panneau titre="Carte graphique">
    {#if etat.carte.nvidia && etat.carte.ok}
      <p class="carte">
        <strong>{etat.carte.nom ?? 'NVIDIA'}</strong>
        <span class="discret">· {detailCarte(etat.carte)}</span>
      </p>
    {:else}
      <p class="ko">{texteCarte(etat.carte)}</p>
    {/if}
    <!-- Mesure en direct : une commande Tauri manque (issue « commande Tauri pour la VRAM utilisée »). Vide et « à tester » d'ici là. -->
    <Jauge etiquette="VRAM" fraction={null} />
    <div class="actions">
      <Bouton petit onclick={() => void liberer()}>Libérer la carte graphique</Bouton>
      {#if ouvrirAssistant}
        <Bouton genre="discret" petit onclick={ouvrirAssistant}>🧭 Relancer l'assistant</Bouton>
      {/if}
    </div>
  </Panneau>

  <Panneau titre="Moteurs">
    <ul class="moteurs">
      {#each etat.moteurs as m (m.id)}
        {@const installer = texteInstaller(m)}
        {@const tache = etat.tache && etat.tache.id === m.id ? etat.tache : null}
        <li>
          <div class="moteur">
            <div class="nom">
              <strong>{m.nom}</strong>
              <span class="puce {teintePastille(m.statut)}">{libelleStatut(m.statut, m.nom)}</span>
              {#if m.facultatif}<span class="puce">facultatif</span>{/if}
              {#if m.occupe}<span class="puce">en marche</span>{/if}
              <span class="discret code">
                {m.versionInstallee ?? m.version}{m.versionInstallee && m.versionInstallee !== m.version
                  ? ` (épinglé ${m.version})`
                  : ''}
              </span>
            </div>
            <div class="actions">
              {#if tache && !tache.fini && tache.etape.startsWith('Téléchargement')}
                <Bouton genre="discret" petit onclick={() => void annuler()}>✕ Annuler</Bouton>
              {/if}
              {#if installer}
                <Bouton
                  petit
                  desactive={enCours || m.prerequis !== null}
                  onclick={() => (demande = { action: 'installer', moteur: m })}>{installer}</Bouton
                >
              {/if}
              {#if peutReinstaller(m)}
                <Bouton genre="discret" petit desactive={enCours} onclick={() => (demande = { action: 'reinstaller', moteur: m })}>
                  Réinstaller
                </Bouton>
              {/if}
              {#if m.precedente && !m.externe}
                <Bouton genre="discret" petit desactive={enCours} onclick={() => (demande = { action: 'revenir', moteur: m })}>
                  ↺ Revenir
                </Bouton>
              {/if}
              {#if !m.externe && m.statut !== 'absent'}
                <Bouton genre="discret" petit onclick={() => void ouvrirMoteurs()}>📂</Bouton>
              {/if}
            </div>
          </div>
          <p class="discret">{m.role}</p>
          {#if m.externe}<p class="discret">Dossier : <code>{m.chemin}</code></p>{/if}
          {#if m.id === 'comfyui' && verification}
            <p class="discret" role="status">
              Vérification de ce ComfyUI : lancement d'essai pour lire ses nœuds (jusqu'à une minute)…
            </p>
          {/if}
          {#if m.prerequis}
            <p class="ko">{m.prerequis}</p>
            <ExpliquerErreur erreur={m.prerequis} origine="moteur" contexte={m.nom} moteur={m.id} />
          {/if}
          {#if tache}
            <div class="suivi">
              <Jauge etiquette={tache.etape} fraction={fractionTache(tache)} mesure={mesureTache(tache)} />
              {#if tache.erreur}
                <p class="ko">{tache.erreur}</p>
                <ExpliquerErreur
                  erreur={tache.erreur}
                  origine="moteur"
                  contexte={`${m.nom} · ${tache.etape}`}
                  moteur={m.id}
                />
              {/if}
            </div>
          {/if}
        </li>
      {/each}
    </ul>
    <hr class="sep" />
    <div class="ligne">
      <span class="discret">J'ai déjà ComfyUI :</span>
      <Bouton genre="discret" petit desactive={enCours || verification} onclick={() => void changerComfyui(true)}>
        Utiliser mon ComfyUI…
      </Bouton>
      {#if etat.moteurs.some((m) => m.id === 'comfyui' && m.externe)}
        <Bouton genre="discret" petit desactive={enCours || verification} onclick={() => void changerComfyui(false)}>
          Revenir à celui d'Aria
        </Bouton>
      {/if}
    </div>
  </Panneau>
{:else}
  <p class="discret">Lecture…</p>
{/if}
{/if}

{#if demande}
  <Dialogue
    titre={demande.action === 'installer'
      ? `Installer ${demande.moteur.nom} ${demande.moteur.version} ?`
      : demande.action === 'reinstaller'
        ? `Réinstaller ${demande.moteur.nom} ${demande.moteur.version} ?`
        : `Revenir à la version précédente de ${demande.moteur.nom} ?`}
    fermer={() => (demande = null)}
  >
    <p>
      {demande.action === 'revenir'
        ? 'Le moteur est arrêté le temps de l’échange ; un second « Revenir » redonne la version actuelle.'
        : texteConfirmation(demande.moteur)}
    </p>
    {#snippet pied()}
      <Bouton genre="discret" onclick={() => (demande = null)}>Annuler</Bouton>
      <Bouton genre="principal" onclick={() => void confirmer()}
        >{demande?.action === 'installer' ? 'Installer' : demande?.action === 'reinstaller' ? 'Réinstaller' : 'Revenir'}</Bouton
      >
    {/snippet}
  </Dialogue>
{/if}

<style>
  .moteurs {
    display: grid;
    gap: 16px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  li {
    padding-bottom: 14px;
    border-bottom: 1px solid color-mix(in srgb, var(--ligne) 50%, transparent);
  }

  li:last-child {
    padding-bottom: 0;
    border-bottom: none;
  }

  .moteur {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: 8px 12px;
    align-items: center;
  }

  .nom {
    display: flex;
    flex-wrap: wrap;
    gap: 8px 10px;
    align-items: center;
  }

  .nom .discret {
    margin: 0;
    font-family: var(--police-code);
    font-size: 12px;
  }

  .moteur .actions {
    margin-top: 0;
  }

  .puce {
    padding: 1px 8px;
    border: 1px solid var(--ligne);
    color: var(--texte-2);
    font-size: 11.5px;
    letter-spacing: 0.04em;
  }

  .puce.cyan {
    border-color: var(--cyan);
    color: var(--cyan);
  }

  .puce.ambre {
    border-color: var(--ambre);
    color: var(--ambre);
  }

  .carte {
    margin: 0 0 10px;
  }

  .carte .discret {
    margin: 0;
  }

  .sep {
    margin: 14px 0;
    border: 0;
    border-top: 1px solid var(--ligne);
  }

  .ligne {
    display: flex;
    flex-wrap: wrap;
    gap: 8px 12px;
    align-items: center;
  }

  .discret {
    margin: 4px 0 0;
    color: var(--texte-2);
    font-size: 13px;
  }

  .ko {
    margin: 6px 0 0;
    color: var(--rouge);
  }

  .manque {
    margin: 0 0 16px;
    padding: 10px 14px;
    border-left: 3px solid var(--orange);
    color: var(--texte);
  }

  .suivi {
    margin-top: 10px;
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    justify-content: flex-end;
    margin-top: 10px;
  }
</style>
