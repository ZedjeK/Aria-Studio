<!--
  Carte à onglet : un choix exclusif qui se coche (un seul parmi plusieurs, comme un bouton radio). Le nom est dans
  l'onglet, qui se remplit en cyan quand la carte est choisie.
-->
<script lang="ts" generics="T extends string">
  import type { Snippet } from 'svelte';

  interface Props {
    groupe: string;
    valeur: T;
    titre: string;
    choisie: T;
    onchange?: (() => void) | undefined;
    children: Snippet;
  }

  let { groupe, valeur, titre, choisie = $bindable(), onchange, children }: Props = $props();
</script>

<label class="carte" class:choisie={choisie === valeur}>
  <input type="radio" name={groupe} value={valeur} bind:group={choisie} {onchange} />
  <span class="onglet">{titre}</span>
  <span class="corps">{@render children()}</span>
</label>

<style>
  .carte {
    display: flex;
    flex-direction: column;
    cursor: pointer;
  }

  /* Le bouton radio reste accessible au clavier, sans se voir : la carte le remplace. */
  input {
    position: absolute;
    opacity: 0;
    width: 1px;
    height: 1px;
  }

  .onglet {
    align-self: flex-start;
    padding: 5px 18px 5px 12px;
    clip-path: polygon(0 0, calc(100% - 8px) 0, 100% 8px, 100% 100%, 0 100%);
    border: 1px solid var(--cyan);
    border-bottom: 0;
    font-size: 12px;
    font-weight: 700;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--cyan);
  }

  .corps {
    flex: 1;
    padding: 12px 14px;
    border: 1px solid var(--ligne);
    background: var(--panneau);
    color: var(--texte-2);
  }

  .choisie .onglet {
    background: var(--cyan);
    color: var(--fond);
  }

  .choisie .corps {
    border-color: var(--cyan);
  }

  .carte:has(input:focus-visible) .corps {
    outline: 2px solid var(--ambre);
    outline-offset: 2px;
  }
</style>
