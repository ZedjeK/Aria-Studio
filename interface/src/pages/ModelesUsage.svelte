<!--
  Modèles → un onglet d'usage (Texte → image, Image → image, Vidéo), comme en v2 : les favoris de l'usage, puis une
  carte par workflow (ce qui manque, checkpoints et checkpoint par défaut, LoRA de la famille, LLM qui réécrit pour lui
  et LLM entraînés pour ce modèle, licence). Les téléchargements passent par la confirmation de la page Modèles.
-->
<script lang="ts">
  import {
    definirFavori,
    definirLlmWorkflow,
    messageErreur,
    modifierReglagesWorkflow,
    ouvrirDossier,
    type ChoixLlm,
    type LicenceModele,
    type SpecialisteUsage,
    type UsageModeles,
    type WorkflowUsage,
  } from '../lib/api';
  import { octetsLisibles } from '../lib/jauge';
  import { attentes, natureDe, nomLlm, puces } from '../lib/usages';
  import Bouton from '../lib/composants/Bouton.svelte';
  import Panneau from '../lib/composants/Panneau.svelte';

  interface Props {
    usage: UsageModeles;
    llmDefaut: string;
    llmChoix: ChoixLlm[];
    signaler: (texte: string, erreur?: boolean) => void;
    /** Relit les usages après un changement. */
    relire: () => Promise<void>;
    /** Demande la confirmation d'un téléchargement (licence lue). */
    demander: (genre: 'workflow' | 'llm', id: string, nom: string, licence: LicenceModele, taille: number) => void;
    /** Ouvre Rechercher sur ce type et cette famille. */
    chercher: (type: 'image' | 'video' | 'lora', famille: string) => void;
  }

  let { usage, llmDefaut, llmChoix, signaler, relire, demander, chercher }: Props = $props();

  let occupe = $state(false);

  const nomDe = (id: string | null) => usage.workflows.find((w) => w.id === id)?.nom ?? id ?? '—';
  const libelle = (cle: string) => usage.favoris.find((f) => f.cle === cle)?.libelle ?? cle;

  async function agir(action: () => Promise<void>, message: string) {
    if (occupe) return;
    occupe = true;
    try {
      await action();
      signaler(message);
      await relire();
    } catch (e) {
      signaler(messageErreur(e), true);
    } finally {
      occupe = false;
    }
  }

  function choisirLlm(wf: WorkflowUsage, valeur: string) {
    const llm = valeur === '' ? null : valeur;
    void agir(
      () => definirLlmWorkflow(wf.id, llm),
      `${wf.nom} : réécrit par ${llm ? nomLlm(llm, llmChoix) : 'le LLM par défaut'}.`,
    );
  }

  function ouvrir(sous: string) {
    void ouvrirDossier('modeles', sous).catch((e: unknown) => signaler(messageErreur(e), true));
  }

  function installer(s: SpecialisteUsage) {
    demander('llm', s.fichier, s.nom, s.licence, s.aTelecharger);
  }
</script>

