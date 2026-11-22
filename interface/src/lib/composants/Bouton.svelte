<!--
  Bouton en biais : un bouton qui agit. Même forme et même taille pour tous.
  principal (plein orange) : l'action principale, une seule par page.
  secondaire (contour cyan) : les autres actions.
  discret (contour discret) : annuler ou fermer.
  Le texte s'écrit en casse normale ; la feuille de style le met en majuscules.
-->
<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    genre?: 'principal' | 'secondaire' | 'discret';
    type?: 'button' | 'submit';
    desactive?: boolean;
    /** Plus bas et plus étroit : dans un bandeau ou une rangée serrée. */
    petit?: boolean;
    onclick?: (() => void) | undefined;
    children: Snippet;
  }

  let { genre = 'secondaire', type = 'button', desactive = false, petit = false, onclick, children }: Props = $props();
</script>

<button class="bouton {genre}" class:petit {type} disabled={desactive} {onclick}><span>{@render children()}</span></button>

<style>
  .bouton {
    --biais: polygon(10px 0, 100% 0, calc(100% - 10px) 100%, 0 100%);
    position: relative;
    min-width: 160px;
    height: 38px;
    padding: 0 26px;
    border: 0;
    cursor: pointer;
    clip-path: var(--biais);
    background: var(--contour);
    color: var(--texte-bouton);
    font: 700 13px var(--police);
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }

  /* Le remplissage, posé à 1 px du bord : le contour suit le biais. */
  .bouton::before {
    content: '';
    position: absolute;
    inset: 1px;
    clip-path: var(--biais);
    background: var(--remplissage);
  }

  .bouton span {
    position: relative;
  }

  .bouton.petit {
    min-width: 0;
    height: 30px;
    padding: 0 20px;
    font-size: 11.5px;
    letter-spacing: 0.1em;
  }

  .principal {
    --contour: var(--orange);
    --remplissage: var(--orange);
    --texte-bouton: var(--fond);
  }

  .secondaire {
    --contour: var(--cyan);
    --remplissage: var(--panneau);
    --texte-bouton: var(--cyan);
  }

  .discret {
    --contour: var(--ligne);
    --remplissage: var(--panneau);
    --texte-bouton: var(--texte-2);
  }

  .bouton:hover:not(:disabled) {
    filter: brightness(1.15);
  }

  .bouton:disabled {
    opacity: 0.5;
    cursor: default;
  }
</style>
