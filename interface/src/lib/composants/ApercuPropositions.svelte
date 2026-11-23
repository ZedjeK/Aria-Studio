<!--
  L'aperçu de « Recommander » et de « ⚡ Adapter à ma machine » : chaque réglage qui changerait (avant → après), sa
  raison et sa source, les notes sur la carte graphique. Rien ne change avant « Appliquer » ; chaque ligne se décoche.
-->
<script lang="ts">
  import type { Proposition } from '../api';
  import { valeurLisible, type Ecart } from '../optimiseur';
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
  {#if entete}<p class="discret">{entete}</p>{/if}
  {#if alertes.length > 0}
    <ul class="alertes">
      {#each alertes as a (a)}<li>{a}</li>{/each}
    </ul>
  {/if}
  {#if lignes.length === 0}
    <p class="ok">{vide}</p>
  {:else}
    <ul class="lignes">
      {#each lignes as l, i (`${l.proposition.cible}|${l.proposition.cle}`)}
        <li>
          <label>
            <input type="checkbox" checked={!decochees.includes(i)} onchange={() => basculer(i)} disabled={occupe} />
            <span class="quoi">
              {#if plusieurs}<span class="discret">{l.proposition.nom} · </span>{/if}{l.proposition.libelle}
            </span>
            <span class="valeurs">
              {valeurLisible(l.avant)} → <strong>{valeurLisible(l.proposition.propose)}</strong>
            </span>
          </label>
          <span class="discret raison">
            {l.proposition.raison}{#if l.proposition.source}
              · <a href={l.proposition.source} target="_blank" rel="noopener noreferrer">source</a>{/if}
          </span>
        </li>
      {/each}
    </ul>
  {/if}
  {#each notes as n (n)}<p class="discret">{n}</p>{/each}
  {#snippet pied()}
    <Bouton genre="discret" desactive={occupe} onclick={fermer}>Fermer</Bouton>
    {#if lignes.length > 0}
      <Bouton genre="principal" desactive={occupe || choisies.length === 0} onclick={() => void confirmer()}>
        Appliquer{choisies.length > 0 ? ` (${choisies.length})` : ''}
      </Bouton>
    {/if}
  {/snippet}
</Dialogue>

<style>
  .lignes,
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

  .lignes li {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  label {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }

  .quoi {
    min-width: 160px;
  }

  .valeurs {
    font-family: var(--police-code);
    font-size: 13px;
  }

  .raison {
    padding-left: 24px;
    font-size: 12px;
  }

  .ok {
    color: var(--cyan);
  }
</style>
