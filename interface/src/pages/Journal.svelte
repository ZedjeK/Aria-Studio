<!--
  Le journal de l'appli : ses dernières opérations, la plus récente en haut. Lecture seule.
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import { lireJournal, messageErreur, type Operation } from '../lib/api';
  import { erreurDuJournal } from '../lib/expliquer';
  import { dateLisible, libelleResultat } from '../lib/journal';
  import ASavoir from '../lib/composants/ASavoir.svelte';
  import Bouton from '../lib/composants/Bouton.svelte';
  import ExpliquerErreur from '../lib/composants/ExpliquerErreur.svelte';
  import Panneau from '../lib/composants/Panneau.svelte';

  interface Props {
    signaler: (texte: string, erreur?: boolean) => void;
  }

  let { signaler }: Props = $props();
  let operations = $state<Operation[] | null>(null);

  async function relire() {
    try {
      operations = await lireJournal();
    } catch (e) {
      signaler(messageErreur(e), true);
    }
  }

  onMount(() => void relire());
</script>

<h1>Journal</h1>
<p class="sous-titre">Ce que l'appli a fait, dans l'ordre.</p>
<ASavoir>Les 500 dernières opérations sont gardées dans journal.json, dans le dossier de données.</ASavoir>

<Panneau titre="Dernières opérations">
  {#if operations === null}
    <p class="discret">Lecture…</p>
  {:else if operations.length === 0}
    <p class="discret">Rien pour l'instant.</p>
  {:else}
    <table>
      <thead><tr><th>Quand</th><th>Quoi</th><th>Sur quoi</th><th>Résultat</th></tr></thead>
      <tbody>
        {#each operations as operation, i (i)}
          <tr>
            <td class="code">{dateLisible(operation.date)}</td>
            <td>{operation.action}</td>
            <td>
              {operation.objet}
              {#if operation.detail}<span class="detail">{operation.detail}</span>{/if}
            </td>
            <td class:echec={operation.resultat === 'erreur'} class:partiel={operation.resultat === 'partiel'}>
              {libelleResultat(operation.resultat)}
              {#if operation.resultat === 'erreur'}
                <ExpliquerErreur
                  erreur={erreurDuJournal(operation)}
                  origine="journal"
                  contexte={`${operation.action} · ${operation.objet}`}
                />
              {/if}
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
  <div class="actions"><Bouton onclick={() => void relire()}>Relire</Bouton></div>
</Panneau>

<style>
  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 13px;
  }

  th {
    padding: 6px 10px 6px 0;
    border-bottom: 1px solid var(--ligne);
    color: var(--texte-2);
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.12em;
    text-align: left;
    text-transform: uppercase;
  }

  td {
    padding: 6px 10px 6px 0;
    border-bottom: 1px solid color-mix(in srgb, var(--ligne) 50%, transparent);
    vertical-align: top;
  }

  td.code {
    color: var(--texte-2);
    white-space: nowrap;
  }

  .detail {
    display: block;
    color: var(--texte-2);
    font-size: 12px;
  }

  td.echec {
    color: var(--rouge);
  }

  td.partiel {
    color: var(--ambre);
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    margin-top: 16px;
  }
</style>
