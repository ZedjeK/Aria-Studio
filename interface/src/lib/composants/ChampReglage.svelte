<!--
  Un réglage fin, selon son type : case à cocher, liste de choix ou nombre borné. La valeur montrée est la saisie en
  cours, sinon celle en vigueur ; ↺ revient à la valeur d'origine.
-->
<script lang="ts">
  import type { ParamReglage } from '../api';
  import { libelleChoix, valeurAffichee, type Saisies } from '../reglages';

  interface Props {
    param: ParamReglage;
    saisies: Saisies;
  }

  let { param, saisies = $bindable() }: Props = $props();

  const valeur = $derived(valeurAffichee(param, saisies));
  const ecart = $derived(valeur !== param.defaut);

  function poser(nouvelle: number | boolean | string) {
    saisies = { ...saisies, [param.cle]: nouvelle };
  }

  function nombre(texte: string) {
    const n = Number(texte);
    if (texte.trim() !== '' && Number.isFinite(n)) poser(n);
  }
</script>

<div class="champ" class:ecart>
  <label for={`reglage-${param.cle}`}>{param.libelle}</label>
  <div class="saisie">
    {#if param.type === 'booleen'}
      <input id={`reglage-${param.cle}`} type="checkbox" checked={valeur === true} onchange={(e) => poser(e.currentTarget.checked)} />
    {:else if param.type === 'choix'}
      <select id={`reglage-${param.cle}`} value={String(valeur)} onchange={(e) => poser(e.currentTarget.value)}>
        {#each param.choix as c (c)}
          <option value={String(c)}>{libelleChoix(param, c)}</option>
        {/each}
      </select>
    {:else}
      <input
        id={`reglage-${param.cle}`}
        type="number"
        min={param.min ?? undefined}
        max={param.max ?? undefined}
        step={param.pas ?? undefined}
        value={valeur}
        onchange={(e) => nombre(e.currentTarget.value)}
      />
    {/if}
    {#if ecart}
      <button type="button" class="origine" title={`Revenir à ${String(param.defaut)}`} onclick={() => (saisies = { ...saisies, [param.cle]: null })}>
        ↺
      </button>
    {/if}
  </div>
  {#if param.aide}<p class="aide">{param.aide}</p>{/if}
</div>

<style>
  .champ {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  label {
    color: var(--texte-2);
    font-size: 13px;
  }

  .ecart label {
    color: var(--orange);
  }

  .saisie {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  input[type='number'] {
    width: 7em;
  }

  select {
    max-width: 100%;
  }

  .origine {
    font: inherit;
    color: var(--texte-2);
    background: transparent;
    border: none;
    cursor: pointer;
  }

  .aide {
    margin: 0;
    color: var(--texte-2);
    font-size: 12px;
  }
</style>