<Panneau titre="Par défaut">
  <div class="favoris">
    {#each usage.favoris as f (f.cle)}
      <div class="favori">
        <span class="discret">{f.libelle}</span>
        <strong>{f.valeur ? `★ ${nomDe(f.valeur)}` : '—'}</strong>
      </div>
    {/each}
  </div>
  <p class="discret">
    Le Plateau, la Retouche et le Montage partent de ces workflows. Le checkpoint par défaut de chacun se choisit plus
    bas ; ses réglages fins (steps, CFG…) sont dans Réglages.
  </p>
</Panneau>

{#each usage.workflows as wf (wf.id)}
  <Panneau titre={wf.nom}>
    <div class="ligne">
      {#if wf.pret}
        <span class="bon">Prêt</span>
      {:else}
        <span class="alerte">Il manque {octetsLisibles(wf.aTelecharger)}</span>
        <Bouton
          genre="principal"
          petit
          onclick={() => demander('workflow', wf.id, wf.nom, wf.licence, wf.aTelecharger)}
        >
          Télécharger ce qui manque
        </Bouton>
      {/if}
    </div>
    <div class="puces">
      {#each puces(wf) as p (p)}
        <span class="puce">{p}</span>
      {/each}
    </div>
    {#if wf.possibles.length > 0}
      <div class="ligne">
        {#each wf.possibles as cle (cle)}
          {#if wf.favoriPour.includes(cle)}
            <span class="bon">★ {libelle(cle)}</span>
          {:else}
            <Bouton
              genre="discret"
              petit
              desactive={occupe || !wf.pret}
              onclick={() => void agir(() => definirFavori(cle, wf.id), `${wf.nom} : par défaut pour « ${libelle(cle)} ».`)}
            >
              ☆ Par défaut pour « {libelle(cle)} »
            </Bouton>
          {/if}
        {/each}
      </div>
      {#if !wf.pret}
        <p class="discret">Télécharge d'abord ce qui manque pour en faire un favori.</p>
      {/if}
    {/if}
    {#if wf.manquants.length > 0}
      <p class="discret">Manquants : {wf.manquants.map((f) => `${f.dossier}/${f.nom}`).join(', ')}</p>
    {/if}

    <div class="colonnes">
      <section aria-label="Checkpoints">
        <h3>Checkpoints</h3>
        <ul>
          {#each wf.modeles as m (m)}
            <li>
              <span class="nom">{m}</span>
              {#if m === wf.origine}<span class="discret">d'origine</span>{/if}
              {#if wf.modeleDefaut !== null}
                {#if m === wf.modeleDefaut}
                  <span class="bon">par défaut</span>
                {:else}
                  <Bouton
                    genre="discret"
                    petit
                    desactive={occupe}
                    onclick={() =>
                      void agir(
                        async () => {
                          await modifierReglagesWorkflow(wf.id, { modele: m });
                        },
                        `${wf.nom} : ${m} par défaut.`,
                      )}
                  >
                    ☆ Par défaut
                  </Bouton>
                {/if}
              {/if}
            </li>
          {:else}
            <li class="discret">Aucun checkpoint prêt.</li>
          {/each}
          {#each wf.aValider as m (m)}
            <li><span class="nom">{m}</span> <span class="alerte">steps à valider (Ma collection)</span></li>
          {/each}
        </ul>
        <div class="ligne">
          <Bouton genre="discret" petit onclick={() => chercher(natureDe(wf), wf.famille)}>Chercher des checkpoints</Bouton>
          <Bouton genre="discret" petit onclick={() => ouvrir(wf.dossierModeles)}>Ouvrir le dossier</Bouton>
        </div>
      </section>
      <section aria-label="LoRA">
        <h3>LoRA</h3>
        <ul>
          {#each wf.loras as l (l.nom)}
            <li>
              <span class="nom">{l.nom}</span>
              <span class="discret">{l.declencheurs ? `« ${l.declencheurs} » · ` : ''}force {l.force}</span>
            </li>
          {:else}
            <li class="discret">Aucune LoRA pour cette famille.</li>
          {/each}
        </ul>
        <div class="ligne">
          <Bouton genre="discret" petit onclick={() => chercher('lora', wf.famille)}>Chercher des LoRA</Bouton>
          <Bouton genre="discret" petit onclick={() => ouvrir(wf.dossierLoras)}>Ouvrir le dossier</Bouton>
        </div>
      </section>
    </div>

    <details open={wf.reecriture.ailleurs !== null}>
      <summary>
        Réécriture du prompt <span class="discret">· {nomLlm(wf.reecriture.llm ?? llmDefaut, llmChoix)}</span>
      </summary>
      <label class="champ">
        LLM
        <select
          value={wf.reecriture.llm ?? ''}
          disabled={occupe}
          onchange={(e) => choisirLlm(wf, e.currentTarget.value)}
        >
          <option value="">LLM par défaut ({nomLlm(llmDefaut, llmChoix)})</option>
          {#each llmChoix as c (c.fichier)}
            <option value={c.fichier}>{c.specialiste ? '🎯 ' : ''}{c.nom}{c.favori ? ' ★' : ''}</option>
          {/each}
        </select>
      </label>
      {#if wf.reecriture.specialistes.find((s) => s.choisi)}
        <p class="discret">
          {wf.reecriture.specialistes.find((s) => s.choisi)?.nom} réécrit avec sa propre consigne officielle : les règles
          du modèle ne lui sont pas données.
        </p>
      {:else}
        <p class="discret">Ce modèle attend {attentes(wf.reecriture)}.</p>
      {/if}
      {#if wf.reecriture.ailleurs}
        <p class="alerte">
          Ce LLM est entraîné pour {wf.reecriture.ailleurs.join(', ')} : pour ce workflow, Aria passera au LLM par défaut.
        </p>
      {/if}
      {#each wf.reecriture.specialistes.filter((s) => !s.choisi) as s (s.fichier)}
        <div class="specialiste">
          <p>
            🎯 Un LLM entraîné pour ce modèle existe : <strong>{s.nom}</strong>. Il réécrit
            {s.usage === 'retouche' ? "les consignes de retouche (il voit l'image)" : 'les idées'} comme son auteur l'a
            prévu.
          </p>
          <p class="discret">{s.licence.nom} · {s.licence.resume}</p>
          <p class="discret code">{s.page}</p>
          {#if s.installe}
            <Bouton
              petit
              desactive={occupe}
              onclick={() => void agir(() => definirLlmWorkflow(wf.id, s.fichier), `${wf.nom} : réécrit par ${s.nom}.`)}
            >
              Utiliser pour ce workflow
            </Bouton>
          {:else}
            <Bouton petit onclick={() => installer(s)}>Installer ({octetsLisibles(s.aTelecharger)})</Bouton>
          {/if}
        </div>
      {/each}
      {#each wf.reecriture.aVenir as a (a.nom)}
        <p class="discret">🕓 {a.nom} est entraîné pour ce modèle : {a.raison}</p>
      {/each}
    </details>

    <details>
      <summary>Licence</summary>
      <p class="discret">
        {wf.licence.nom}{wf.licence.commercial ? '' : ' · usage commercial non permis'} · {wf.licence.resume}
      </p>
      <p class="discret code">{wf.licence.url}</p>
    </details>
  </Panneau>
{:else}
  <p class="discret">Aucun workflow pour cet usage.</p>
{/each}

{#if usage.variation.length > 0}
  <p class="discret">
    🎨 Redessiner une image (Plateau, départ « Redessiner ») part d'une image avec un modèle texte vers image :
    {usage.variation.join(', ')}.
  </p>
{/if}

<style>
  .favoris,
  .ligne,
  .puces {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px;
    margin: 6px 0;
  }

  .favori {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 180px;
  }

  .puce {
    padding: 2px 8px;
    border: 1px solid var(--ligne);
    font-size: 12px;
    color: var(--texte-2);
  }

  .colonnes {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(260px, 1fr));
    gap: 16px;
    margin: 10px 0;
  }

  h3 {
    margin: 0 0 6px;
    font-size: 13px;
  }

  ul {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin: 0 0 8px;
    padding: 0;
    list-style: none;
  }

  li {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    font-size: 13px;
  }

  .nom {
    overflow-wrap: anywhere;
  }

  details {
    margin-top: 8px;
  }

  summary {
    cursor: pointer;
  }

  .champ {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin: 8px 0;
    font-size: 12px;
    color: var(--texte-2);
  }

  .specialiste {
    margin: 8px 0;
    padding: 8px 12px;
    border-left: 2px solid var(--cyan);
  }

  .code {
    overflow-wrap: anywhere;
  }

  .bon {
    color: var(--cyan);
  }

  .alerte {
    color: var(--orange);
  }
</style>
