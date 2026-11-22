<!--
  Le menu de l'icône de notification, aux couleurs de l'appli : « Ouvrir » en gras, le bloc « État », le bloc
  « Aller à », puis « Démarrer avec Windows » et « Quitter ». Les blocs propres à l'appli (« Agir sans ouvrir »)
  se placent entre « État » et « Aller à ».
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import { actionMenu, ecouter, etatMenu, messageErreur, reglerDemarrageAuto, tailleMenu } from './lib/api';
  import type { ActionMenu, EtatMenu } from './lib/api';
  import { couleurTon, ligneErreur } from './lib/menu';

  let etat = $state<EtatMenu | null>(null);
  let boite = $state<HTMLElement | null>(null);

  async function relire() {
    try {
      etat = await etatMenu();
    } catch (e) {
      etat = { nom: etat?.nom ?? 'Aria', lignes: [ligneErreur(messageErreur(e))], demarrageAuto: false, miseAJour: null };
    }
  }

  function agir(action: ActionMenu) {
    void actionMenu(action);
  }

  async function basculerDemarrage() {
    if (!etat) return;
    try {
      etat.demarrageAuto = await reglerDemarrageAuto(!etat.demarrageAuto);
    } catch (e) {
      etat.lignes = [ligneErreur(messageErreur(e)), ...etat.lignes];
    }
  }

  // La fenêtre prend la hauteur du contenu, à chaque changement.
  $effect(() => {
    if (!boite) return;
    const observateur = new ResizeObserver(() => {
      if (boite) void tailleMenu(Math.ceil(boite.getBoundingClientRect().height));
    });
    observateur.observe(boite);
    return () => observateur.disconnect();
  });

  onMount(() => {
    void relire();
    const arret = ecouter('etat-icone', () => void relire());
    return () => void arret.then((f) => f());
  });
</script>

<div class="menu" bind:this={boite} role="menu">
  {#if etat}
    <button class="ligne ouvrir" role="menuitem" onclick={() => agir('ouvrir')}>Ouvrir {etat.nom}</button>
    <div class="separation"></div>

    <p class="titre">État</p>
    {#each etat.lignes as ligne, i (i)}
      <p class="etat" class:attention={ligne.ton === 'attention'} class:repos={ligne.ton === 'repos'}>
        <span class="point" style:background={couleurTon(ligne.ton)}></span>{ligne.texte}
      </p>
    {/each}
    {#if etat.miseAJour}
      <button class="ligne" role="menuitem" onclick={() => agir('miseAJour')}>Voir la mise à jour</button>
    {/if}
    <div class="separation"></div>

    <p class="titre">Aller à</p>
    <button class="ligne" role="menuitem" onclick={() => agir('parametres')}>Paramètres</button>
    <div class="separation"></div>

    <button class="ligne case" role="menuitemcheckbox" aria-checked={etat.demarrageAuto} onclick={basculerDemarrage}>
      <span class="coche" class:cochee={etat.demarrageAuto}></span>Démarrer avec Windows
    </button>
    <button class="ligne" role="menuitem" onclick={() => agir('quitter')}>Quitter {etat.nom}</button>
  {/if}
</div>

<style>
  :global(body) {
    overflow: hidden;
  }

  .menu {
    display: flex;
    flex-direction: column;
    padding: 6px 0;
    background: var(--panneau);
    border: 1px solid var(--ligne);
    font-size: 13px;
    user-select: none;
  }

  .ligne {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 6px 16px;
    border: 0;
    background: none;
    color: var(--texte);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  .ligne:hover,
  .ligne:focus-visible {
    background: #0b1a30;
    outline: none;
  }

  .ouvrir {
    font-weight: 700;
  }

  .titre {
    margin: 4px 16px 2px;
    color: var(--texte-2);
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  .etat {
    display: flex;
    align-items: center;
    gap: 10px;
    margin: 0;
    padding: 4px 16px;
  }

  .etat.repos {
    color: var(--texte-2);
  }

  .etat.attention {
    color: var(--orange);
  }

  .point {
    flex: none;
    width: 8px;
    height: 8px;
    border-radius: 50%;
  }

  .separation {
    height: 1px;
    margin: 6px 0;
    background: var(--ligne);
  }

  .coche {
    flex: none;
    width: 12px;
    height: 12px;
    border: 1px solid var(--texte-2);
  }

  .coche.cochee {
    border-color: var(--cyan);
    background: var(--cyan);
    box-shadow: inset 0 0 0 2px var(--panneau);
  }
</style>
