<!--
  « Ce que le LLM a écrit », sous un résultat : ta consigne, le prompt réellement envoyé au modèle et ce qui s'est passé
  à la réécriture. Repliable, fermé d'office : la comparaison avant · après reste la vedette du résultat.
-->
<script lang="ts">
  import type { TextesLlm } from '../lib/reecriture';

  interface Props {
    textes: TextesLlm;
  }

  let { textes }: Props = $props();
</script>

<details class="llm">
  <summary>Ce que le LLM a écrit</summary>
  <dl>
    <dt>Ta consigne</dt>
    <dd>{textes.idee}</dd>
    <dt>Prompt envoyé au modèle{textes.llm ? ` (écrit par ${textes.llm})` : ''}</dt>
    <dd class="code">{textes.prompt}</dd>
  </dl>
  {#if textes.remarque}
    <p class="remarque">⚠ {textes.remarque}</p>
  {/if}
</details>

<style>
  .llm {
    margin-top: 12px;
    padding: 8px 12px;
    border: 1px solid var(--ligne);
  }

  summary {
    cursor: pointer;
    color: var(--texte-2);
    font-size: 12px;
    font-weight: 700;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  dl {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin: 10px 0 0;
  }

  dt {
    color: var(--cyan);
    font-size: 12px;
  }

  dd {
    margin: 0 0 8px;
    overflow-wrap: anywhere;
    white-space: pre-wrap;
  }

  .remarque {
    margin: 0;
    color: var(--orange);
    font-size: 12px;
  }
</style>
