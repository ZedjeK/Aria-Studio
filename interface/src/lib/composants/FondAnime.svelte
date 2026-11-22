<!--
  Fond animé : la zone principale seulement, jamais sous le menu latéral ; les panneaux, opaques, passent dessus.
  Les règles (coupé, en pause, durée, nombre d'éléments) sont dans lib\fond.ts ; ce composant ne fait que dessiner
  le motif par défaut : des points qui traversent la zone. Chaque appli remplace ce motif par celui qui dit ce
  qu'elle fait.
-->
<script lang="ts">
  import type { ReglagesFond } from '../api';
  import { dureePassage, modeFond, tirage } from '../fond';

  interface Props {
    reglages: ReglagesFond;
  }

  let { reglages }: Props = $props();

  const moinsAnimations = matchMedia('(prefers-reduced-motion: reduce)');
  let toile = $state<HTMLCanvasElement | undefined>();
  let visible = $state(!document.hidden);
  let animations = $state(!moinsAnimations.matches);
  const mode = $derived(modeFond(reglages, visible, animations));

  interface Point {
    y: number;
    taille: number;
    retard: number;
    cyan: boolean;
  }

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
    const cyan = style.getPropertyValue('--cyan').trim();
    const bleu = style.getPropertyValue('--bleu').trim();
    const duree = dureePassage(reglages.vitesse);
    const fond = { ...reglages };

    const nouveauPassage = (): Point[] =>
      Array.from({ length: tirage(fond) }, () => ({
        y: 0.08 + Math.random() * 0.84,
        taille: 1.5 + Math.random() * 1.5,
        retard: Math.random() * 0.3,
        cyan: Math.random() < 0.7,
      }));

    let points = nouveauPassage();
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
        points = nouveauPassage();
        debut = maintenant;
        t = 0;
      }
      contexte.clearRect(0, 0, largeur, hauteur);
      for (const p of points) {
        const avance = (t - p.retard) / (1 - p.retard);
        if (avance <= 0) continue;
        const x = avance * (largeur + 120 * ratio) - 60 * ratio;
        const y = p.y * hauteur;
        const traine = 60 * ratio;
        const degrade = contexte.createLinearGradient(x - traine, y, x, y);
        degrade.addColorStop(0, 'transparent');
        degrade.addColorStop(1, p.cyan ? cyan : bleu);
        contexte.globalAlpha = 0.35;
        contexte.strokeStyle = degrade;
        contexte.lineWidth = p.taille * ratio * 0.6;
        contexte.beginPath();
        contexte.moveTo(x - traine, y);
        contexte.lineTo(x, y);
        contexte.stroke();
        contexte.globalAlpha = 0.6;
        contexte.fillStyle = p.cyan ? cyan : bleu;
        contexte.beginPath();
        contexte.arc(x, y, p.taille * ratio, 0, Math.PI * 2);
        contexte.fill();
      }
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
