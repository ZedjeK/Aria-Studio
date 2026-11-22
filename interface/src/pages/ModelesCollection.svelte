<!--
  Modèles → Ma collection, comme en v2 : ce qui est installé, rangé par groupe, d'où ça vient et à quoi ça sert ; les
  steps à valider d'un checkpoint ; les mots déclencheurs et la force d'une LoRA ; la corbeille. Un modèle téléchargé
  par Aria part à la corbeille (rien n'est effacé) ; seul « Vider la corbeille », confirmé, efface pour de bon.
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import {
    collectionModeles,
    corbeilleModeles,
    etatSources,
    jeterModele,
    messageErreur,
    oublierReglageLora,
    reglerLora,
    relireLora,
    restaurerModele,
    retrouverSources,
    validerStepsModele,
    viderCorbeilleModeles,
    type Collection,
    type CorbeilleModeles,
    type ElementCollection,
    type IdentificationSources,
  } from '../lib/api';
  import { octetsLisibles } from '../lib/jauge';
  import {
    LONGUEUR_DECLENCHEURS,
    STEPS_MAX,
    dateLisible,
    erreurLora,
    erreurSteps,
    libelleOrigine,
    stepsProposes,
    texteIdentification,
  } from '../lib/modeles';
  import ASavoir from '../lib/composants/ASavoir.svelte';
  import Bouton from '../lib/composants/Bouton.svelte';
  import Dialogue from '../lib/composants/Dialogue.svelte';
  import Panneau from '../lib/composants/Panneau.svelte';

  interface Props {
    signaler: (texte: string, erreur?: boolean) => void;
  }

  let { signaler }: Props = $props();

  let collection = $state<Collection | null>(null);
  let corbeille = $state<CorbeilleModeles | null>(null);
  /** Les steps saisis pour chaque checkpoint à valider, par clé. */
  let steps = $state<Record<string, number>>({});
  /** La LoRA en cours de réglage. */
  let lora = $state<{ cle: string; nom: string; declencheurs: string; force: number; source: string } | null>(null);
  /** Le modèle à mettre à la corbeille, en attente de confirmation. */
  let aJeter = $state<ElementCollection | null>(null);
  let viderDemande = $state(false);
  let occupe = $state(false);
  let identification = $state<IdentificationSources | null>(null);

  const groupes = $derived(collection?.groupes.filter((g) => g.elements.length > 0) ?? []);
  const erreurReglage = $derived(lora ? erreurLora(lora.declencheurs, lora.force) : null);

  async function relire() {
    try {
      const [c, k] = await Promise.all([collectionModeles(), corbeilleModeles()]);
      collection = c;
      corbeille = k;
      for (const g of c.groupes) {
        for (const e of g.elements) {
          if (e.aValider && steps[e.cle] === undefined) steps[e.cle] = stepsProposes(e.aValider) ?? 20;
        }
      }
    } catch (e) {
      signaler(messageErreur(e), true);
    }
  }

  /** Une action, puis la collection relue ; un seul clic à la fois. */
  async function agir(action: () => Promise<string>) {
    if (occupe) return;
    occupe = true;
    try {
      signaler(await action());
      await relire();
    } catch (e) {
      signaler(messageErreur(e), true);
    } finally {
      occupe = false;
    }
  }

  function valider(e: ElementCollection) {
    const n = steps[e.cle] ?? 0;
    const erreur = erreurSteps(n);
    if (erreur) {
      signaler(erreur, true);
      return;
    }
    void agir(async () => {
      await validerStepsModele(e.cle, n);
      return `${e.nom} : ${n} steps validés.`;
    });
  }

  function regler(e: ElementCollection) {
    if (!e.lora) return;
    lora = {
      cle: e.cle,
      nom: e.nom,
      declencheurs: e.lora.declencheurs,
      force: e.lora.force,
      source: e.lora.declencheursSource,
    };
  }

  function enregistrerLora() {
    const l = lora;
    if (!l || erreurReglage) return;
    lora = null;
    void agir(async () => {
      await reglerLora(l.cle, l.declencheurs, l.force);
      return `${l.nom} : réglage enregistré.`;
    });
  }

  function revenirALaSource() {
    const l = lora;
    if (!l) return;
    lora = null;
    void agir(async () => {
      await oublierReglageLora(l.cle);
      return `${l.nom} : mots et force de la source.`;
    });
  }

  function relireSurCivitai(e: ElementCollection) {
    void agir(async () => {
      await relireLora(e.cle);
      return `${e.nom} : mots et force relus sur Civitai.`;
    });
  }

  function jeter() {
    const e = aJeter;
    aJeter = null;
    if (!e) return;
    void agir(async () => {
      await jeterModele(e.cle);
      return `${e.nom} est dans la corbeille.`;
    });
  }

  function restaurer(cle: string, nom: string) {
    void agir(async () => {
      await restaurerModele(cle);
      return `${nom} est restauré.`;
    });
  }

  function vider() {
    viderDemande = false;
    void agir(async () => {
      const r = await viderCorbeilleModeles();
      return `${r.effaces} modèle${r.effaces > 1 ? 's' : ''} effacé${r.effaces > 1 ? 's' : ''}, ${octetsLisibles(r.liberes)} libérés.`;
    });
  }

  async function retrouver() {
    try {
      identification = await retrouverSources();
    } catch (e) {
      signaler(messageErreur(e), true);
    }
  }

  async function suivreSources() {
    if (identification?.etat !== 'en_cours') return;
    try {
      identification = await etatSources();
      if (identification.etat === 'termine') {
        signaler(`Sources : ${texteIdentification(identification)}.`);
        await relire();
      }
    } catch (e) {
      signaler(messageErreur(e), true);
    }
  }

  onMount(() => {
    void relire();
    void etatSources()
      .then((i) => (identification = i))
      .catch((e: unknown) => signaler(messageErreur(e), true));
    const minuterie = setInterval(() => void suivreSources(), 1000);
    return () => clearInterval(minuterie);
  });
</script>

<ASavoir>
  Tout ce qui est dans le dossier des modèles, téléchargé par Aria ou déposé à la main. Seuls les modèles téléchargés
  par Aria se suppriment d'ici : ils partent dans la corbeille, d'où ils se restaurent ; « Vider la corbeille » les
  efface pour de bon.
</ASavoir>

{#if collection}
  {#if collection.orphelins > 0 || identification?.etat === 'en_cours'}
    <div class="sources">
      <p class="discret">
        {collection.orphelins} fichier{collection.orphelins > 1 ? 's' : ''} déposé{collection.orphelins > 1 ? 's' : ''}
        à la main : Aria les utilise, mais ne les supprime pas. « Retrouver les sources » calcule leur empreinte et la
        cherche dans le catalogue d'Aria puis sur Civitai, pour connaître leur page et leur licence.
      </p>
      <Bouton petit desactive={identification?.etat === 'en_cours'} onclick={() => void retrouver()}>
        Retrouver les sources
      </Bouton>
    </div>
  {/if}
  {#if identification && identification.etat !== 'repos'}
    <p class="discret">{texteIdentification(identification)}</p>
    {#if identification.etat === 'termine'}
      {#each identification.trouves as t (t.nom)}
        <p class="discret">✓ {t.nom} · {t.ou}</p>
      {/each}
      {#each identification.erreurs as e (e.nom)}
        <p class="alerte">{e.nom} : {e.erreur}</p>
      {/each}
    {/if}
  {/if}
  {#each groupes as g (g.id)}
    <Panneau titre={`${g.nom} · ${octetsLisibles(g.taille)}`}>
      <ul class="elements">
        {#each g.elements as e (e.cle)}
          <li>
            <div class="texte">
              <strong>{e.nom}</strong>
              <span class="discret">
                {e.famille ? `${e.famille} · ` : ''}{octetsLisibles(e.taille)} · {libelleOrigine(e.origine)}{e.licence
                  ? ` · licence ${e.licence}`
                  : ''}
              </span>
              {#if e.utilisePar.length > 0}
                <span class="discret">Utilisé par : {e.utilisePar.join(', ')}</span>
              {/if}
              {#if e.declencheurs.length > 0}
                <span class="discret">Mots déclencheurs de la source : {e.declencheurs.join(', ')}</span>
              {/if}
              {#if e.page}
                <span class="discret code">{e.page}</span>
              {/if}
              {#if e.steps !== null}
                <span class="discret">{e.steps} steps</span>
              {/if}
              {#if e.lora}
                <span class="discret">
                  Force {e.lora.force}{e.lora.declencheurs ? ` · « ${e.lora.declencheurs} »` : ''}{e.lora.regle
                    ? ' · réglé par toi'
                    : ''}
                </span>
              {/if}
              {#if e.aValider}
                <div class="valider">
                  <span class="alerte">Steps à valider : ce checkpoint n'est proposé nulle part avant.</span>
                  {#each e.aValider as d, i (i)}
                    <span class="discret">{String(d.valeur)} : {d.origine}{d.source ? ` (${d.source})` : ''}</span>
                  {/each}
                  <label>
                    Steps
                    <input type="number" min="1" max={STEPS_MAX} step="1" bind:value={steps[e.cle]} />
                  </label>
                  <Bouton petit desactive={occupe} onclick={() => valider(e)}>Valider</Bouton>
                </div>
              {/if}
            </div>
            <div class="actions">
              {#if e.lora}
                <Bouton genre="discret" petit desactive={occupe} onclick={() => regler(e)}>Régler</Bouton>
                {#if e.lora.civitai}
                  <Bouton genre="discret" petit desactive={occupe} onclick={() => relireSurCivitai(e)}>Relire sur Civitai</Bouton>
                {/if}
              {/if}
              {#if e.supprimable}
                <Bouton genre="discret" petit desactive={occupe} onclick={() => (aJeter = e)}>Corbeille</Bouton>
              {/if}
            </div>
          </li>
        {/each}
      </ul>
    </Panneau>
  {:else}
    <p class="discret">Aucun modèle installé pour l'instant.</p>
  {/each}

  {#if corbeille && corbeille.modeles.length > 0}
    <Panneau titre={`Corbeille · ${octetsLisibles(corbeille.taille)}`}>
      <ul class="elements">
        {#each corbeille.modeles as m (m.cle)}
          <li>
            <div class="texte">
              <strong>{m.nom}</strong>
              <span class="discret">
                {m.present ? octetsLisibles(m.taille) : 'fichier effacé à la main'}{m.date
                  ? ` · le ${dateLisible(m.date)}`
                  : ''}
              </span>
              {#if m.ailleurs}
                <span class="discret">Dans l'ancien dossier des modèles : {m.dossierModeles}</span>
              {/if}
            </div>
            <Bouton
              genre="discret"
              petit
              desactive={occupe || m.ailleurs || !m.present}
              onclick={() => restaurer(m.cle, m.nom)}
            >
              Restaurer
            </Bouton>
          </li>
        {/each}
      </ul>
      <div class="actions">
        <Bouton petit desactive={occupe} onclick={() => (viderDemande = true)}>Vider la corbeille</Bouton>
      </div>
    </Panneau>
  {/if}
{:else}
  <p class="discret">Lecture…</p>
{/if}

{#if lora}
  <Dialogue titre={`Régler ${lora.nom}`} fermer={() => (lora = null)}>
    <label class="champ">
      Mots déclencheurs (ajoutés au prompt)
      <textarea rows="3" maxlength={LONGUEUR_DECLENCHEURS} bind:value={lora.declencheurs}></textarea>
    </label>
    {#if lora.source}
      <p class="discret">Ceux de la source : {lora.source}</p>
    {/if}
    <label class="champ">
      Force (0 à 2)
      <input type="number" min="0" max="2" step="0.05" bind:value={lora.force} />
    </label>
    {#if erreurReglage}
      <p class="alerte">{erreurReglage}</p>
    {/if}
    {#snippet pied()}
      <Bouton genre="discret" onclick={revenirALaSource}>Revenir à la source</Bouton>
      <Bouton genre="principal" desactive={erreurReglage !== null} onclick={enregistrerLora}>Enregistrer</Bouton>
    {/snippet}
  </Dialogue>
{/if}

{#if aJeter}
  <Dialogue titre={`Mettre ${aJeter.nom} à la corbeille ?`} fermer={() => (aJeter = null)}>
    <p>
      {octetsLisibles(aJeter.taille)} quittent le dossier des modèles pour sa corbeille : rien n'est effacé, il se
      restaure tant que la corbeille n'est pas vidée.
    </p>
    {#if aJeter.utilisePar.length > 0}
      <p class="alerte">Utilisé par : {aJeter.utilisePar.join(', ')}.</p>
    {/if}
    {#snippet pied()}
      <Bouton genre="discret" onclick={() => (aJeter = null)}>Annuler</Bouton>
      <Bouton genre="principal" onclick={jeter}>Mettre à la corbeille</Bouton>
    {/snippet}
  </Dialogue>
{/if}

{#if viderDemande && corbeille}
  <Dialogue titre="Vider la corbeille des modèles ?" fermer={() => (viderDemande = false)}>
    <p>
      {corbeille.modeles.length} modèle{corbeille.modeles.length > 1 ? 's' : ''}, {octetsLisibles(corbeille.taille)} :
      effacés pour de bon, sans retour possible.
    </p>
    {#snippet pied()}
      <Bouton genre="discret" onclick={() => (viderDemande = false)}>Annuler</Bouton>
      <Bouton genre="principal" onclick={vider}>Effacer pour de bon</Bouton>
    {/snippet}
  </Dialogue>
{/if}

<style>
  .elements {
    display: flex;
    flex-direction: column;
    gap: 12px;
    margin: 8px 0;
    padding: 0;
    list-style: none;
  }

  .elements li {
    display: flex;
    align-items: flex-start;
    gap: 12px;
  }

  .texte {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  .code {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .valider {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-top: 6px;
  }

  .valider label,
  .champ {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 12px;
    color: var(--texte-2);
  }

  .valider input {
    width: 8em;
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px;
  }

  .alerte {
    color: var(--orange);
  }

  .sources {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 12px;
  }

  .sources p {
    flex: 1;
    min-width: 260px;
  }
</style>
