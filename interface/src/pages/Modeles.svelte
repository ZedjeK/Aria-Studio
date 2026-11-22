<!--
  Modèles, comme en v2 (première partie) : les workflows livrés, rangés par tâche, avec leurs fichiers et leur licence,
  et les LLM du catalogue ; « Télécharger ce qui manque » met en file des fichiers aux adresses, tailles et empreintes
  figées. Rien ne part sans confirmation : le clic, après lecture de la licence, vaut accord. La file suit en direct.
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import {
    annulerTelechargement,
    etatModeles,
    installerLlm,
    lireTelechargements,
    messageErreur,
    ouvrirDossier,
    telechargerWorkflow,
    type EtatModeles,
    type LicenceModele,
  } from '../lib/api';
  import { octetsLisibles } from '../lib/jauge';
  import {
    fractionTelechargement,
    libelleTache,
    ordreTache,
    telechargementActif,
    texteTelechargement,
  } from '../lib/modeles';
  import ASavoir from '../lib/composants/ASavoir.svelte';
  import Bouton from '../lib/composants/Bouton.svelte';
  import Dialogue from '../lib/composants/Dialogue.svelte';
  import Panneau from '../lib/composants/Panneau.svelte';

  interface Props {
    signaler: (texte: string, erreur?: boolean) => void;
  }

  let { signaler }: Props = $props();

  let etat = $state<EtatModeles | null>(null);
  /** Ce qu'on demande de confirmer : un workflow ou un LLM, sa licence et ce qu'il faut télécharger. */
  let demande = $state<{ genre: 'workflow' | 'llm'; id: string; nom: string; licence: LicenceModele; taille: number } | null>(
    null,
  );

  const parTache = $derived(
    etat ? [...etat.workflows].sort((a, b) => ordreTache(a.tache) - ordreTache(b.tache) || a.nom.localeCompare(b.nom)) : [],
  );
  const actifs = $derived(etat?.taches.some(telechargementActif) ?? false);

  async function relire() {
    try {
      etat = await etatModeles();
    } catch (e) {
      signaler(messageErreur(e), true);
    }
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
    demande = null;
    if (!d) return;
    try {
      const n = d.genre === 'workflow' ? await telechargerWorkflow(d.id) : await installerLlm(d.id);
      signaler(n > 0 ? `${n} fichier${n > 1 ? 's' : ''} en file pour ${d.nom}.` : `Rien de plus à télécharger pour ${d.nom}.`);
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
<p class="sous-titre">Les modèles des workflows livrés et les LLM, téléchargés depuis leur source officielle.</p>
<ASavoir>
  Chaque fichier a une adresse, une taille et une empreinte SHA-256 fixées dans la fiche de son workflow : un fichier
  qui ne correspond pas est rejeté. Un téléchargement interrompu reprend où il s'était arrêté. Les modèles déjà
  téléchargés par la version 2 sont repris tels quels.
</ASavoir>

{#if etat && etat.taches.length > 0}
  <Panneau titre="Téléchargements">
    <ul class="file">
      {#each [...etat.taches].reverse() as t (t.id)}
        <li class:ko={t.etat === 'erreur'}>
          <div class="texte">
            <span>{t.nom} <span class="discret">· {t.dossier}</span></span>
            <span class="discret">{texteTelechargement(t)}</span>
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
  </Panneau>
{/if}

{#if etat}
  {#each parTache as w (w.id)}
    <Panneau titre={`${libelleTache(w.tache)} · ${w.nom}`}>
      <p class="discret">
        {w.famille} · licence {w.licence.nom}{w.licence.commercial ? '' : ' · usage commercial non permis'}
      </p>
      <p class="discret">{w.licence.resume}</p>
      <ul class="fichiers">
        {#each w.fichiers as f (f.dossier + f.nom)}
          <li class:present={f.present}>
            <span>{f.present ? '✓' : '·'}</span>
            <span class="code">{f.dossier}/{f.nom}</span>
            <span class="discret">{octetsLisibles(f.taille)}</span>
          </li>
        {/each}
      </ul>
      <div class="actions">
        {#if w.pret}
          <span class="bon">Prêt</span>
        {:else}
          <Bouton
            petit
            onclick={() =>
              (demande = { genre: 'workflow', id: w.id, nom: w.nom, licence: w.licence, taille: w.aTelecharger })}
          >
            Télécharger ce qui manque ({octetsLisibles(w.aTelecharger)})
          </Bouton>
        {/if}
        <Bouton genre="discret" petit onclick={() => void ouvrirDossier('modeles')}>Ouvrir le dossier des modèles</Bouton>
      </div>
    </Panneau>
  {/each}

  <Panneau titre="LLM">
    <ul class="llm">
      {#each etat.llm as l (l.fichier)}
        <li>
          <div class="texte">
            <strong>{l.nom}</strong>
            <span class="discret">
              {l.specialiste ? `spécialiste (${l.specialiste === 'creation' ? 'réécriture' : 'retouche'})` : 'généraliste'}
              {l.vision ? ' · voit les images' : ''} · licence {l.licence.nom} · {octetsLisibles(l.taille)}
            </span>
          </div>
          {#if l.installe}
            <span class="bon">Installé</span>
          {:else}
            <Bouton
              petit
              onclick={() => (demande = { genre: 'llm', id: l.fichier, nom: l.nom, licence: l.licence, taille: l.aTelecharger })}
            >
              Installer ({octetsLisibles(l.aTelecharger)})
            </Bouton>
          {/if}
        </li>
      {/each}
    </ul>
  </Panneau>
{:else}
  <p class="discret">Lecture…</p>
{/if}

{#if demande}
  <Dialogue titre={`Télécharger ${demande.nom} ?`} fermer={() => (demande = null)}>
    <p>{octetsLisibles(demande.taille)} à télécharger depuis Hugging Face, empreinte SHA-256 vérifiée.</p>
    <p>
      Licence : <strong>{demande.licence.nom}</strong>{demande.licence.commercial ? '' : ' (usage commercial non permis)'}.
      {demande.licence.resume}
    </p>
    <p class="discret code">{demande.licence.url}</p>
    {#snippet pied()}
      <Bouton genre="discret" onclick={() => (demande = null)}>Annuler</Bouton>
      <Bouton genre="principal" onclick={() => void confirmer()}>J'accepte la licence, télécharger</Bouton>
    {/snippet}
  </Dialogue>
{/if}

<style>
  .file,
  .fichiers,
  .llm {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin: 8px 0;
    padding: 0;
    list-style: none;
  }

  .file li,
  .llm li {
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

  .fichiers li {
    display: grid;
    grid-template-columns: 1.2em minmax(0, 1fr) auto;
    gap: 8px;
    font-size: 12px;
    color: var(--texte-2);
  }

  .fichiers li.present {
    color: var(--texte);
  }

  .fichiers .code {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px;
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
