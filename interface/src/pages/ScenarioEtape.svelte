<!--
  Titre d'étape du Scénario, comme la maquette : « 1 · Départ », en bleu et en capitales, suivi d'un filet. Il se pose
  entre les panneaux d'un atelier pour le numéroter ; le bouton orange est à la dernière étape.
-->
<script lang="ts">
  interface Props {
    numero: number;
    titre: string;
  }

  let { numero, titre }: Props = $props();
</script>

<h2 class="etape">{numero} · {titre}</h2>

<style>
  .etape {
    display: flex;
    align-items: center;
    gap: 10px;
    margin: 4px 0 -4px;
    color: var(--bleu);
    font-size: 13px;
    font-weight: 700;
    letter-spacing: 0.14em;
    text-transform: uppercase;
  }

  .etape::after {
    content: '';
    flex: 1;
    height: 1px;
    background: var(--ligne);
  }
</style>
