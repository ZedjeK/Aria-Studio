<!--
  Réglages → Emplacements et clés, comme en v2 : les dossiers d'Aria (ouverts dans l'Explorateur), le dossier des
  modèles (pris en compte au prochain lancement, rien n'est déplacé), et les clés Civitai et Hugging Face, enregistrées
  chiffrées et testées auprès de leur source. Une clé ne revient jamais vers l'interface.
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import {
    annulerRepriseV2,
    choisirDossierModeles,
    enregistrerCle,
    etatCles,
    etatEmplacements,
    etatRepriseV2,
    messageErreur,
    ouvrirDossier,
    parcourirDossier,
    supprimerCle,
    testerCle,
    type EtatCle,
    type EtatLieux,
    type EtatReprise,
  } from '../lib/api';
  import { enVigueur } from '../lib/reprise';
  import { octetsLisibles } from '../lib/jauge';
  import ASavoir from '../lib/composants/ASavoir.svelte';
  import Bouton from '../lib/composants/Bouton.svelte';
  import Dialogue from '../lib/composants/Dialogue.svelte';
  import Panneau from '../lib/composants/Panneau.svelte';

  interface Props {
    signaler: (texte: string, erreur?: boolean) => void;
  }

  let { signaler }: Props = $props();

  let lieux = $state<EtatLieux | null>(null);
  let cles = $state<EtatCle[]>([]);
  let chemin = $state('');
  let saisies = $state<Record<string, string>>({});
  let tests = $state<Record<string, { ok: boolean; message: string }>>({});
  let aSupprimer = $state<EtatCle | null>(null);
  /** La reprise d'une v2 pour cet essai portable, s'il y en a eu une. */
  let reprise = $state<EtatReprise | null>(null);
  let confirmerAbandon = $state(false);
  /** Les dossiers de la v2 ne valent plus qu'au lancement en cours. */
  let abandonnee = $state(false);

  /** La pastille d'une clé : où elle est lue, ou « aucune ». */
  const ORIGINES = { aria: 'enregistrée', variable: "variable d'environnement" };

  async function relire() {
    try {
      [lieux, cles, reprise] = await Promise.all([etatEmplacements(), etatCles(), etatRepriseV2()]);
      chemin = lieux.modelesChoisi ?? '';
    } catch (e) {
      signaler(messageErreur(e), true);
    }
  }

  async function abandonnerV2() {
    confirmerAbandon = false;
    try {
      const retires = await annulerRepriseV2();
      abandonnee = retires.length > 0;
      signaler(
        retires.length > 0
          ? "Dossiers de la version 2 laissés : l'essai reprend les siens au prochain lancement."
          : 'Plus rien de la version 2 n’était utilisé.',
      );
      await relire();
    } catch (e) {
      signaler(messageErreur(e), true);
    }
  }

  async function ouvrir(cle: string) {
    try {
      await ouvrirDossier(cle);
    } catch (e) {
      signaler(messageErreur(e), true);
    }
  }

  async function parcourir() {
    try {
      const choisi = await parcourirDossier("Dossier des modèles d'Aria Studio", chemin || lieux?.modelesProchain || null);
      if (choisi) chemin = choisi;
    } catch (e) {
      signaler(messageErreur(e), true);
    }
  }

  async function choisir(nouveau: string | null) {
    try {
      lieux = await choisirDossierModeles(nouveau);
      chemin = lieux.modelesChoisi ?? '';
      signaler(lieux.redemarrage ? 'Dossier des modèles enregistré : il vaudra au prochain lancement.' : 'Dossier des modèles enregistré.');
    } catch (e) {
      signaler(messageErreur(e), true);
    }
  }

  async function enregistrer(source: string) {
    try {
      cles = await enregistrerCle(source, saisies[source] ?? '');
      saisies[source] = '';
      signaler('Clé enregistrée, chiffrée dans le dossier de données.');
    } catch (e) {
      signaler(messageErreur(e), true);
    }
  }

  async function tester(source: string) {
    try {
      tests[source] = await testerCle(source);
    } catch (e) {
      signaler(messageErreur(e), true);
    }
  }

  async function supprimer() {
    const source = aSupprimer?.source;
    aSupprimer = null;
    if (!source) return;
    try {
      cles = await supprimerCle(source);
      delete tests[source];
      signaler('Clé retirée.');
    } catch (e) {
      signaler(messageErreur(e), true);
    }
  }

  onMount(() => void relire());
</script>

<ASavoir>
  {lieux?.portable
    ? 'Mode portable : tout reste à côté de l’exe, les moteurs dans services.'
    : 'Tout vit dans le dossier d’Aria de ton profil ; le dossier des modèles peut aller ailleurs.'}
  Les clés sont chiffrées par Windows pour ton compte, et ne quittent jamais l'appli.
</ASavoir>

{#if lieux}
  <Panneau titre="Emplacements">
    <table>
      <tbody>
        {#each lieux.dossiers as d (d.cle)}
          <tr>
            <td><strong>{d.nom}</strong></td>
            <td class="code chemin">{d.chemin}</td>
            <td>
              {#if lieux.libres[d.cle] != null}<span class="puce">{octetsLisibles(lieux.libres[d.cle] ?? 0)} libres</span>{/if}
            </td>
            <td><Bouton genre="discret" petit onclick={() => void ouvrir(d.cle)}>📂 Ouvrir</Bouton></td>
          </tr>
        {/each}
      </tbody>
    </table>
  </Panneau>

  <Panneau titre="Dossier des modèles">
    <p class="discret">
      Par défaut : <span class="code">{lieux.modelesDefaut}</span>. Un autre dossier vaut au prochain lancement d'Aria ;
      les fichiers ne sont pas déplacés.
    </p>
    <div class="ligne">
      <input type="text" class="large" placeholder="D:\IA\Aria-modeles" bind:value={chemin} aria-label="Dossier des modèles" />
      <Bouton petit onclick={() => void parcourir()}>Parcourir…</Bouton>
      <Bouton petit desactive={chemin.trim() === '' || chemin === lieux.modelesChoisi} onclick={() => void choisir(chemin)}>Enregistrer</Bouton>
      {#if lieux.modelesChoisi}
        <Bouton genre="discret" petit onclick={() => void choisir(null)}>Revenir au dossier par défaut</Bouton>
      {/if}
    </div>
    {#if lieux.redemarrage}
      <p class="attention">Au prochain lancement : <span class="code">{lieux.modelesProchain}</span>.</p>
    {/if}
  </Panneau>
{/if}

{#if reprise?.bilan}
  {@const bilan = reprise.bilan}
  <Panneau titre="Repris de la version 2">
    <p class="discret">
      Au premier lancement de cet essai ({bilan.date.replace('T', ' ')}), Aria a repris ces dossiers de la version 2
      (<span class="code">{bilan.depuis}</span>), sans rien copier ni rien écrire chez elle.
    </p>
    <table>
      <tbody>
        {#each bilan.repris as e (e.cle)}
          <tr>
            <td>{e.nom}</td>
            <td class="code chemin">{e.chemin}</td>
            <td class="discret">{enVigueur(reprise, e.cle) ? 'utilisé' : 'plus utilisé'}</td>
          </tr>
        {/each}
      </tbody>
    </table>
    {#if bilan.enVigueur.length > 0}
      <div class="ligne">
        <Bouton genre="discret" petit onclick={() => (confirmerAbandon = true)}>Ne plus utiliser la version 2</Bouton>
      </div>
    {:else if abandonnee}
      <p class="attention">Au prochain lancement, l'essai reprend ses propres dossiers.</p>
    {/if}
  </Panneau>
{:else if reprise?.v2Lancee}
  <Panneau titre="Version 2">
    <p class="attention">
      Une version 2 tournait au lancement : rien n'a été repris. Ferme-la, puis relance cet essai pour reprendre ses
      modèles et ses moteurs.
    </p>
  </Panneau>
{/if}

<Panneau titre="🔑 Clés d'accès">
  {#each cles as c (c.source)}
    <div class="cle">
      <div class="ligne">
        <strong class="nom-cle">{c.nom}</strong>
        <input
          type="password"
          class="saisie"
          autocomplete="off"
          placeholder="Colle ta clé"
          aria-label={`Clé ${c.nom}`}
          bind:value={saisies[c.source]}
        />
        <span class="puce" class:cyan={c.origine !== null}>{c.origine ? ORIGINES[c.origine] : 'aucune'}</span>
        <Bouton petit desactive={(saisies[c.source] ?? '').trim() === ''} onclick={() => void enregistrer(c.source)}>Enregistrer</Bouton>
        <Bouton genre="discret" petit desactive={c.origine === null} onclick={() => void tester(c.source)}>Tester</Bouton>
        {#if c.origine === 'aria'}
          <Bouton genre="discret" petit onclick={() => (aSupprimer = c)}>Supprimer</Bouton>
        {/if}
      </div>
      <p class="discret">{c.aide}. Repli : la variable d'environnement {c.variable}.</p>
      {#if tests[c.source]}
        <p class:bon={tests[c.source]?.ok} class:ko={!tests[c.source]?.ok}>{tests[c.source]?.message}</p>
      {/if}
    </div>
  {/each}
  <p class="discret">Les clés sont chiffrées par Windows pour ton compte, dans le dossier de données : elles ne sont jamais renvoyées à l'interface.</p>
</Panneau>

{#if confirmerAbandon}
  <Dialogue titre="Ne plus utiliser la version 2 ?" fermer={() => (confirmerAbandon = false)}>
    <p>
      Au prochain lancement, l'essai reprend ses propres dossiers : ses modèles à côté de l'exe, et ses moteurs dans
      services (à installer depuis Réglages → Moteurs). Rien n'est supprimé, ni chez la version 2 ni ici, et la reprise
      ne se refera pas toute seule.
    </p>
    {#snippet pied()}
      <Bouton genre="discret" onclick={() => (confirmerAbandon = false)}>Annuler</Bouton>
      <Bouton genre="principal" onclick={() => void abandonnerV2()}>Ne plus l'utiliser</Bouton>
    {/snippet}
  </Dialogue>
{/if}

{#if aSupprimer}
  <Dialogue titre={`Supprimer la clé ${aSupprimer.nom} ?`} fermer={() => (aSupprimer = null)}>
    <p>
      Elle est effacée du dossier de données ; il faudra la recoller pour télécharger depuis {aSupprimer.nom}. Celle que
      la version 2 gardait dans le Gestionnaire d'identification de Windows reste en place.
    </p>
    {#snippet pied()}
      <Bouton genre="discret" onclick={() => (aSupprimer = null)}>Annuler</Bouton>
      <Bouton genre="principal" onclick={() => void supprimer()}>Supprimer</Bouton>
    {/snippet}
  </Dialogue>
{/if}

<style>
  table {
    width: 100%;
    border-collapse: collapse;
  }

  td {
    padding: 4px 6px;
    border-bottom: 1px solid var(--ligne);
    vertical-align: middle;
  }

  .chemin {
    font-size: 12px;
    word-break: break-all;
  }

  .ligne {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 12px;
    margin: 8px 0;
  }

  input[type='text'],
  input[type='password'] {
    font: inherit;
    color: var(--texte);
    background: var(--fond);
    border: 1px solid var(--ligne);
    padding: 6px 8px;
  }

  .large {
    flex: 1;
    min-width: 260px;
    font-family: var(--police-code);
  }

  .puce {
    padding: 1px 8px;
    border: 1px solid var(--ligne);
    color: var(--texte-2);
    font-size: 11.5px;
    letter-spacing: 0.04em;
    white-space: nowrap;
  }

  .puce.cyan {
    border-color: var(--cyan);
    color: var(--cyan);
  }

  .nom-cle {
    min-width: 110px;
  }

  .saisie {
    flex: 1;
    min-width: 220px;
  }

  .cle {
    padding: 10px 0;
    border-bottom: 1px solid var(--ligne);
  }

  .cle:last-child {
    border-bottom: none;
  }

  .cle p {
    margin: 2px 0;
  }

  .attention {
    color: var(--ambre);
  }

  .bon {
    color: var(--cyan);
  }

  .ko {
    color: var(--rouge);
  }
</style>
