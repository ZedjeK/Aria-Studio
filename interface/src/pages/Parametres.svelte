<!--
  Les paramètres de l'appli elle-même (démarrage, installation, mises à jour, affichage), à part des réglages de son
  travail. Chaque changement est enregistré tout de suite. Pas de bouton orange ici, donc aucun panneau accentué.
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import {
    chercherMiseAJour,
    lireDemarrageAuto,
    messageErreur,
    miseAJourDisponible,
    reglerDemarrageAuto,
  } from '../lib/api';
  import type { Identite, InfosMiseAJour, Reglages } from '../lib/api';
  import ASavoir from '../lib/composants/ASavoir.svelte';
  import Bouton from '../lib/composants/Bouton.svelte';
  import CarteChoix from '../lib/composants/CarteChoix.svelte';
  import Panneau from '../lib/composants/Panneau.svelte';
  import DialogueMiseAJour from '../dialogues/MiseAJour.svelte';
  import { texteRecherche } from '../lib/miseAJour';

  interface Props {
    identite: Identite;
    reglages: Reglages;
    enregistrer: () => void;
    signaler: (texte: string, erreur?: boolean) => void;
  }

  let { identite, reglages = $bindable(), enregistrer, signaler }: Props = $props();
  let demarrageAuto = $state(false);
  /** La version trouvée au lancement, s'il y en a une : « Voir la mise à jour » n'apparaît qu'alors. */
  let miseAJour = $state<InfosMiseAJour | null>(null);
  let dialogueMiseAJour = $state(false);
  /** « Vérifier maintenant » : la requête part, puis la ligne dit ce qui a été trouvé ou ce qui a manqué. */
  let recherche = $state<'repos' | 'en_cours'>('repos');
  let retourRecherche = $state<{ texte: string; erreur: boolean } | null>(null);

  async function verifierMaintenant() {
    recherche = 'en_cours';
    retourRecherche = null;
    try {
      const trouvee = await chercherMiseAJour();
      miseAJour = trouvee;
      retourRecherche = { texte: texteRecherche(trouvee, identite.version), erreur: false };
    } catch (e) {
      retourRecherche = { texte: messageErreur(e), erreur: true };
    } finally {
      recherche = 'repos';
    }
  }

  async function relireDemarrageAuto() {
    if (!identite.enFond) return;
    try {
      demarrageAuto = await lireDemarrageAuto();
    } catch (e) {
      signaler(messageErreur(e), true);
    }
  }

  async function changerDemarrageAuto() {
    try {
      demarrageAuto = await reglerDemarrageAuto(demarrageAuto);
      signaler(demarrageAuto ? 'Démarrage avec Windows activé.' : 'Démarrage avec Windows désactivé.');
    } catch (e) {
      signaler(messageErreur(e), true);
      await relireDemarrageAuto();
    }
  }

  onMount(() => {
    void relireDemarrageAuto();
    void miseAJourDisponible()
      .then((infos) => (miseAJour = infos))
      .catch((e: unknown) => signaler(messageErreur(e), true));
    // « Démarrer avec Windows » se change aussi dans Windows et dans le menu de l'icône : relu au retour.
    const auRetour = () => void relireDemarrageAuto();
    window.addEventListener('focus', auRetour);
    return () => window.removeEventListener('focus', auRetour);
  });
</script>

<h1>Paramètres</h1>
<p class="sous-titre">{identite.nom} elle-même : démarrage, installation, mises à jour, affichage.</p>
<ASavoir>
  {identite.enFond
    ? 'Chaque changement est enregistré tout de suite : dans reglages.json, et dans Windows pour le démarrage.'
    : 'Chaque changement est enregistré tout de suite dans reglages.json, dans le dossier de données.'}
</ASavoir>

{#if identite.enFond}
  <Panneau titre="Démarrage">
    <label class="ligne">
      <input type="checkbox" bind:checked={demarrageAuto} onchange={changerDemarrageAuto} />
      Démarrer avec Windows (dans l'icône de notification, sans ouvrir la fenêtre)
    </label>
  </Panneau>
{/if}

{#if identite.installable}
  <Panneau titre="Installation">
    <label class="ligne">
      <input type="checkbox" bind:checked={reglages.proposerInstallation} onchange={enregistrer} />
      Proposer l'installation quand l'exe est lancé hors de son dossier
    </label>
  </Panneau>
{/if}

<Panneau titre="Mises à jour">
  <label class="ligne">
    <input type="checkbox" bind:checked={reglages.chercherMisesAJour} onchange={enregistrer} />
    Chercher une nouvelle version au lancement (une requête vers GitHub ; rien n'est installé sans ton accord)
  </label>
  <div class="actions">
    <span class="discret" class:erreur={retourRecherche?.erreur} role="status">
      {#if recherche === 'en_cours'}
        Recherche en cours… (une requête vers GitHub)
      {:else if retourRecherche}
        {retourRecherche.texte}
      {:else if miseAJour}
        {texteRecherche(miseAJour, identite.version)}
      {:else}
        Une seule requête vers GitHub, seulement quand tu cliques ; rien n'est installé sans ton accord.
      {/if}
    </span>
    <div class="boutons">
      <Bouton petit desactive={recherche === 'en_cours'} onclick={() => void verifierMaintenant()}>
        Vérifier maintenant
      </Bouton>
      {#if miseAJour}
        <Bouton petit onclick={() => (dialogueMiseAJour = true)}>Voir la mise à jour</Bouton>
      {/if}
    </div>
  </div>
</Panneau>

<Panneau titre="Fond animé">
  <label class="ligne">
    <input type="checkbox" bind:checked={reglages.fond.actif} onchange={enregistrer} />
    Afficher le fond animé (jamais si Windows demande moins d'animations)
  </label>

  <p class="etiquette">Vitesse</p>
  <div class="cartes">
    <CarteChoix groupe="vitesse" valeur="lente" titre="Lente" bind:choisie={reglages.fond.vitesse} onchange={enregistrer}>
      Un passage toutes les 14 s
    </CarteChoix>
    <CarteChoix groupe="vitesse" valeur="normale" titre="Normale" bind:choisie={reglages.fond.vitesse} onchange={enregistrer}>
      Un passage toutes les 7 s
    </CarteChoix>
    <CarteChoix groupe="vitesse" valeur="rapide" titre="Rapide" bind:choisie={reglages.fond.vitesse} onchange={enregistrer}>
      Un passage toutes les 3,5 s
    </CarteChoix>
  </div>

  <p class="etiquette">Gouttes par passage</p>
  <div class="ligne">
    <label>Au moins <input type="number" min="1" max="12" bind:value={reglages.fond.minimum} onchange={enregistrer} /></label>
    <label>au plus <input type="number" min="1" max="12" bind:value={reglages.fond.maximum} onchange={enregistrer} /></label>
  </div>
</Panneau>

{#if dialogueMiseAJour && miseAJour}
  <DialogueMiseAJour infos={miseAJour} fermer={() => (dialogueMiseAJour = false)} />
{/if}

<style>
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 10px 16px;
    align-items: center;
    justify-content: space-between;
    margin-top: 10px;
  }

  .discret {
    color: var(--texte-2);
    font-size: 13px;
  }

  .discret.erreur {
    color: var(--rouge);
  }

  .boutons {
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
  }

  .ligne {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px 16px;
  }

  .etiquette {
    margin: 18px 0 8px;
    color: var(--texte-2);
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.14em;
    text-transform: uppercase;
  }

  .cartes {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 12px;
  }

  input[type='number'] {
    width: 4.5em;
  }
</style>
