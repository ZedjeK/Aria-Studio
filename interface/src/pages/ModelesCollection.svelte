<!--
  Modèles → Ma collection, comme la maquette : le dossier des modèles (chemin, place libre, retrouver les sources), ce
  qui est installé rangé par groupe, d'où ça vient et à quoi ça sert ; les steps à valider d'un checkpoint déposé à la
  main (le seul bouton orange de l'onglet, sur le premier à valider) ; la force d'une LoRA sur sa ligne, ses mots
  déclencheurs dans un dialogue ; la corbeille, toujours visible. Chaque modèle, téléchargé par Aria ou déposé à la
  main, part à la corbeille (rien n'est effacé) ; seul « Vider », confirmé, efface pour de bon.
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import {
    choisirDossierModeles,
    collectionModeles,
    corbeilleModeles,
    etatEmplacements,
    etatSources,
    jeterModele,
    messageErreur,
    oublierReglageLora,
    ouvrirDossier,
    parcourirDossier,
    reglerLora,
    relireLora,
    restaurerModele,
    retrouverSources,
    validerStepsModele,
    viderCorbeilleModeles,
    type Collection,
    type CorbeilleModeles,
    type ElementCollection,
    type EtatLieux,
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
  let lieux = $state<EtatLieux | null>(null);
  /** Les steps saisis pour chaque checkpoint à valider, par clé. */
  let steps = $state<Record<string, number>>({});
  /** La force saisie pour chaque LoRA, par clé. */
  let forces = $state<Record<string, number>>({});
  /** La LoRA en cours de réglage (mots déclencheurs). */
  let lora = $state<{ cle: string; nom: string; declencheurs: string; force: number; source: string } | null>(null);
  /** Le modèle à mettre à la corbeille, en attente de confirmation. */
  let aJeter = $state<ElementCollection | null>(null);
  let viderDemande = $state(false);
  let occupe = $state(false);
  let identification = $state<IdentificationSources | null>(null);

  const groupes = $derived(collection?.groupes.filter((g) => g.elements.length > 0) ?? []);
  const erreurReglage = $derived(lora ? erreurLora(lora.declencheurs, lora.force) : null);
  /** Le premier checkpoint à valider : son groupe est accentué, son bouton est l'orange de l'onglet. */
  const premierAValider = $derived(groupes.flatMap((g) => g.elements).find((e) => e.aValider !== null)?.cle ?? null);
  const dossier = $derived(lieux?.dossiers.find((d) => d.cle === 'modeles')?.chemin ?? lieux?.modelesProchain ?? '');
  const libre = $derived(lieux?.libres['modeles'] ?? null);

  async function relire() {
    try {
      const [c, k, l] = await Promise.all([collectionModeles(), corbeilleModeles(), etatEmplacements()]);
      collection = c;
      corbeille = k;
      lieux = l;
      for (const g of c.groupes) {
        for (const e of g.elements) {
          if (e.aValider && steps[e.cle] === undefined) steps[e.cle] = stepsProposes(e.aValider) ?? 20;
          if (e.lora) forces[e.cle] = e.lora.force;
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

  /** Enregistre la force saisie sur la ligne, avec les mots déjà réglés. */
  function enregistrerForce(e: ElementCollection) {
    if (!e.lora) return;
    const force = forces[e.cle] ?? e.lora.force;
    const mots = e.lora.declencheurs;
    const erreur = erreurLora(mots, force);
    if (erreur) {
      signaler(erreur, true);
      return;
    }
    void agir(async () => {
      await reglerLora(e.cle, mots, force);
      return `${e.nom} : force ${force} enregistrée.`;
    });
  }

  function regler(e: ElementCollection) {
    if (!e.lora) return;
    lora = {
      cle: e.cle,
      nom: e.nom,
      declencheurs: e.lora.declencheurs,
      force: forces[e.cle] ?? e.lora.force,
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

  async function ouvrir() {
    try {
      await ouvrirDossier('modeles');
    } catch (e) {
      signaler(messageErreur(e), true);
    }
  }

  async function changer() {
    try {
      const choisi = await parcourirDossier("Dossier des modèles d'Aria Studio", lieux?.modelesProchain ?? null);
      if (!choisi) return;
      lieux = await choisirDossierModeles(choisi);
      signaler(
        lieux.redemarrage ? 'Dossier des modèles enregistré : il vaudra au prochain lancement.' : 'Dossier des modèles enregistré.',
      );
    } catch (e) {
      signaler(messageErreur(e), true);
    }
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

{#if collection}
  <Panneau titre="Dossier des modèles">
    <div class="ligne">
      <span class="code chemin">{dossier}</span>
      {#if libre !== null}<span class="puce">{octetsLisibles(libre)} libres</span>{/if}
      <Bouton genre="discret" petit onclick={() => void ouvrir()}>📂 Ouvrir</Bouton>
      <Bouton genre="discret" petit onclick={() => void changer()}>Changer…</Bouton>
      <Bouton genre="secondaire" petit desactive={identification?.etat === 'en_cours'} onclick={() => void retrouver()}>
        🔍 Retrouver les sources
      </Bouton>
    </div>
    {#if lieux?.redemarrage}
      <p class="discret">Nouveau dossier : {lieux.modelesProchain}, pris en compte au prochain lancement.</p>
    {/if}
    {#if collection.orphelins > 0}
      <p class="discret">
        {collection.orphelins} fichier{collection.orphelins > 1 ? 's' : ''} déposé{collection.orphelins > 1 ? 's' : ''} à
        la main : ils se suppriment comme les autres (corbeille, rien n'est effacé). « Retrouver les sources » calcule
        leur empreinte et la cherche dans le catalogue d'Aria puis sur Civitai, pour connaître leur page et leur
        licence.
      </p>
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
  </Panneau>

  {#each groupes as g (g.id)}
    <Panneau
      titre={`${g.nom} · ${g.elements.length} · ${octetsLisibles(g.taille)}`}
      accentue={g.elements.some((e) => e.cle === premierAValider)}
    >
      <ul class="elements">
        {#each g.elements as e (e.cle)}
          <li>
            <div class="texte">
              <div class="ligne">
                <strong>{e.nom}</strong>
                <span class="discret">· {octetsLisibles(e.taille)}</span>
                <span class="puce" class:neutre={e.origine !== 'aria'}>{libelleOrigine(e.origine)}</span>
                {#if e.famille}<span class="puce neutre">{e.famille}</span>{/if}
                {#if e.licence}<span class="puce neutre">licence {e.licence}</span>{/if}
                {#if e.steps !== null}<span class="puce neutre">{e.steps} steps</span>{/if}
                {#if e.aValider}<span class="puce ambre">Steps à valider</span>{/if}
              </div>
              {#if e.utilisePar.length > 0}
                <span class="discret">Utilisé par : {e.utilisePar.join(', ')}</span>
              {/if}
              {#if e.declencheurs.length > 0}
                <span class="discret">Mots déclencheurs de la source : {e.declencheurs.join(', ')}</span>
              {/if}
              {#if e.page}
                <span class="discret code">{e.page}</span>
              {/if}
              {#if e.lora}
                <div class="ligne">
                  {#if e.lora.declencheurs}<span class="puce neutre">« {e.lora.declencheurs} »</span>{/if}
                  <label class="discret">
                    Force
                    <input type="number" min="0" max="2" step="0.05" bind:value={forces[e.cle]} aria-label={`Force de ${e.nom}`} />
                  </label>
                  <Bouton genre="discret" petit desactive={occupe} onclick={() => enregistrerForce(e)}>Enregistrer</Bouton>
                  <Bouton genre="discret" petit desactive={occupe} onclick={() => regler(e)}>Mots déclencheurs…</Bouton>
                  {#if e.lora.civitai}
                    <Bouton genre="discret" petit desactive={occupe} onclick={() => relireSurCivitai(e)}>🔄 Civitai</Bouton>
                  {/if}
                  {#if e.lora.regle}<span class="discret">réglé par toi</span>{/if}
                </div>
              {/if}
              {#if e.aValider}
                <p class="alerte">Steps à valider : ce checkpoint n'est proposé nulle part avant.</p>
                {#each e.aValider as d, i (i)}
                  <span class="discret">{String(d.valeur)} : {d.origine}{d.source ? ` (${d.source})` : ''}</span>
                {/each}
                <div class="ligne">
                  <label class="discret">
                    Steps
                    <input type="number" min="1" max={STEPS_MAX} step="1" bind:value={steps[e.cle]} />
                  </label>
                  <Bouton
                    genre={e.cle === premierAValider ? 'principal' : 'secondaire'}
                    petit
                    desactive={occupe}
                    onclick={() => valider(e)}
                  >
                    Valider et mettre en service
                  </Bouton>
                </div>
              {/if}
            </div>
            <Bouton genre="discret" petit desactive={occupe} onclick={() => (aJeter = e)}>Supprimer</Bouton>
          </li>
        {/each}
      </ul>
    </Panneau>
  {:else}
    <p class="discret">Aucun modèle installé pour l'instant.</p>
  {/each}

  <Panneau
    titre={`Corbeille des modèles · ${corbeille?.modeles.length ?? 0}${corbeille && corbeille.modeles.length > 0 ? ` · ${octetsLisibles(corbeille.taille)}` : ''}`}
  >
    {#if corbeille && corbeille.modeles.length > 0}
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
      <div class="ligne">
        <Bouton genre="secondaire" petit desactive={occupe} onclick={() => (viderDemande = true)}>Vider la corbeille</Bouton>
      </div>
    {:else}
      <p class="discret">Rien pour l'instant.</p>
    {/if}
  </Panneau>
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
  <Dialogue titre={`Supprimer ${aJeter.nom} ?`} fermer={() => (aJeter = null)}>
    <p>
      {octetsLisibles(aJeter.taille)} quittent le dossier des modèles pour sa corbeille : rien n'est effacé, il se
      restaure tant que la corbeille n'est pas vidée.
    </p>
    {#if aJeter.origine === 'main'}
      <p class="discret">Déposé à la main : il revient tel quel à la restauration.</p>
    {/if}
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
  <Dialogue titre="Vider la corbeille ?" fermer={() => (viderDemande = false)}>
    <ASavoir>Les éléments et leurs fiches sont supprimés pour de bon. Rien ne les ramène.</ASavoir>
    <p>{corbeille.modeles.length} modèle{corbeille.modeles.length > 1 ? 's' : ''}, {octetsLisibles(corbeille.taille)}.</p>
    {#snippet pied()}
      <Bouton genre="discret" onclick={() => (viderDemande = false)}>Annuler</Bouton>
      <Bouton genre="principal" onclick={vider}>Vider</Bouton>
    {/snippet}
  </Dialogue>
{/if}

<style>
  .elements {
    display: flex;
    flex-direction: column;
    gap: 14px;
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
    gap: 4px;
    min-width: 0;
  }

  .ligne {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }

  .chemin {
    flex: 1;
    min-width: 220px;
    overflow-wrap: anywhere;
  }

  .code {
    overflow: hidden;
    text-overflow: ellipsis;
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

  label.discret {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
  }

  label.discret input {
    width: 6em;
  }

  .champ {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 12px;
    color: var(--texte-2);
  }

  .alerte {
    color: var(--orange);
  }
</style>
