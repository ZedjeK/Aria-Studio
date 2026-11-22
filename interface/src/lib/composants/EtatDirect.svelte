<!--
  L'état en direct, en haut à droite de chaque page : une pastille par élément suivi (rectangle, point lumineux, nom,
  détail). Le point prend la couleur de son état.
-->
<script lang="ts">
  import { couleurTon, type Pastille } from '../menu';

  interface Props {
    pastilles: Pastille[];
  }

  let { pastilles }: Props = $props();
</script>

<ul class="etat-direct" aria-label="État en direct">
  {#each pastilles as pastille (pastille.nom)}
    <li>
      <span class="point" style:background={couleurTon(pastille.ton)} style:color={couleurTon(pastille.ton)}></span>
      <span class="nom">{pastille.nom}</span>
      <span class="detail code">{pastille.detail}</span>
    </li>
  {/each}
</ul>

<style>
  .etat-direct {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  li {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 5px 12px;
    border: 1px solid var(--ligne);
    background: color-mix(in srgb, var(--panneau) 85%, transparent);
    font-size: 11px;
  }

  .point {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    box-shadow: 0 0 6px currentColor;
  }

  .nom {
    font-weight: 600;
    letter-spacing: 0.12em;
    text-transform: uppercase;
  }

  .detail {
    color: var(--texte-2);
  }
</style>
