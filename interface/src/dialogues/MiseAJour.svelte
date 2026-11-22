<!--
  Le dialogue de mise à jour, posé sur la fenêtre de l'appli : ouvert depuis le bandeau ou le menu de l'icône.
-->
<script lang="ts">
  import { installerMiseAJour, messageErreur, voirNotes } from '../lib/api';
  import type { InfosMiseAJour } from '../lib/api';
  import { octetsLisibles } from '../lib/jauge';
  import ASavoir from '../lib/composants/ASavoir.svelte';
  import Bouton from '../lib/composants/Bouton.svelte';
  import Dialogue from '../lib/composants/Dialogue.svelte';
  import Panneau from '../lib/composants/Panneau.svelte';

  interface Props {
    infos: InfosMiseAJour;
    fermer: () => void;
  }

  let { infos, fermer }: Props = $props();
  let enCours = $state(false);
  let etat = $state('');
  let erreur = $state(false);

  function signaler(texte: string, estErreur = false) {
    etat = texte;
    erreur = estErreur;
  }

  async function installer() {
    enCours = true;
    signaler('Téléchargement et vérification…');
    try {
      await installerMiseAJour();
    } catch (e) {
      signaler(messageErreur(e), true);
      enCours = false;
    }
  }

  async function notes() {
    try {
      await voirNotes();
    } catch (e) {
      signaler(messageErreur(e), true);
    }
  }
</script>

<Dialogue titre={`Mettre à jour ${infos.nom}`} fermer={() => !enCours && fermer()}>
  <ASavoir>
    La nouvelle version est téléchargée depuis la release GitHub de l'appli, vérifiée (taille et empreinte SHA-256),
    puis installée là où l'appli tourne. La version en cours est gardée dans les sauvegardes, et l'appli se relance.
  </ASavoir>

  <Panneau titre="Nouvelle version" accentue>
    <p><strong class="code">{infos.version}</strong> <span class="discret">(en cours : {infos.actuelle})</span></p>
    <p class="discret">Téléchargement : {octetsLisibles(infos.taille)}</p>
    <div class="lien">
      <Bouton genre="discret" onclick={notes}>Voir les notes de version</Bouton>
    </div>
  </Panneau>

  {#snippet pied()}
    <p class="etat" class:erreur role="status">{etat}</p>
    <Bouton genre="discret" desactive={enCours} onclick={fermer}>Plus tard</Bouton>
    <Bouton genre="principal" desactive={enCours} onclick={installer}>Installer la mise à jour</Bouton>
  {/snippet}
</Dialogue>

<style>
  p {
    margin: 0;
  }

  .discret {
    margin-top: 6px;
    color: var(--texte-2);
  }

  .lien {
    margin-top: 12px;
  }

  .etat {
    flex: 1;
    min-height: 1.5em;
    color: var(--texte-2);
  }

  .etat.erreur {
    color: var(--rouge);
  }
</style>
