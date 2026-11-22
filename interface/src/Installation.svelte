<!--
  La proposition d'installation, montrée au lancement d'un exe hors de son dossier programme\. Fenêtre de dialogue :
  titre en cyan, « À savoir », le panneau sur lequel agit le bouton orange (accentué), puis la ligne d'état et les
  boutons, l'orange en dernier.
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import { infosInstallation, installer, lancerSansInstaller, messageErreur } from './lib/api';
  import type { InfosInstallation } from './lib/api';
  import ASavoir from './lib/composants/ASavoir.svelte';
  import Bouton from './lib/composants/Bouton.svelte';
  import Panneau from './lib/composants/Panneau.svelte';

  let infos = $state<InfosInstallation | null>(null);
  let raccourci = $state(false);
  let nePlusProposer = $state(false);
  let enCours = $state(false);
  let etat = $state('');
  let erreur = $state(false);

  onMount(async () => {
    try {
      infos = await infosInstallation();
    } catch (e) {
      etat = messageErreur(e);
      erreur = true;
    }
  });

  async function agir(action: () => Promise<void>, pendant: string) {
    enCours = true;
    etat = pendant;
    erreur = false;
    try {
      await action();
    } catch (e) {
      etat = messageErreur(e);
      erreur = true;
      enCours = false;
    }
  }
</script>

<div class="dialogue">
  <h1>{infos ? `Installer ${infos.nom}` : 'Installer'}</h1>
  <ASavoir>
    L'appli peut se ranger dans ton dossier de données et se lancer de là. Rien n'est installé ailleurs, et rien ne
    change dans Windows.
  </ASavoir>

  <Panneau titre="Dossier d'installation" accentue>
    {#if infos}
      <p class="code">{infos.dossier}</p>
      {#if infos.versionInstallee}
        <p class="discret">
          La version {infos.versionInstallee} déjà installée sera remplacée par la {infos.version}, et gardée dans les
          sauvegardes.
        </p>
      {/if}
    {/if}
  </Panneau>

  <div class="options">
    <label class="ligne">
      <input type="checkbox" bind:checked={raccourci} disabled={!infos?.raccourci} />
      Créer un raccourci sur le Bureau
    </label>
    {#if infos?.raccourci}<p class="aide code">{infos.raccourci}</p>{/if}

    <label class="ligne">
      <input type="checkbox" bind:checked={nePlusProposer} />
      Ne plus me proposer l'installation
    </label>
    <p class="aide">
      Pris en compte avec « Lancer sans installer ». La proposition se réactive dans les paramètres de l'appli.
    </p>
  </div>

  <p class="etat" class:erreur role="status">{etat}</p>

  <div class="boutons">
    <Bouton
      genre="secondaire"
      desactive={enCours}
      onclick={() => agir(() => lancerSansInstaller(nePlusProposer), 'Ouverture…')}
    >
      Lancer sans installer
    </Bouton>
    <Bouton
      genre="principal"
      desactive={enCours || !infos}
      onclick={() => agir(() => installer(raccourci), 'Installation…')}
    >
      Installer
    </Bouton>
  </div>
</div>

<style>
  .dialogue {
    display: flex;
    flex-direction: column;
    gap: 16px;
    height: 100%;
    padding: 24px;
    overflow-y: auto;
  }

  h1 {
    margin: 0;
    color: var(--cyan);
    font-size: 20px;
    font-weight: 600;
  }

  p {
    margin: 0;
  }

  .discret {
    margin-top: 8px;
    color: var(--texte-2);
  }

  .options {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .ligne {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-top: 8px;
  }

  .aide {
    margin-left: 28px;
    color: var(--texte-2);
    font-size: 12px;
  }

  .etat {
    min-height: 1.5em;
    color: var(--texte-2);
  }

  .etat.erreur {
    color: var(--rouge);
  }

  .boutons {
    display: flex;
    justify-content: flex-end;
    gap: 12px;
    margin-top: auto;
  }
</style>
