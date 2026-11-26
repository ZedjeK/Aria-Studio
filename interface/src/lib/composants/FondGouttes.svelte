<!--
  Fond animé d'Aria Studio : la zone principale seulement, jamais sous le menu latéral ; les panneaux, opaques, passent
  dessus. Le motif dit ce que fait l'appli, comme le veut le contrat : une grille de points que des gouttes, des cercles
  qui grandissent, allument au hasard en cyan, en bleu et en orange (le calcul est dans lib\gouttes.ts). Les règles
  communes (coupé, en pause, durée d'un passage, nombre de gouttes) sont celles du modèle, dans lib\fond.ts. Ce composant
  remplace FondAnime.svelte, le motif par défaut du modèle, qui reste tel quel dans lib\composants\.
-->
<script lang="ts">
  import type { ReglagesFond } from '../api';
  import { dureePassage, modeFond } from '../fond';
  import { avancement, eclat, LARGEUR_ANNEAU, nouveauPassage, PAS_GRILLE, type Goutte, type TeinteGoutte } from '../gouttes';

  interface Props {
    reglages: ReglagesFond;
  }

  let { reglages }: Props = $props();

  /** Opacité d'un point que rien n'allume, et part que l'éclat d'une goutte y ajoute. */
  const OPACITE_BASE = 0.17;
  const OPACITE_ECLAT = 0.85;

  const moinsAnimations = matchMedia('(prefers-reduced-motion: reduce)');
  let toile = $state<HTMLCanvasElement | undefined>();
  let visible = $state(!document.hidden);
  let animations = $state(!moinsAnimations.matches);
  const mode = $derived(modeFond(reglages, visible, animations));

  // Windows qui change de réglage, fenêtre cachée ou réduite : le fond suit, sans redémarrer l'appli.
  $effect(() => {
    const surMouvement = () => (animations = !moinsAnimations.matches);
    const surVisibilite = () => (visible = !document.hidden);
    moinsAnimations.addEventListener('change', surMouvement);
    document.addEventListener('visibilitychange', surVisibilite);
    return () => {
      moinsAnimations.removeEventListener('change', surMouvement);
      document.removeEventListener('visibilitychange', surVisibilite);
    };
  });

  $effect(() => {
    const canvas = toile;
    const contexte = canvas?.getContext('2d');
    if (!canvas || !contexte) return;
    if (mode !== 'actif') {
      contexte.clearRect(0, 0, canvas.width, canvas.height);
      return;
    }

    const style = getComputedStyle(canvas);
    const couleurs: Record<TeinteGoutte, string> = {
      cyan: style.getPropertyValue('--cyan').trim(),
      bleu: style.getPropertyValue('--bleu').trim(),
      orange: style.getPropertyValue('--orange').trim(),
    };
    const base = style.getPropertyValue('--texte-2').trim();
    const duree = dureePassage(reglages.vitesse);
    const fond = { ...reglages };

    let gouttes = nouveauPassage(fond);
    let debut = performance.now();
    let image = 0;

    const dessiner = (maintenant: number) => {
      const ratio = window.devicePixelRatio || 1;
      const largeur = Math.round(canvas.clientWidth * ratio);
      const hauteur = Math.round(canvas.clientHeight * ratio);
      if (canvas.width !== largeur || canvas.height !== hauteur) {
        canvas.width = largeur;
        canvas.height = hauteur;
      }
      let t = (maintenant - debut) / duree;
      if (t >= 1) {
        gouttes = nouveauPassage(fond);
        debut = maintenant;
        t = 0;
      }
      contexte.clearRect(0, 0, largeur, hauteur);

      // Les gouttes en cours, avec leur rayon du moment : calculées une fois par image.
      const enCours: { goutte: Goutte; avance: number; centreX: number; centreY: number; rayon: number }[] = [];
      for (const goutte of gouttes) {
        const avance = avancement(goutte, t);
        if (avance === null) continue;
        enCours.push({
          goutte,
          avance,
          centreX: goutte.x * largeur,
          centreY: goutte.y * hauteur,
          rayon: avance * goutte.rayon * ratio,
        });
      }

      // Le cercle de chaque goutte, fin, qui s'efface en grandissant.
      contexte.lineWidth = 1.2 * ratio;
      for (const g of enCours) {
        contexte.globalAlpha = 0.28 * (1 - g.avance);
        contexte.strokeStyle = couleurs[g.goutte.teinte];
        contexte.beginPath();
        contexte.arc(g.centreX, g.centreY, g.rayon, 0, Math.PI * 2);
        contexte.stroke();
      }

      // La grille de points, allumée par l'anneau de chaque goutte.
      const pas = PAS_GRILLE * ratio;
      const anneau = LARGEUR_ANNEAU * ratio;
      for (let y = pas / 2; y < hauteur; y += pas) {
        for (let x = pas / 2; x < largeur; x += pas) {
          let opacite = OPACITE_BASE;
          let couleur = base;
          let taille = 1.3;
          for (const g of enCours) {
            const k = eclat(Math.hypot(x - g.centreX, y - g.centreY), g.rayon, anneau, g.avance);
            if (k > 0 && OPACITE_BASE + k * OPACITE_ECLAT > opacite) {
              opacite = OPACITE_BASE + k * OPACITE_ECLAT;
              couleur = couleurs[g.goutte.teinte];
              taille = 1.3 + k * 1.9;
            }
          }
          contexte.globalAlpha = Math.min(opacite, 1);
          contexte.fillStyle = couleur;
          contexte.beginPath();
          contexte.arc(x, y, taille * ratio, 0, Math.PI * 2);
          contexte.fill();
        }
      }
      contexte.globalAlpha = 1;
      image = requestAnimationFrame(dessiner);
    };

    image = requestAnimationFrame(dessiner);
    return () => cancelAnimationFrame(image);
  });
</script>

<canvas bind:this={toile} class="fond" aria-hidden="true"></canvas>

<style>
  .fond {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    z-index: 0;
    pointer-events: none;
  }
</style>
