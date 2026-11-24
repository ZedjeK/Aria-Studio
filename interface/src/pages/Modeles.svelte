<!--
  Modèles, comme la maquette : les workflows livrés et les LLM du catalogue, les onglets d'usage (Texte → image, Image →
  image, Vidéo) avec leur workflow par défaut, Ma collection et Rechercher. « Télécharger ce qui manque » met en file
  des fichiers aux adresses, tailles et empreintes figées. Rien ne part sans accord : la case « J'ai lu la licence »
  doit être cochée. La file des téléchargements s'ouvre à droite des onglets et suit en direct.
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import {
    annulerTelechargement,
    etatEmplacements,
    etatModeles,
    installerLlm,
    lireTelechargements,
    messageErreur,
    ouvrirDossier,
    telechargerWorkflow,
    usagesModeles,
    type EtatLieux,
    type EtatModeles,
    type LicenceModele,
    type TypeRecherche,
    type Usages,
  } from '../lib/api';
  import { octetsLisibles } from '../lib/jauge';
  import {
    etatFichier,
    fichiersAManquer,
    fractionTelechargement,
    libelleEtatFichier,
    libelleTache,
    ordreTache,
    roleFichier,
    telechargementActif,
    texteBoutonTelechargements,
    texteTelechargement,
  } from '../lib/modeles';
  import ASavoir from '../lib/composants/ASavoir.svelte';
  import Bouton from '../lib/composants/Bouton.svelte';
  import ExpliquerErreur from '../lib/composants/ExpliquerErreur.svelte';
  import Dialogue from '../lib/composants/Dialogue.svelte';
  import Panneau from '../lib/composants/Panneau.svelte';
  import SousOnglets from '../lib/composants/SousOnglets.svelte';
  import ModelesCollection from './ModelesCollection.svelte';
  import ModelesRechercher from './ModelesRechercher.svelte';
  import ModelesUsage from './ModelesUsage.svelte';

  interface Props {
    signaler: (texte: string, erreur?: boolean) => void;
  }

  let { signaler }: Props = $props();

  const onglets = [
    { id: 'telecharger', titre: 'Workflows et LLM' },
    { id: 'texte_vers_image', titre: 'Texte → image' },
    { id: 'image_vers_image', titre: 'Image → image' },
    { id: 'video', titre: 'Vidéo' },
    { id: 'collection', titre: 'Ma collection' },
    { id: 'rechercher', titre: 'Rechercher' },
  ];
  /** Le commentaire de chaque onglet, sous les onglets. */
  const COMMENTAIRES: Record<string, string> = {
    telecharger:
      "Chaque workflow dit les fichiers qu'il lui faut et sa licence. Rien n'est téléchargé sans ton accord : chaque fichier a une adresse, une taille et une empreinte SHA-256 fixées, et un fichier qui ne correspond pas est rejeté. Les modèles déjà téléchargés par la version 2 sont repris tels quels.",
    texte_vers_image:
      'Le workflow par défaut (★) est celui dont partent le Plateau, la Retouche et le Montage ; ses réglages fins (steps, CFG…) sont dans Réglages.',
    image_vers_image:
      'Pour retoucher, mélanger ou redessiner une image existante. Le workflow par défaut (★) est celui de la Retouche ; ses réglages fins sont dans Réglages.',
    video:
      'Les modèles vidéo sont lourds : vérifie la place libre avant de télécharger. Le workflow par défaut (★) est celui du Montage.',
    collection:
      "Un fichier déposé à la main est reconnu par son empreinte SHA-256 ; sa source se retrouve sur Civitai. Seuls les modèles téléchargés par Aria se suppriment d'ici : ils partent dans la corbeille, d'où ils se restaurent.",
    rechercher:
      "La recherche se limite aux familles qu'Aria sait faire tourner. Seuls .safetensors et .gguf sont téléchargés : les autres formats peuvent exécuter du code au chargement. Un modèle qui demande une clé se télécharge avec celle de Réglages → Emplacements et clés.",
  };
  let onglet = $state('telecharger');
  let etat = $state<EtatModeles | null>(null);
  let usages = $state<Usages | null>(null);
  let lieux = $state<EtatLieux | null>(null);
  /** La file des téléchargements est ouverte. */
  let fileOuverte = $state(false);
  /** La case « J'ai lu la licence » de la demande en cours. */
  let licenceLue = $state(false);
  /** Rechercher ouvert depuis un onglet d'usage : son type et sa famille. */
  let depart = $state<{ type: TypeRecherche; famille: string } | null>(null);

  const usageOuvert = $derived(usages?.usages.find((u) => u.id === onglet) ?? null);
  /** Ce qu'on demande de confirmer : un workflow ou un LLM, sa licence et ce qu'il faut télécharger. */
  let demande = $state<{
    genre: 'workflow' | 'llm';
    id: string;
    nom: string;
    licence: LicenceModele;
    taille: number;
    fichiers: number | null;
  } | null>(null);

  const parTache = $derived(
    etat ? [...etat.workflows].sort((a, b) => ordreTache(a.tache) - ordreTache(b.tache) || a.nom.localeCompare(b.nom)) : [],
  );
  const actifs = $derived(etat?.taches.some(telechargementActif) ?? false);
  const enCours = $derived(etat?.taches.filter(telechargementActif).length ?? 0);
  /** Le premier workflow à qui il manque des fichiers : le panneau accentué et le bouton orange de l'onglet. */
  const premierManquant = $derived(parTache.find((w) => !w.pret)?.id ?? null);
  const libre = $derived(lieux?.libres['modeles'] ?? null);

  async function relire() {
    try {
      [etat, usages, lieux] = await Promise.all([etatModeles(), usagesModeles(), etatEmplacements()]);
    } catch (e) {
      signaler(messageErreur(e), true);
    }
  }

  function demander(genre: 'workflow' | 'llm', id: string, nom: string, licence: LicenceModele, taille: number) {
    const w = genre === 'workflow' ? etat?.workflows.find((x) => x.id === id) : undefined;
    licenceLue = false;
    demande = { genre, id, nom, licence, taille, fichiers: w ? fichiersAManquer(w.fichiers) : null };
  }

  function chercher(type: TypeRecherche, famille: string) {
    depart = { type, famille };
    onglet = 'rechercher';
  }

  async function suivre() {
    if (!etat) return;
    try {
      const avant = etat.taches.filter((t) => t.etat === 'termine').length;
      etat.taches = await lireTelechargements();
      // Un fichier vient d'arriver : ce qui manque change.
      if (etat.taches.filter((t) => t.etat === 'termine').length !== avant) await relire();
    } catch (e) {
      signaler(messageErreur(e), true);
    }
  }

  async function confirmer() {
    const d = demande;
    if (!d || !licenceLue) return;
    demande = null;
    try {
      const n = d.genre === 'workflow' ? await telechargerWorkflow(d.id) : await installerLlm(d.id);
      signaler(
        n > 0
          ? `${n} fichier${n > 1 ? 's' : ''} en file pour ${d.nom} : suis-les dans « Téléchargements ».`
          : `Rien de plus à télécharger pour ${d.nom}.`,
      );
      await relire();
    } catch (e) {
      signaler(messageErreur(e), true);
    }
  }

  async function annuler(id: string) {
    try {
      await annulerTelechargement(id);
      await suivre();
    } catch (e) {
      signaler(messageErreur(e), true);
    }
  }

  onMount(() => {
    void relire();
    const minuterie = setInterval(() => {
      if (actifs) void suivre();
    }, 1000);
    return () => clearInterval(minuterie);
  });
