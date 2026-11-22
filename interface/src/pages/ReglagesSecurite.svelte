<!--
  Réglages → Sécurité, comme en v2 : les listes du filtre AS-18 (protection des mineurs). Le socle livré se lit ; on y
  ajoute ses propres termes et ses phrases témoins, et on retire les siens. Rien ici ne retire ni ne modifie un terme du
  socle. Un ajout qui ferait bloquer une phrase témoin « doit passer » est d'abord à confirmer.
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import {
    ajouterTemoinSecurite,
    ajouterTermeSecurite,
    essayerSecurite,
    etatSecurite,
    messageErreur,
    retirerTemoinSecurite,
    retirerTermeSecurite,
    type EssaiSecurite,
    type EtatSecurite,
  } from '../lib/api';
  import ASavoir from '../lib/composants/ASavoir.svelte';
  import Bouton from '../lib/composants/Bouton.svelte';
  import Dialogue from '../lib/composants/Dialogue.svelte';
  import Panneau from '../lib/composants/Panneau.svelte';

  interface Props {
    signaler: (texte: string, erreur?: boolean) => void;
  }

  let { signaler }: Props = $props();

  let etat = $state<EtatSecurite | null>(null);
  let phrase = $state('');
  let essai = $state<EssaiSecurite | null>(null);
  let saisies = $state<Record<string, string>>({});
  let confirmation = $state<{ liste: string; terme: string; message: string } | null>(null);

  const echecs = $derived(etat?.temoins.filter((t) => !t.ok).length ?? 0);

  async function relire() {
    try {
      etat = await etatSecurite();
    } catch (e) {
      signaler(messageErreur(e), true);
    }
  }

  async function essayer() {
    try {
      essai = await essayerSecurite(phrase);
    } catch (e) {
      signaler(messageErreur(e), true);
    }
  }

  async function ajouter(liste: string, forcer = false) {
    const terme = forcer ? (confirmation?.terme ?? '') : (saisies[liste] ?? '');
    try {
      const reponse = await ajouterTermeSecurite(liste, terme, forcer);
      if (reponse.aConfirmer) {
        confirmation = { liste, terme, message: reponse.aConfirmer };
        return;
      }
      if (reponse.etat) etat = reponse.etat;
      saisies[liste] = '';
      confirmation = null;
      signaler(`« ${terme.trim()} » ajouté.`);
    } catch (e) {
      signaler(messageErreur(e), true);
    }
  }

  async function retirer(liste: string, terme: string) {
    try {
      etat = await retirerTermeSecurite(liste, terme);
      signaler(`« ${terme} » retiré de tes ajouts.`);
    } catch (e) {
      signaler(messageErreur(e), true);
    }
  }

  async function garderTemoin(attendu: 'bloque' | 'passe') {
    try {
      etat = await ajouterTemoinSecurite(phrase, attendu);
      signaler(attendu === 'bloque' ? 'Phrase témoin gardée : elle doit bloquer.' : 'Phrase témoin gardée : elle doit passer.');
    } catch (e) {
      signaler(messageErreur(e), true);
    }
  }

  async function retirerTemoin(texte: string) {
    try {
      etat = await retirerTemoinSecurite(texte);
      signaler('Phrase témoin retirée.');
    } catch (e) {
      signaler(messageErreur(e), true);
    }
  }

  onMount(() => void relire());
</script>

<ASavoir>
  Le filtre AS-18 refuse toute demande qui associe un personnage mineur à un terme sensible, et les termes toujours
  bloqués partout. Le socle livré avec Aria ne se retire pas ; tes ajouts s'y ajoutent, dans reglages.json. Les phrases
  témoins sont rejouées à chaque changement.
</ASavoir>

