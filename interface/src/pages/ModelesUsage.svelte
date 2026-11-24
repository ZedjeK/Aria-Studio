<!--
  Modèles → un onglet d'usage (Texte → image, Image → image, Vidéo), comme la maquette : le workflow par défaut en
  premier, seul panneau accentué de l'onglet, avec ses checkpoints (★ sur le défaut), les LoRA de sa famille, le LLM qui
  réécrit pour lui et sa licence ; les autres workflows en cartes plus courtes, le détail repliable. Aucun bouton
  orange : « Télécharger ce qui manque » est une action secondaire. Les téléchargements passent par la confirmation de
  la page Modèles.
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
  import { repereVision } from '../lib/plateau';
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
    /** Ouvre Ma collection (steps à valider d'un checkpoint déposé à la main). */
    versCollection: () => void;
  }

  let { usage, llmDefaut, llmChoix, signaler, relire, demander, chercher, versCollection }: Props = $props();

  let occupe = $state(false);

  const libelle = (cle: string) => usage.favoris.find((f) => f.cle === cle)?.libelle ?? cle;
  /** Les workflows par défaut d'abord, le reste dans l'ordre reçu. */
  const classes = $derived(
    [...usage.workflows].sort((a, b) => Number(b.favoriPour.length > 0) - Number(a.favoriPour.length > 0)),
  );
  /** Le seul panneau accentué de l'onglet : le premier workflow par défaut. */
  const premierDefaut = $derived(classes.find((w) => w.favoriPour.length > 0)?.id ?? null);

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

