<!--
  « Quel rush choisir ? », le dialogue commun à la Retouche, au Plateau, au Montage et au Scénario : les rushes les plus
  récents à cliquer, et « Une image de mon disque… » (l'action principale du dialogue, en orange) qui importe l'image dans
  les Rushes puis la sélectionne. « ✕ Fermer » reste discret.
-->
<script lang="ts">
  import { importerImage, messageErreur, type Creation } from '../lib/api';
  import Bouton from '../lib/composants/Bouton.svelte';
  import Dialogue from '../lib/composants/Dialogue.svelte';
  import ChoixRushes from './ChoixRushes.svelte';

  interface Props {
    max: number;
    selection: string[];
    signaler: (texte: string, erreur?: boolean) => void;
    fermer: () => void;
    /** Un rush vient d'être choisi, retiré ou importé : la page décide si le dialogue se referme. */
    apresChoix?: (() => void) | undefined;
    /** Une image du disque vient d'entrer dans les Rushes : la page relit son panneau. */
    apresImport?: ((fiche: Creation) => void) | undefined;
    /** « Ranger dans » : le dossier où l'image importée est rangée (celui de la page, à défaut le dossier habituel). */
    dossier?: string | null | undefined;
  }

  let { max, selection = $bindable(), signaler, fermer, apresChoix, apresImport, dossier = null }: Props = $props();

  let champ = $state<HTMLInputElement | null>(null);
  let occupe = $state(false);
  /** Relit la grille des rushes quand une image vient d'y entrer. */
  let version = $state(0);

  function importer(evenement: Event & { currentTarget: HTMLInputElement }) {
    const fichier = evenement.currentTarget.files?.[0];
    evenement.currentTarget.value = '';
    if (!fichier) return;
    const lecteur = new FileReader();
    lecteur.onload = () => {
      if (typeof lecteur.result !== 'string') return;
      const image = lecteur.result;
      void (async () => {
        occupe = true;
        try {
          const fiche = await importerImage(image, fichier.name, dossier);
          signaler(`Image importée : ${fiche.largeur}×${fiche.hauteur}.`);
          selection = [...selection, fiche.id].slice(-Math.max(1, max));
          version += 1;
          apresImport?.(fiche);
          apresChoix?.();
        } catch (e) {
          signaler(messageErreur(e), true);
        } finally {
          occupe = false;
        }
      })();
    };
    lecteur.readAsDataURL(fichier);
  }
</script>

<Dialogue titre="Quel rush choisir ?" {fermer}>
  <ChoixRushes {max} bind:selection {signaler} {apresChoix} {version} />
  {#snippet pied()}
    <input
      bind:this={champ}
      class="cache"
      type="file"
      accept="image/png,image/jpeg,image/webp"
      aria-label="Une image de mon disque"
      tabindex="-1"
      onchange={importer}
    />
    <Bouton genre="discret" onclick={fermer}>✕ Fermer</Bouton>
    <Bouton genre="principal" desactive={occupe} onclick={() => champ?.click()}>Une image de mon disque…</Bouton>
  {/snippet}
</Dialogue>

<style>
  .cache {
    display: none;
  }
</style>