{#if etat?.erreur}
  <p class="ko" role="alert">{etat.erreur}. Tant que ce n'est pas corrigé, toute génération est refusée.</p>
{/if}

<Panneau titre="Essayer une phrase">
  <textarea rows="2" bind:value={phrase} placeholder="Une idée ou un prompt, pour voir ce que le filtre en dit"></textarea>
  <div class="ligne">
    <Bouton petit desactive={phrase.trim() === ''} onclick={() => void essayer()}>Essayer</Bouton>
    {#if essai}
      <span class:ko={essai.bloque} class:bon={!essai.bloque}>
        {essai.bloque ? `Bloquée : ${essai.termes.map((t) => t.terme).join(', ')}` : 'Elle passe.'}
      </span>
      <Bouton genre="discret" petit desactive={phrase.trim() === ''} onclick={() => void garderTemoin('bloque')}>
        Garder : doit bloquer
      </Bouton>
      <Bouton genre="discret" petit desactive={phrase.trim() === ''} onclick={() => void garderTemoin('passe')}>
        Garder : doit passer
      </Bouton>
    {/if}
  </div>
</Panneau>

{#each etat?.listes ?? [] as liste (liste.cle)}
  <Panneau titre={liste.nom}>
    <details>
      <summary class="discret">Socle livré : {liste.socle.length} termes</summary>
      <p class="socle code">{liste.socle.join(' · ')}</p>
    </details>
    <p class="etiquette">Tes ajouts</p>
    {#if liste.ajouts.length === 0}
      <p class="discret">Aucun.</p>
    {:else}
      <ul class="ajouts">
        {#each liste.ajouts as terme (terme)}
          <li>
            <span>{terme}</span>
            <button type="button" class="retirer" aria-label={`Retirer ${terme}`} onclick={() => void retirer(liste.cle, terme)}>✕</button>
          </li>
        {/each}
      </ul>
    {/if}
    <div class="ligne">
      <input
        type="text"
        aria-label={`Ajouter à : ${liste.nom}`}
        placeholder={`Un terme (${etat?.longueurMin ?? 3} lettres au moins)`}
        bind:value={saisies[liste.cle]}
      />
      <Bouton petit desactive={(saisies[liste.cle] ?? '').trim() === ''} onclick={() => void ajouter(liste.cle)}>Ajouter</Bouton>
    </div>
  </Panneau>
{/each}

{#if etat && etat.temoins.length > 0}
  <Panneau titre="Phrases témoins">
    <p class:ko={echecs > 0} class="discret">
      {echecs === 0 ? 'Toutes donnent le résultat attendu.' : `${echecs} ne donne${echecs > 1 ? 'nt' : ''} pas le résultat attendu.`}
    </p>
    <table>
      <thead><tr><th>Phrase</th><th>Attendu</th><th>Obtenu</th><th></th></tr></thead>
      <tbody>
        {#each etat.temoins as t, i (i)}
          <tr class:ko={!t.ok}>
            <td>{t.texte}{#if t.livre}<span class="discret"> · livrée</span>{/if}</td>
            <td>{t.attendu === 'bloque' ? 'bloque' : 'passe'}</td>
            <td>{t.obtenu === 'bloque' ? 'bloque' : 'passe'}</td>
            <td>
              {#if !t.livre}
                <button type="button" class="retirer" aria-label="Retirer cette phrase témoin" onclick={() => void retirerTemoin(t.texte)}>✕</button>
              {/if}
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  </Panneau>
{/if}

{#if confirmation}
  <Dialogue titre="Ajouter quand même ?" fermer={() => (confirmation = null)}>
    <p>{confirmation.message}.</p>
    {#snippet pied()}
      <Bouton genre="discret" onclick={() => (confirmation = null)}>Annuler</Bouton>
      <Bouton genre="principal" onclick={() => void ajouter(confirmation?.liste ?? '', true)}>Ajouter quand même</Bouton>
    {/snippet}
  </Dialogue>
{/if}

<style>
  textarea,
  input[type='text'] {
    font: inherit;
    color: var(--texte);
    background: var(--fond);
    border: 1px solid var(--ligne);
    padding: 6px 8px;
  }

  textarea {
    width: 100%;
    resize: vertical;
  }

  .ligne {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 12px;
    margin-top: 10px;
  }

  .etiquette {
    margin: 12px 0 6px;
    color: var(--texte-2);
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.14em;
    text-transform: uppercase;
  }

  .socle {
    max-height: 140px;
    overflow-y: auto;
    font-size: 12px;
    color: var(--texte-2);
  }

  .ajouts {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .ajouts li {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 2px 8px;
    border: 1px solid var(--ligne);
  }

  .retirer {
    font: inherit;
    color: var(--texte-2);
    background: transparent;
    border: none;
    cursor: pointer;
  }

  .ko {
    color: var(--rouge);
  }

  .bon {
    color: var(--cyan);
  }

  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 13px;
  }

  th,
  td {
    padding: 4px 6px;
    text-align: left;
    border-bottom: 1px solid var(--ligne);
  }
</style>
