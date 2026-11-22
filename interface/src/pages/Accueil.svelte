<!--
  Page d'exemple, à remplacer : une action de l'appli (le panneau accentué et son bouton orange, un seul par page) et
  ce qu'elle produit, puis le dossier de données.
-->
<script lang="ts">
  import { mesurerDisque, messageErreur, type Identite } from '../lib/api';
  import { octetsLisibles } from '../lib/jauge';
  import ASavoir from '../lib/composants/ASavoir.svelte';
  import Bouton from '../lib/composants/Bouton.svelte';
  import Jauge from '../lib/composants/Jauge.svelte';
  import Panneau from '../lib/composants/Panneau.svelte';
  import SousOnglets from '../lib/composants/SousOnglets.svelte';

  interface Props {
    identite: Identite | null;
    /** Part libre du disque, mesurée ; null tant que rien n'est mesuré. */
    fraction: number | null;
    mesure: string;
    signaler: (texte: string, erreur?: boolean) => void;
  }

  let { identite, fraction = $bindable(), mesure = $bindable(), signaler }: Props = $props();

  const onglets = [
    { id: 'apercu', titre: 'Aperçu' },
    { id: 'dossier', titre: 'Dossier' },
  ];
  const commentaires: Record<string, string> = {
    apercu: "Exemple à remplacer : une action de l'appli et ce qu'elle produit. Rien n'est modifié sans ton accord.",
    dossier: "Tout ce que l'appli garde est dans ce dossier, et nulle part ailleurs.",
  };
  let onglet = $state('apercu');
  let enCours = $state(false);

  async function mesurer() {
    enCours = true;
    try {
      const espace = await mesurerDisque();
      fraction = espace.total > 0 ? espace.libre / espace.total : 0;
      mesure = `${octetsLisibles(espace.libre)} libres sur ${octetsLisibles(espace.total)}`;
      signaler('Mesure faite, rien n’a été écrit.');
    } catch (e) {
      signaler(messageErreur(e), true);
    } finally {
      enCours = false;
    }
  }
</script>

<h1>Accueil</h1>
{#if identite}<p class="sous-titre">{identite.promesse}</p>{/if}

<SousOnglets {onglets} bind:actif={onglet} />
<ASavoir>{commentaires[onglet]}</ASavoir>

{#if onglet === 'apercu'}
  <Panneau titre="Exemple d'action" accentue>
    <p class="discret">
      Le bouton orange mesure la place libre sur le disque de ton dossier de données. Lecture seule, rien n'est écrit.
    </p>
    <Jauge etiquette="Disque" {fraction} mesure={mesure || undefined} />
    <div class="actions">
      <Bouton genre="principal" desactive={enCours} onclick={mesurer}>Mesurer le disque</Bouton>
    </div>
  </Panneau>
{:else}
  <Panneau titre="Dossier de données">
    <p>Réglages, journal, sauvegardes, secrets chiffrés et cache de l'interface.</p>
    {#if identite}<p class="code discret">{identite.dossier}</p>{/if}
  </Panneau>
{/if}

<style>
  .actions {
    display: flex;
    justify-content: flex-end;
    margin-top: 16px;
  }
</style>
