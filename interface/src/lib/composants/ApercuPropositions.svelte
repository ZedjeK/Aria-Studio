<!--
  L'aperçu de « Recommander » et de « ⚡ Adapter à ma machine » : chaque réglage qui changerait (avant → après), sa
  raison et sa source, les notes sur la carte graphique. Rien ne change avant « Appliquer » ; chaque ligne se décoche.
-->
<script lang="ts">
  import type { Proposition } from '../api';
  import { valeurLisible, type Ecart } from '../optimiseur';
  import ASavoir from './ASavoir.svelte';
  import Bouton from './Bouton.svelte';
  import Dialogue from './Dialogue.svelte';

  interface Props {
    titre: string;
    /** Une ligne au-dessus (la machine, par exemple). */
    entete?: string;
    lignes: Ecart[];
    alertes: string[];
    /** Ce qui ne change pas, à dire quand même. */
    notes: string[];
    /** Ce qu'on dit quand rien ne changerait. */
    vide: string;
    appliquer: (choisies: Proposition[]) => Promise<void>;
    fermer: () => void;
  }

  let { titre, entete = '', lignes, alertes, notes, vide, appliquer, fermer }: Props = $props();

  let decochees = $state<number[]>([]);
  let occupe = $state(false);

  const choisies = $derived(lignes.filter((_, i) => !decochees.includes(i)).map((l) => l.proposition));
  /** Plusieurs modèles : le nom devant chaque réglage. */
  const plusieurs = $derived(new Set(lignes.map((l) => l.proposition.cible)).size > 1);

  function basculer(i: number) {
    decochees = decochees.includes(i) ? decochees.filter((x) => x !== i) : [...decochees, i];
  }

  async function confirmer() {
    occupe = true;
    try {
      await appliquer(choisies);
    } finally {
      occupe = false;
    }
  }
</script>

<Dialogue {titre} fermer={() => !occupe && fermer()}>
  <ASavoir>
    {#if entete}{entete}. {/if}Rien n'est appliqué avant « Appliquer la sélection » ; chaque ligne se décoche.
  </ASavoir>
  {#if alertes.length > 0}
    <ul class="alertes">
      {#each alertes as a (a)}<li>{a}</li>{/each}
    </ul>
  {/if}
  {#if lignes.length === 0}
    <p class="ok">{vide}</p>
  {:else}
    <table>
      <thead>
        <tr><th></th><th>Réglage</th><th>Avant</th><th>Après</th><th>Pourquoi</th></tr>
      </thead>
      <tbody>
        {#each lignes as l, i (`${l.proposition.cible}|${l.proposition.cle}`)}
          <tr>
            <td>
              <input
                type="checkbox"
                aria-label={l.proposition.libelle}
                checked={!decochees.includes(i)}
                onchange={() => basculer(i)}
                disabled={occupe}
              />
            </td>
            <td>
              {#if plusieurs}<span class="discret">{l.proposition.nom} · </span>{/if}{l.proposition.libelle}
            </td>
            <td class="valeur">{valeurLisible(l.avant)}</td>
            <td class="valeur"><strong>{valeurLisible(l.proposition.propose)}</strong></td>
            <td class="discret">
              {l.proposition.raison}{#if l.proposition.source}
                · <a href={l.proposition.source} target="_blank" rel="noopener noreferrer">source</a>{/if}
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
  {#each notes as n (n)}<p class="discret">{n}</p>{/each}
  {#snippet pied()}
    <Bouton genre="discret" desactive={occupe} onclick={fermer}>Fermer</Bouton>
    {#if lignes.length > 0}
      <Bouton desactive={occupe || decochees.length === 0} onclick={() => (decochees = [])}>Tout cocher</Bouton>
      <Bouton genre="principal" desactive={occupe || choisies.length === 0} onclick={() => void confirmer()}>
        Appliquer la sélection{choisies.length > 0 ? ` (${choisies.length})` : ''}
      </Bouton>
    {/if}
  {/snippet}
</Dialogue>

<style>
  .alertes {
    display: flex;
    flex-direction: column;
    gap: 10px;
    margin: 8px 0;
    padding: 0;
    list-style: none;
  }

  .alertes li {
    color: var(--orange);
  }

  table {
    width: 100%;
    margin: 8px 0;
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
    padding: 7px 10px 7px 0;
    border-bottom: 1px solid color-mix(in srgb, var(--ligne) 50%, transparent);
    vertical-align: top;
  }

  .valeur {
    font-family: var(--police-code);
    white-space: nowrap;
  }

  .discret {
    color: var(--texte-2);
    font-size: 12px;
  }

  .ok {
    color: var(--cyan);
  }
</style>