</script>

<h1>Modèles</h1>
<p class="sous-titre">Ce qu'Aria utilise pour écrire et pour dessiner, et d'où ça vient.</p>
<div class="rangee">
  <SousOnglets {onglets} bind:actif={onglet} />
  <div class="droite">
    <Bouton genre="secondaire" petit onclick={() => (fileOuverte = true)}>{texteBoutonTelechargements(enCours)}</Bouton>
  </div>
</div>
<ASavoir>{COMMENTAIRES[onglet] ?? ''}</ASavoir>

{#if onglet === 'collection'}
  <ModelesCollection {signaler} />
{:else if onglet === 'rechercher'}
  <ModelesRechercher {signaler} {depart} ajoute={() => void relire()} />
{:else if onglet !== 'telecharger'}
  {#if usages && usageOuvert}
    <ModelesUsage
      usage={usageOuvert}
      llmDefaut={usages.llmDefaut}
      llmChoix={usages.llmChoix}
      {signaler}
      {relire}
      {demander}
      {chercher}
      versCollection={() => (onglet = 'collection')}
    />
  {:else}
    <p class="discret">Lecture…</p>
  {/if}
{:else if etat}
  {#each parTache as w (w.id)}
    {@const premier = w.id === premierManquant}
    <Panneau titre={`${w.nom} · ${libelleTache(w.tache)}`} accentue={premier}>
      <ul class="fichiers">
        {#each w.fichiers as f (f.dossier + f.nom)}
          {@const etatF = etatFichier(w.pret, f.present)}
          <li>
            <span class="verdict" class:ok={f.present} role="img" aria-label={libelleEtatFichier(etatF)}>
              {etatF === 'present' ? '✓' : etatF === 'manque' ? '○' : '–'}
            </span>
            <span class="nom">
              <span class="code">{f.nom}</span>
              <span class="discret">
                · {roleFichier(f.dossier)} · {octetsLisibles(f.taille)}{etatF === 'present' ? '' : ` · ${libelleEtatFichier(etatF)}`}
              </span>
            </span>
          </li>
        {/each}
      </ul>
      <p class="discret">
        📜 Licence : {w.licence.nom}{w.licence.commercial ? '' : ' · usage commercial non permis'} · {w.licence.resume}
      </p>
      <p class="discret code">{w.licence.url}</p>
      <div class="actions">
        {#if w.pret}
          <span class="bon">Prêt</span>
        {:else}
          <Bouton
            genre={premier ? 'principal' : 'secondaire'}
            onclick={() => demander('workflow', w.id, w.nom, w.licence, w.aTelecharger)}
          >
            ⬇ Télécharger ce qui manque ({octetsLisibles(w.aTelecharger)})
          </Bouton>
        {/if}
        <Bouton genre="discret" petit onclick={() => void ouvrirDossier('modeles')}>📂 Ouvrir le dossier des modèles</Bouton>
      </div>
    </Panneau>
  {/each}

  <Panneau titre="LLM du catalogue">
    <ul class="fichiers">
      {#each etat.llm as l (l.fichier)}
        <li>
          <span class="verdict" class:ok={l.installe} role="img" aria-label={l.installe ? 'installé' : 'à installer'}>
            {l.installe ? '✓' : '○'}
          </span>
          <span class="nom">
            <strong>{l.nom}</strong>
            <span class="discret">
              · {l.specialiste ? `spécialiste (${l.specialiste === 'creation' ? 'réécriture' : 'retouche'})` : 'généraliste'}
              {l.vision ? ' · 👁 voit les images' : ''} · licence {l.licence.nom} · {octetsLisibles(l.taille)}
            </span>
          </span>
          {#if l.installe && usages?.llmDefaut === l.fichier}
            <span class="puce">En service</span>
          {:else if l.installe}
            <span class="bon">Installé</span>
          {:else}
            <Bouton genre="secondaire" petit onclick={() => demander('llm', l.fichier, l.nom, l.licence, l.aTelecharger)}>
              ⬇ Installer ({octetsLisibles(l.aTelecharger)})
            </Bouton>
          {/if}
        </li>
      {/each}
    </ul>
  </Panneau>
{:else}
  <p class="discret">Lecture…</p>
{/if}

{#if fileOuverte}
  <Dialogue titre="Téléchargements" fermer={() => (fileOuverte = false)}>
    {#if etat && etat.taches.length > 0}
      <ul class="file">
        {#each [...etat.taches].reverse() as t (t.id)}
          <li class:ko={t.etat === 'erreur'}>
            <div class="texte">
              <span class="code">{t.nom} <span class="discret">· {t.dossier}</span></span>
              <span class="discret">{texteTelechargement(t)}</span>
              {#if t.etat === 'erreur' && t.erreur}
                <ExpliquerErreur erreur={t.erreur} origine="telechargement" contexte={`${t.nom} · ${t.dossier}`} />
              {/if}
              {#if t.etat === 'en_cours'}
                <progress max="1" value={fractionTelechargement(t) ?? 0}></progress>
              {/if}
            </div>
            {#if telechargementActif(t)}
              <Bouton genre="discret" petit onclick={() => void annuler(t.id)}>✕ Annuler</Bouton>
            {/if}
          </li>
        {/each}
      </ul>
    {:else}
      <p class="discret">Aucun téléchargement pour l'instant.</p>
    {/if}
    <p class="discret">Un téléchargement interrompu reprend où il s'est arrêté. Taille et SHA-256 vérifiés à la fin.</p>
    {#snippet pied()}
      <Bouton genre="discret" onclick={() => (fileOuverte = false)}>Fermer</Bouton>
    {/snippet}
  </Dialogue>
{/if}

{#if demande}
  <Dialogue titre={demande.genre === 'llm' ? 'Installer ce LLM' : 'Télécharger ce qui manque'} fermer={() => (demande = null)}>
    <Panneau titre={demande.nom} accentue>
      <p>
        {demande.fichiers !== null ? `${demande.fichiers} fichier${demande.fichiers > 1 ? 's' : ''} · ` : ''}<b class="code">
          {octetsLisibles(demande.taille)}
        </b>{libre !== null ? ` · place libre : ${octetsLisibles(libre)}` : ''} · depuis Hugging Face, empreinte SHA-256
        vérifiée.
      </p>
      <label class="case">
        <input type="checkbox" bind:checked={licenceLue} />
        J'ai lu la licence {demande.licence.nom}{demande.licence.commercial ? '' : ' (usage commercial non permis)'}
      </label>
      <p class="discret">{demande.licence.resume}</p>
      <p class="discret code">{demande.licence.url}</p>
    </Panneau>
    {#snippet pied()}
      <Bouton genre="discret" onclick={() => (demande = null)}>Annuler</Bouton>
      <Bouton genre="principal" desactive={!licenceLue} onclick={() => void confirmer()}>Télécharger</Bouton>
    {/snippet}
  </Dialogue>
{/if}

<style>
  /* Six onglets et le bouton Téléchargements : un onglet ne passe jamais à la ligne ni ne se rétrécit ; faute de place,
     la rangée des onglets passe à la ligne par onglet entier et le bouton descend à droite. */
  .rangee {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-end;
    gap: 6px 12px;
    border-bottom: 1px solid var(--ligne);
  }

  .rangee > :global(.onglets) {
    flex: 1 1 auto;
    flex-wrap: wrap;
    min-width: 0;
    border-bottom: 0;
  }

  .rangee :global(.onglet) {
    flex: none;
    white-space: nowrap;
  }

  .droite {
    margin-left: auto;
    padding-bottom: 6px;
  }

  .file,
  .fichiers {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin: 8px 0;
    padding: 0;
    list-style: none;
  }

  .file li,
  .fichiers li {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .texte {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  .nom {
    flex: 1;
    min-width: 0;
    overflow-wrap: anywhere;
  }

  .verdict {
    flex: none;
    width: 1.2em;
    text-align: center;
    color: var(--ambre);
  }

  .verdict.ok {
    color: var(--cyan);
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px;
    margin-top: 8px;
  }

  .case {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 10px 0;
  }

  .puce {
    padding: 2px 10px;
    border: 1px solid var(--cyan);
    color: var(--cyan);
    font-size: 11.5px;
    letter-spacing: 0.04em;
  }

  progress {
    width: 100%;
    height: 4px;
    accent-color: var(--cyan);
  }

  .bon {
    color: var(--cyan);
  }

  .ko {
    color: var(--rouge);
  }
</style>
