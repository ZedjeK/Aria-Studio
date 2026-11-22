<!--
  Réglages → Emplacements et clés, comme en v2 : les dossiers d'Aria (ouverts dans l'Explorateur), le dossier des
  modèles (pris en compte au prochain lancement, rien n'est déplacé), et les clés Civitai et Hugging Face, enregistrées
  chiffrées et testées auprès de leur source. Une clé ne revient jamais vers l'interface.
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import {
    choisirDossierModeles,
    enregistrerCle,
    etatCles,
    etatEmplacements,
    messageErreur,
    ouvrirDossier,
    parcourirDossier,
    supprimerCle,
    testerCle,
    type EtatCle,
    type EtatLieux,
  } from '../lib/api';
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

  const ORIGINES = { aria: 'Enregistrée dans Aria (chiffrée)', variable: "Variable d'environnement" };

  async function relire() {
    try {
      [lieux, cles] = await Promise.all([etatEmplacements(), etatCles()]);
      chemin = lieux.modelesChoisi ?? '';
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
  <Panneau titre="Dossiers">
    <table>
      <tbody>
        {#each lieux.dossiers as d (d.cle)}
          <tr>
            <td>{d.nom}</td>
            <td class="code chemin">{d.chemin}</td>
            <td class="discret">{lieux.libres[d.cle] != null ? `${octetsLisibles(lieux.libres[d.cle] ?? 0)} libres` : ''}</td>
            <td><Bouton genre="discret" petit onclick={() => void ouvrir(d.cle)}>Ouvrir</Bouton></td>
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

<Panneau titre="Clés des sources de modèles">
  {#each cles as c (c.source)}
    <div class="cle">
      <p>
        <strong>{c.nom}</strong>
        <span class="discret"> · {c.origine ? ORIGINES[c.origine] : 'aucune clé'}</span>
      </p>
      <p class="discret">{c.aide}. Repli : la variable d'environnement {c.variable}.</p>
      <div class="ligne">
        <input
          type="password"
          autocomplete="off"
          placeholder="Colle la clé ici"
          aria-label={`Clé ${c.nom}`}
          bind:value={saisies[c.source]}
        />
        <Bouton petit desactive={(saisies[c.source] ?? '').trim() === ''} onclick={() => void enregistrer(c.source)}>Enregistrer</Bouton>
        <Bouton genre="discret" petit desactive={c.origine === null} onclick={() => void tester(c.source)}>Tester</Bouton>
        {#if c.origine === 'aria'}
          <Bouton genre="discret" petit onclick={() => (aSupprimer = c)}>Retirer</Bouton>
        {/if}
      </div>
      {#if tests[c.source]}
        <p class:bon={tests[c.source]?.ok} class:ko={!tests[c.source]?.ok}>{tests[c.source]?.message}</p>
      {/if}
    </div>
  {/each}
</Panneau>

{#if aSupprimer}
  <Dialogue titre={`Retirer la clé ${aSupprimer.nom} ?`} fermer={() => (aSupprimer = null)}>
    <p>
      Elle est effacée du dossier de données ; il faudra la recoller pour télécharger depuis {aSupprimer.nom}. Celle que
      la version 2 gardait dans le Gestionnaire d'identification de Windows reste en place.
    </p>
    {#snippet pied()}
      <Bouton genre="discret" onclick={() => (aSupprimer = null)}>Annuler</Bouton>
      <Bouton genre="principal" onclick={() => void supprimer()}>Retirer</Bouton>
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