{#snippet detail(wf: WorkflowUsage)}
  <p class="etiquette">Checkpoints</p>
  <ul>
    {#each wf.modeles as m (m)}
      <li>
        <span class="etoile">{m === wf.modeleDefaut ? '★' : ''}</span>
        <span class="nom code">{m}</span>
        {#if m === wf.origine}<span class="discret">d'origine</span>{/if}
        {#if wf.modeleDefaut !== null && m !== wf.modeleDefaut}
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
      </li>
    {:else}
      <li class="discret">Aucun checkpoint prêt.</li>
    {/each}
    {#each wf.aValider as m (m)}
      <li>
        <span class="etoile"></span>
        <span class="nom code">{m}</span>
        <span class="puce ambre">Steps à valider</span>
        <Bouton genre="discret" petit onclick={versCollection}>Valider dans Ma collection</Bouton>
      </li>
    {/each}
  </ul>
  {#if wf.aValider.length > 0}
    <p class="discret">Un checkpoint déposé à la main n'est proposé nulle part tant que ses steps ne sont pas validés.</p>
  {/if}
  <div class="ligne">
    <Bouton genre="secondaire" petit onclick={() => chercher(natureDe(wf), wf.famille)}>🔎 Chercher des checkpoints</Bouton>
    <Bouton genre="discret" petit onclick={() => ouvrir(wf.dossierModeles)}>📂 Ouvrir le dossier</Bouton>
  </div>

  <hr />
  <p class="etiquette">LoRA de la famille</p>
  <ul>
    {#each wf.loras as l (l.nom)}
      <li>
        <span class="nom">{l.nom}</span>
        {#if l.declencheurs}<span class="discret">« {l.declencheurs} »</span>{/if}
        <span class="puce">force {l.force}</span>
      </li>
    {:else}
      <li class="discret">Aucune LoRA pour cette famille.</li>
    {/each}
  </ul>
  <div class="ligne">
    <Bouton genre="secondaire" petit onclick={() => chercher('lora', wf.famille)}>🔎 Chercher des LoRA</Bouton>
    <Bouton genre="discret" petit onclick={() => ouvrir(wf.dossierLoras)}>📂 Ouvrir le dossier</Bouton>
  </div>

  <hr />
  <p class="etiquette">✍️ Réécriture du prompt</p>
  <div class="ligne">
    <select
      value={wf.reecriture.llm ?? ''}
      disabled={occupe}
      aria-label="LLM qui réécrit le prompt de ce workflow"
      onchange={(e) => choisirLlm(wf, e.currentTarget.value)}
    >
      <option value="">LLM par défaut ({nomLlm(llmDefaut, llmChoix)})</option>
      {#each llmChoix as c (c.fichier)}
        <option value={c.fichier}>{c.specialiste ? '🎯 ' : ''}{c.nom}{c.favori ? ' ★' : ''}{repereVision(c)}</option>
      {/each}
    </select>
  </div>
  {#if wf.reecriture.specialistes.find((s) => s.choisi)}
    <p class="discret">
      {wf.reecriture.specialistes.find((s) => s.choisi)?.nom} réécrit avec sa propre consigne officielle : les règles du
      modèle ne lui sont pas données.
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
        {s.usage === 'retouche' ? "les consignes de retouche (il voit l'image)" : 'les idées'} comme son auteur l'a prévu.
      </p>
      <p class="discret">{s.licence.nom} · {s.licence.resume}</p>
      <p class="discret code">{s.page}</p>
      {#if s.installe}
        <Bouton
          genre="discret"
          petit
          desactive={occupe}
          onclick={() => void agir(() => definirLlmWorkflow(wf.id, s.fichier), `${wf.nom} : réécrit par ${s.nom}.`)}
        >
          Utiliser pour ce workflow
        </Bouton>
      {:else}
        <Bouton genre="secondaire" petit onclick={() => installer(s)}>⬇ Installer ({octetsLisibles(s.aTelecharger)})</Bouton>
      {/if}
    </div>
  {/each}
  {#each wf.reecriture.aVenir as a (a.nom)}
    <p class="discret">🕓 {a.nom} est entraîné pour ce modèle : {a.raison}</p>
  {/each}

  <hr />
  <p class="discret">
    📜 Licence {wf.licence.nom}{wf.licence.commercial ? '' : ' · usage commercial non permis'} · {wf.licence.resume}
  </p>
  <p class="discret code">{wf.licence.url}</p>
{/snippet}

{#each classes as wf (wf.id)}
  {@const defaut = wf.favoriPour.length > 0}
  <Panneau titre={defaut ? `${wf.nom} · ★ Par défaut` : wf.nom} accentue={wf.id === premierDefaut}>
    <div class="ligne">
      <span class="puce" class:ambre={!wf.pret}>{wf.pret ? 'complet' : 'incomplet'}</span>
      {#each puces(wf) as p (p)}
        <span class="puce neutre">{p}</span>
      {/each}
      <span class="puce neutre">{wf.licence.nom}</span>
    </div>
    {#if !wf.pret}
      <div class="ligne">
        <span class="alerte">Il manque {octetsLisibles(wf.aTelecharger)}</span>
        {#if wf.manquants.length > 0}
          <span class="discret">{wf.manquants.map((f) => `${f.dossier}/${f.nom}`).join(', ')}</span>
        {/if}
        <Bouton genre="secondaire" petit onclick={() => demander('workflow', wf.id, wf.nom, wf.licence, wf.aTelecharger)}>
          ⬇ Télécharger ce qui manque
        </Bouton>
      </div>
    {/if}
    {#if wf.possibles.length > 0}
      <div class="ligne">
        {#each wf.possibles as cle (cle)}
          {@const court = wf.possibles.length === 1}
          {#if wf.favoriPour.includes(cle)}
            <span class="bon">★ Par défaut{court ? '' : ` pour « ${libelle(cle)} »`}</span>
          {:else}
            <Bouton
              genre="discret"
              petit
              desactive={occupe || !wf.pret}
              onclick={() => void agir(() => definirFavori(cle, wf.id), `${wf.nom} : par défaut pour « ${libelle(cle)} ».`)}
            >
              ☆ Par défaut{court ? '' : ` pour « ${libelle(cle)} »`}
            </Bouton>
          {/if}
        {/each}
      </div>
      {#if !wf.pret}
        <p class="discret">Télécharge d'abord ce qui manque pour en faire un workflow par défaut.</p>
      {/if}
    {/if}

    {#if wf.id === premierDefaut}
      <hr />
      {@render detail(wf)}
    {:else}
      <details>
        <summary>Checkpoints, LoRA, réécriture du prompt et licence</summary>
        {@render detail(wf)}
      </details>
    {/if}
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
  .ligne {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px;
    margin: 6px 0;
  }

  .puce {
    padding: 2px 10px;
    border: 1px solid var(--cyan);
    color: var(--cyan);
    font-size: 11.5px;
    letter-spacing: 0.04em;
  }

  .puce.neutre {
    border-color: var(--ligne);
    color: var(--texte-2);
  }

  .puce.ambre {
    border-color: var(--ambre);
    color: var(--ambre);
  }

  .etiquette {
    margin: 10px 0 4px;
    font-size: 12px;
    letter-spacing: 0.06em;
    color: var(--texte-2);
  }

  hr {
    margin: 12px 0;
    border: 0;
    border-top: 1px solid var(--ligne);
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

  .etoile {
    width: 1.2em;
    color: var(--ambre);
  }

  .nom {
    overflow-wrap: anywhere;
  }

  details {
    margin-top: 8px;
  }

  summary {
    cursor: pointer;
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
