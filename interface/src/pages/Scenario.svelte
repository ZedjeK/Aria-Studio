<!--
  Scénario, comme en v2. Les ateliers Image, Retouche et Clip : ton idée, que le LLM écrit en prompt pour le modèle
  cible (✨ Écrire le prompt ; il voit l'image à retoucher ou le rush à animer), et des compléments : listes à cliquer rangées en trois onglets (Sujet, Décor et ambiance, Mise en scène),
  âge précis, texte dans l'image et précisions traduites par le LLM. Le prompt assemblé suit les compléments en
  direct (règles des listes et filtre AS-18) ; le prompt final, modifiable, part tel quel au Plateau, à la Retouche
  ou au Montage. Chaque atelier garde son idée et son prompt final. L'atelier
  Séquence (ScenarioSequence) découpe une histoire en plans, le personnage et le décor tirés des mêmes compléments, et
  l'envoie au Montage. Les règles sont appliquées ici pour guider ; c'est le cœur qui fait foi.
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import {
    assemblerPrompt,
    ecrirePromptScenario,
    enregistrerPersonnage,
    estAnnulation,
    lirePersonnages,
    messageErreur,
    optionsMontage,
    optionsPlateau,
    optionsRetouche,
    structureScenario,
    sujetsScenario,
    supprimerPersonnage,
    traduirePrecisions,
    type Assemblage,
    type ChoixLlm,
    type ChoixListes,
    type ChoixWorkflow,
    type EnvoiClip,
    type EnvoiRetouche,
    type ModeReference,
    type OptionsMontage,
    type OptionsPlateau,
    type Personnage,
    type Sequence,
    type OngletScenario,
    type StructureScenario,
    type SujetScenario,
  } from '../lib/api';
  import { AIDES_GUIDE, guideVide, type EtatGuide } from '../lib/guide';
  import {
    ONGLETS,
    aideReference,
    choixDansOnglet,
    choixDuBrouillon,
    contexteListes,
    incompatible,
    poserChoix,
    sousListeVisible,
    valeursProposees,
  } from '../lib/scenario';
  import { motsPourDuree } from '../lib/decoupage';
  import { repereVision, rushesMax, workflowsPour } from '../lib/plateau';
  import { etatVide, versIdee, type EtatDirections } from '../lib/directions';
  import AnnulerEcriture from '../lib/composants/AnnulerEcriture.svelte';
  import ASavoir from '../lib/composants/ASavoir.svelte';
  import Bouton from '../lib/composants/Bouton.svelte';
  import CarteChoix from '../lib/composants/CarteChoix.svelte';
  import Panneau from '../lib/composants/Panneau.svelte';
  import SousOnglets from '../lib/composants/SousOnglets.svelte';
  import CasesRushes from './CasesRushes.svelte';
  import ScenarioAlerte from './ScenarioAlerte.svelte';
  import ScenarioEtape from './ScenarioEtape.svelte';
  import ScenarioConversation from './ScenarioConversation.svelte';
  import ScenarioDirections from './ScenarioDirections.svelte';
  import ScenarioGuide from './ScenarioGuide.svelte';
  import ScenarioSequence from './ScenarioSequence.svelte';

  interface Props {
    signaler: (texte: string, erreur?: boolean) => void;
    /** Envoie le prompt final au Plateau, avec le modèle cible et, pour s'inspirer ou redessiner, ses images. */
    envoyer: (prompt: string, workflow: string | null, references: string[], modeReference: ModeReference | null) => void;
    /** Ouvre une séquence dans le Montage (atelier Séquence). */
    versMontage: (id: string) => void;
    /** Une séquence du Montage à retravailler ici. */
    recuSequence: Sequence | null;
    recuLu: () => void;
    /** Envoie une consigne de retouche à la page Retouche, avec son image. */
    envoyerRetouche: (envoi: EnvoiRetouche) => void;
    /** Envoie le prompt d'un clip au Montage. */
    envoyerClip: (envoi: EnvoiClip) => void;
  }

  let { signaler, envoyer, versMontage, recuSequence, recuLu, envoyerRetouche, envoyerClip }: Props = $props();

  type Atelier = 'image' | 'retouche' | 'clip' | 'sequence';
  const ATELIERS: Atelier[] = ['image', 'retouche', 'clip', 'sequence'];
  /** L'atelier : une image (Plateau), une retouche, un clip (Montage), ou une séquence découpée en plans (Montage). */
  let atelier = $state<Atelier>('image');
  const ateliers: { id: Atelier; titre: string }[] = [
    { id: 'image', titre: '🖼️ Image' },
    { id: 'retouche', titre: '🖌️ Retouche' },
    { id: 'clip', titre: '🎬 Clip' },
    { id: 'sequence', titre: '📽️ Séquence' },
  ];
  /** Le commentaire de l'onglet actif, sous les ateliers. */
  const TEXTES_ATELIER: Record<Atelier, string> = {
    image:
      "Chaque atelier garde son brouillon. Le LLM écrit pour le modèle cible : sa langue, sa longueur, ses règles. Chaque liste ajoute sa traduction anglaise, et le filtre AS-18 contrôle le prompt avant qu'il soit proposé.",
    retouche: "Le LLM voit l'image à retoucher et écrit la consigne qui la transforme, rien de plus.",
    clip: "Le prompt d'un clip dit le mouvement, la caméra, le son. La longueur suit la durée choisie.",
    sequence: "Raconte une histoire : Aria la découpe en 2 à 12 plans, que le Montage tourne l'un après l'autre.",
  };
  /** Le brouillon des autres ateliers : leur idée et leur prompt final. */
  let brouillons = $state<Record<string, { idee: string; final: string; finalRetouche: boolean }>>({});
  let workflowsRetouche = $state<ChoixWorkflow[]>([]);
  let workflowRetouche = $state<string | null>(null);
  let montage = $state<OptionsMontage | null>(null);
  let modeClip = $state<'animer' | 'texte'>('animer');
  let dureeClip = $state(5);
  /** Clip : ce que dit le personnage (mot pour mot) et la musique d'ambiance, écrites par le LLM dans le prompt. */
  let repliqueClip = $state('');
  let musiqueClip = $state('');
  const motsClip = $derived(motsPourDuree(repliqueClip, dureeClip));
  /** L'image à retoucher, ou le rush à animer. */
  let imageAtelier = $state<string[]>([]);
  const proposesClip = $derived(montage ? (modeClip === 'animer' ? montage.animer : montage.texte) : null);
  const workflowClip = $derived(proposesClip ? (proposesClip.defaut ?? proposesClip.workflows[0]?.id ?? null) : null);
  /** Le modèle vidéo du clip : le Montage le choisit, le Scénario écrit pour lui. */
  const nomWorkflowClip = $derived(proposesClip?.workflows.find((w) => w.id === workflowClip)?.nom ?? null);
  /** L'atelier a besoin d'une image : celle à retoucher, ou le rush d'un clip animé. */
  const imageRequise = $derived(atelier === 'retouche' || (atelier === 'clip' && modeClip === 'animer'));
  /** Atelier Image : partir de rien, s'inspirer d'images de référence, ou en redessiner une (comme au Plateau). */
  let departImage = $state<ModeReference | null>(null);
  let referencesImage = $state<string[]>([]);
  let optionsImage = $state<OptionsPlateau | null>(null);
  let optionsReferences = $state<OptionsPlateau | null>(null);
  const imageManquante = $derived(
    (imageRequise && imageAtelier.length === 0) ||
      (atelier === 'image' && departImage !== null && referencesImage.length === 0),
  );

  const BROUILLON = 'aria-scenario';

  let sujets = $state<SujetScenario[]>([]);
  let sujet = $state('personnage');
  let structure = $state<StructureScenario | null>(null);
  /** Les choix de chaque sujet. */
  let choixParSujet = $state<Record<string, ChoixListes>>({});
  let age = $state<number | null>(null);
  let texte = $state('');
  let toutes = $state(false);
  let workflows = $state<ChoixWorkflow[]>([]);
  let workflowId = $state<string | null>(null);
  /** Les modèles cibles de l'atelier Image, selon son départ : tous, ceux qui reçoivent des références, ou ceux qui
   * savent redessiner. */
  const proposesImage = $derived(
    optionsImage ? workflowsPour(departImage, optionsImage, optionsReferences) : workflows,
  );
  const maxReferences = $derived(rushesMax(departImage, proposesImage.find((w) => w.id === workflowId)));
  /** Le modèle cible de l'atelier Image ou Clip, avec les rubriques qu'il ignore. */
  const workflowGuide = $derived(
    atelier === 'clip'
      ? proposesClip?.workflows.find((w) => w.id === workflowClip)
      : proposesImage.find((w) => w.id === workflowId),
  );
  /** Le modèle cible de l'atelier affiché. */
  const cible = $derived(atelier === 'retouche' ? workflowRetouche : atelier === 'clip' ? workflowClip : workflowId);
  /** Les images que le LLM voit : l'image à retoucher, le rush à animer, ou les références de l'atelier Image. */
  const imagesLlm = $derived(imageRequise ? imageAtelier : atelier === 'image' && departImage ? referencesImage : []);
  let llms = $state<ChoixLlm[]>([]);
  let llmParWorkflow = $state<Record<string, string>>({});
  /** Le LLM choisi ; vide : celui du modèle cible. */
  let llmChoisi = $state('');
  let idee = $state('');
  /** « ✨ Écrire le prompt » d'après l'idée, « 💬 M'aider à préciser » en conversation, ou « 🧭 Me guider » (Image et
   * Clip) : des questions une à une, sans LLM. */
  let modeIdee = $state('ecrire');
  const guidable = $derived(atelier === 'image' || atelier === 'clip');
  const ongletsIdee = $derived([
    { id: 'ecrire', titre: '✨ Écrire' },
    { id: 'preciser', titre: "💬 M'aider à préciser" },
    ...(guidable ? [{ id: 'guider', titre: '🧭 Me guider' }] : []),
  ]);
  /** Le déroulé du guide de chaque atelier. */
  let guides = $state<{ image: EtatGuide; clip: EtatGuide }>({ image: guideVide(), clip: guideVide() });
  /** Les réponses du guide, à poser dans le champ de la conversation. */
  let amorce = $state('');
  /** « 🧭 Proposer des directions » : celles de chaque atelier, gardées tant qu'on reste au Scénario. */
  let directions = $state<Record<'image' | 'retouche' | 'clip', EtatDirections>>({
    image: etatVide(),
    retouche: etatVide(),
    clip: etatVide(),
  });
  /** Le LLM propose des directions : ✨ attend. */
  let directionsEnCours = $state(false);
  const atelierDirections = $derived(atelier === 'retouche' || atelier === 'clip' ? atelier : 'image');
  let precisions = $state('');
  /** La traduction des précisions, et le texte qu'elle traduit (elle ne vaut plus s'il change). */
  let traduction = $state<{ de: string; en: string } | null>(null);
  /** Le LLM travaille (écriture ou traduction). */
  let ecriture = $state<'ecrire' | 'traduire' | null>(null);
  let personnages = $state<Personnage[]>([]);
  /** Le nom sous lequel enregistrer le personnage en cours. */
  let nomPersonnage = $state('');
  let assemblage = $state<Assemblage | null>(null);
  let erreurAssemblage = $state<string | null>(null);
  let final = $state('');
  /** Le prompt final a été retouché à la main : l'assemblage ne l'écrase plus. */
  let finalRetouche = $state(false);
  let minuteur: ReturnType<typeof setTimeout> | undefined;

  const choix = $derived(choixParSujet[sujet] ?? {});
  /** Les choix pour une séquence : sans le Plan d'un clip seul. */
  const choixSequence = $derived(
    Object.fromEntries(Object.entries(choix).filter(([bloc]) => !structure?.blocs.find((b) => b.id === bloc)?.video)),
  );
  const ctx = $derived(structure ? contexteListes(structure, choix, age, toutes) : null);
  const ongletsAvecCompte = $derived(
    ONGLETS.map((o) => {
      const n = structure ? choixDansOnglet(structure, choix, o.id) : 0;
      return { id: o.id, titre: n > 0 ? `${o.titre} (${n})` : o.titre };
    }),
  );
  /** Les choix faits dans l'onglet affiché, en puces à retirer. */
  const puces = $derived(
    structure
      ? structure.blocs
          .filter((b) => atelier === 'clip' || !b.video)
          .flatMap((b) =>
            b.groupes
              .filter((g) => g.onglet === onglet)
              .flatMap((g) => g.sousListes)
              .flatMap((sl) => {
                const valeur = choix[b.id]?.[sl.nom];
                return valeur ? [{ cle: `${b.id}/${sl.nom}`, bloc: b.id, sousListe: sl.nom, valeur }] : [];
              }),
          )
      : [],
  );
  let ongletChoisi = $state<string>('sujet');
  const onglet = $derived<OngletScenario>(
    ongletChoisi === 'decor' || ongletChoisi === 'scene' ? ongletChoisi : 'sujet',
  );

  /** Le brouillon est-il gardé dans ce navigateur ? Faux si le stockage est refusé. */
  let brouillonGarde = $state(true);

  function garder() {
    try {
      localStorage.setItem(
        BROUILLON,
        JSON.stringify({
          sujet,
          choixParSujet,
          age,
          texte,
          toutes,
          workflowId,
          idee,
          precisions,
          traduction,
          atelier,
          brouillons,
          workflowRetouche,
          modeClip,
          dureeClip,
          repliqueClip,
          musiqueClip,
        }),
      );
      brouillonGarde = true;
    } catch {
      // stockage indisponible : le brouillon vaut pour cette fois
      brouillonGarde = false;
    }
  }

  function relireBrouillon() {
    try {
      const brut = localStorage.getItem(BROUILLON);
      if (!brut) return;
      const d: unknown = JSON.parse(brut);
      if (typeof d !== 'object' || d === null) return;
      if ('sujet' in d && typeof d.sujet === 'string') sujet = d.sujet;
      if ('atelier' in d && typeof d.atelier === 'string') {
        const lu = ATELIERS.find((a) => a === d.atelier);
        if (lu) atelier = lu;
      }
      if ('brouillons' in d && typeof d.brouillons === 'object' && d.brouillons !== null) {
        for (const [nom, b] of Object.entries(d.brouillons)) {
          if (typeof b !== 'object' || b === null) continue;
          brouillons[nom] = {
            idee: 'idee' in b && typeof b.idee === 'string' ? b.idee : '',
            final: 'final' in b && typeof b.final === 'string' ? b.final : '',
            finalRetouche: 'finalRetouche' in b && b.finalRetouche === true,
          };
        }
      }
      if ('workflowRetouche' in d && typeof d.workflowRetouche === 'string') workflowRetouche = d.workflowRetouche;
      if ('modeClip' in d && (d.modeClip === 'animer' || d.modeClip === 'texte')) modeClip = d.modeClip;
      if ('dureeClip' in d && typeof d.dureeClip === 'number') dureeClip = d.dureeClip;
      if ('repliqueClip' in d && typeof d.repliqueClip === 'string') repliqueClip = d.repliqueClip;
      if ('musiqueClip' in d && typeof d.musiqueClip === 'string') musiqueClip = d.musiqueClip;
      if ('choixParSujet' in d) choixParSujet = choixDuBrouillon(d.choixParSujet);
      if ('age' in d && typeof d.age === 'number') age = d.age;
      if ('texte' in d && typeof d.texte === 'string') texte = d.texte;
      if ('toutes' in d && typeof d.toutes === 'boolean') toutes = d.toutes;
      if ('workflowId' in d && typeof d.workflowId === 'string') workflowId = d.workflowId;
      if ('idee' in d && typeof d.idee === 'string') idee = d.idee;
      if ('precisions' in d && typeof d.precisions === 'string') precisions = d.precisions;
      if (
        'traduction' in d &&
        typeof d.traduction === 'object' &&
        d.traduction !== null &&
        'de' in d.traduction &&
        'en' in d.traduction &&
        typeof d.traduction.de === 'string' &&
        typeof d.traduction.en === 'string'
      ) {
        traduction = { de: d.traduction.de, en: d.traduction.en };
      }
    } catch {
      // brouillon illisible ou stockage indisponible : on repart de zéro
    }
  }

  async function choisirSujet(id: string) {
    sujet = id;
    try {
      structure = await structureScenario(id);
    } catch (e) {
      signaler(messageErreur(e), true);
    }
    planifier();
  }

  function changer(bloc: string, sousListe: string, valeur: string) {
    if (!structure) return;
    choixParSujet = { ...choixParSujet, [sujet]: poserChoix(structure, choix, bloc, sousListe, valeur) };
    planifier();
  }

  async function relirePersonnages() {
    try {
      personnages = await lirePersonnages();
    } catch (e) {
      signaler(messageErreur(e), true);
    }
  }

  /** Rappelle un personnage : ses choix remplacent ceux du bloc « personnage », son âge l'âge précis. */
  function rappeler(nom: string) {
    const p = personnages.find((x) => x.nom === nom);
    if (!p) return;
    choixParSujet = { ...choixParSujet, personnage: { ...(choixParSujet['personnage'] ?? {}), personnage: { ...p.choix } } };
    age = p.age;
    nomPersonnage = p.nom;
    planifier();
  }

  async function enregistrer() {
    try {
      const p = await enregistrerPersonnage(nomPersonnage, choix['personnage'] ?? {}, age);
      signaler(`${p.nom} enregistré.`);
      await relirePersonnages();
    } catch (e) {
      signaler(messageErreur(e), true);
    }
  }

  async function supprimer() {
    const nom = nomPersonnage.trim();
    if (!personnages.some((p) => p.nom === nom)) return;
    try {
      await supprimerPersonnage(nom);
      signaler(`${nom} supprimé.`);
      await relirePersonnages();
    } catch (e) {
      signaler(messageErreur(e), true);
    }
  }

  function toutEffacer() {
    choixParSujet = { ...choixParSujet, [sujet]: {} };
    age = null;
    texte = '';
    precisions = '';
    traduction = null;
    finalRetouche = false;
    planifier();
  }

  /** Chaque changement repasse par le cœur, un peu après la dernière frappe. */
  function planifier() {
    clearTimeout(minuteur);
    garder();
    minuteur = setTimeout(() => void assembler(), 250);
  }

  async function assembler() {
    try {
      const r = await assemblerPrompt({
        sujet,
        choix,
        age,
        texte,
        precisions: traductionValide ? (traduction?.en ?? '') : '',
        workflow: cible,
        action: '',
        replique: '',
        dialogue: null,
        musique: '',
        avecMusique: null,
        sansSujet: false,
      });
      assemblage = r;
      erreurAssemblage = null;
      if (!finalRetouche) final = r.prompt;
    } catch (e) {
      erreurAssemblage = messageErreur(e);
    }
  }

  /** Le LLM de l'atelier : celui choisi, sinon celui du modèle cible. */
  const llmEffectif = $derived(llmChoisi || (cible ? (llmParWorkflow[cible] ?? null) : null));

  /** Change d'atelier : celui qu'on quitte garde son idée et son prompt final, celui qu'on ouvre retrouve les siens. */
  function changerAtelier(nouvel: Atelier) {
    if (nouvel === atelier) return;
    brouillons = { ...brouillons, [atelier]: { idee, final, finalRetouche } };
    atelier = nouvel;
    const b = brouillons[nouvel];
    idee = b?.idee ?? '';
    final = b?.final ?? '';
    finalRetouche = b?.finalRetouche ?? false;
    imageAtelier = [];
    if (nouvel === 'sequence' || (modeIdee === 'guider' && nouvel !== 'image' && nouvel !== 'clip')) modeIdee = 'ecrire';
    planifier();
  }
  const traductionValide = $derived(traduction !== null && traduction.de === precisions.trim());
  /** Ce que les compléments imposent : le prompt assemblé, s'il y a un choix. */
  const complements = $derived(
    Object.values(choix).some((b) => Object.keys(b).length > 0) || age !== null || texte.trim() !== '' || traductionValide
      ? (assemblage?.prompt ?? '')
      : '',
  );

  async function traduire() {
    const de = precisions.trim();
    if (de === '' || ecriture) return;
    ecriture = 'traduire';
    try {
      traduction = { de, en: await traduirePrecisions(de, llmEffectif) };
      planifier();
    } catch (e) {
      signaler(estAnnulation(e) ? 'Traduction annulée.' : messageErreur(e), !estAnnulation(e));
    } finally {
      ecriture = null;
    }
  }

  /** Ce que le contrôle qualité (et les heures de coupe) disent du dernier prompt écrit par le LLM. */
  let alertesFinal = $state<string[]>([]);

  /** Ce que le contrôle reproche au prompt : les règles des listes, puis ce que le LLM a écrit de travers. */
  const alertesAffichees = $derived([...new Set([...(assemblage?.alertes ?? []), ...alertesFinal])]);

  // Un autre atelier : les alertes du prompt écrit pour le précédent ne valent plus.
  $effect(() => {
    void atelier;
    alertesFinal = [];
  });

  async function ecrire() {
    if (ecriture) return;
    ecriture = 'ecrire';
    try {
      const r = await ecrirePromptScenario({
        atelier: atelier === 'retouche' || atelier === 'clip' ? atelier : 'image',
        idee,
        complements,
        workflow: cible,
        llm: llmChoisi || null,
        images: imagesLlm,
        role: atelier === 'clip' && modeClip === 'animer' ? 'animer' : atelier === 'image' ? departImage : null,
        duree: atelier === 'clip' ? dureeClip : null,
        replique: atelier === 'clip' ? repliqueClip.trim() : null,
        musique: atelier === 'clip' ? musiqueClip.trim() : null,
      });
      final = r.prompt;
      finalRetouche = true;
      alertesFinal = r.alertes;
      const coupes = r.alertes.length > 0 ? ` · ⚠ ${r.alertes.join(' ')}` : '';
      signaler(`Prompt écrit par ${r.llm} en ${r.dureeS} s.${coupes}`, r.alertes.length > 0);
    } catch (e) {
      signaler(estAnnulation(e) ? 'Écriture annulée.' : messageErreur(e), !estAnnulation(e));
    } finally {
      ecriture = null;
    }
  }

  /** Le prompt final part tel quel : au Plateau, à la Retouche (avec son image) ou au Montage. */
  function envoyerFinal() {
    const texteFinal = final.trim();
    if (texteFinal === '') return;
    if (atelier === 'retouche') {
      const image = imageAtelier[0];
      if (image) envoyerRetouche({ source: image, instruction: texteFinal, workflow: workflowRetouche });
    } else if (atelier === 'clip') {
      envoyerClip({
        prompt: texteFinal,
        mode: modeClip,
        source: modeClip === 'animer' ? (imageAtelier[0] ?? null) : null,
        duree: dureeClip,
      });
    } else {
      envoyer(texteFinal, workflowId, departImage ? referencesImage : [], departImage);
    }
  }

  /** Le départ de l'atelier Image, tel que les cartes le nomment. */
  type DepartCarte = 'texte' | ModeReference;
  const departCarte = $derived<DepartCarte>(departImage ?? 'texte');
  const versDepart = (v: DepartCarte): ModeReference | null => (v === 'texte' ? null : v);

  /** Le départ du clip : animer un rush, ou partir du texte (sans image). */
  function changerModeClip(mode: 'animer' | 'texte') {
    if (mode === modeClip) return;
    modeClip = mode;
    if (mode === 'texte') imageAtelier = [];
    planifier();
  }

  /** Change le départ de l'atelier Image : les images repartent de zéro, le modèle cible suit (comme au Plateau). */
  function changerDepartImage(mode: ModeReference | null) {
    if (mode === departImage) return;
    departImage = mode;
    referencesImage = [];
    const liste = optionsImage ? workflowsPour(mode, optionsImage, optionsReferences) : workflows;
    const voulu = mode === 'inspirer' ? optionsImage?.defautReferences : optionsImage?.defaut;
    workflowId = (liste.find((w) => w.id === voulu) ?? liste[0])?.id ?? null;
    planifier();
  }

  function retoucherFinal(valeur: string) {
    final = valeur;
    finalRetouche = valeur !== (assemblage?.prompt ?? '');
  }

  function reprendreAssemblage() {
    finalRetouche = false;
    final = assemblage?.prompt ?? '';
  }

  onMount(() => {
    relireBrouillon();
    if (recuSequence) atelier = 'sequence';
    void (async () => {
      try {
        sujets = await sujetsScenario();
        const options = await optionsPlateau();
        optionsImage = options;
        workflows = options.workflows;
        llms = options.llm;
        llmParWorkflow = options.llmParWorkflow;
        await relirePersonnages();
        if (!workflows.some((w) => w.id === workflowId)) workflowId = options.defaut;
        const retouche = await optionsRetouche();
        optionsReferences = retouche;
        workflowsRetouche = retouche.workflows;
        if (!workflowsRetouche.some((w) => w.id === workflowRetouche)) workflowRetouche = retouche.defaut;
        montage = await optionsMontage();
        if (!montage.durees.includes(dureeClip)) dureeClip = montage.durees[0] ?? 5;
      } catch (e) {
        signaler(messageErreur(e), true);
      }
      await choisirSujet(sujets.some((s) => s.id === sujet) ? sujet : 'personnage');
    })();
    return () => clearTimeout(minuteur);
  });
</script>

<h1>Scénario</h1>
<p class="sous-titre">Du vague au prompt final, avec le LLM qui t'aide à préciser.</p>
<SousOnglets onglets={ateliers} bind:actif={() => atelier, (a) => changerAtelier(a as Atelier)} />
<ASavoir>{TEXTES_ATELIER[atelier]}</ASavoir>
<div class="barre">
  <span class="puce" class:ambre={!brouillonGarde}>
    <i class="point" aria-hidden="true"></i>
    {brouillonGarde ? 'Brouillon gardé' : 'Brouillon non gardé'}
  </span>
  {#if atelier === 'image'}
    <label class="inline">
      <span class="etiquette">Modèle cible</span>
      <select
        value={workflowId ?? ''}
        onchange={(e) => {
          workflowId = e.currentTarget.value || null;
          planifier();
        }}
      >
        {#each proposesImage as w (w.id)}
          <option value={w.id}>{w.nom}</option>
        {/each}
      </select>
    </label>
  {:else if atelier === 'retouche'}
    <label class="inline">
      <span class="etiquette">Modèle cible</span>
      <select
        value={workflowRetouche ?? ''}
        onchange={(e) => {
          workflowRetouche = e.currentTarget.value || null;
          planifier();
        }}
      >
        {#each workflowsRetouche as w (w.id)}
          <option value={w.id}>{w.nom}</option>
        {/each}
      </select>
    </label>
  {:else if atelier === 'clip' && nomWorkflowClip}
    <span class="inline">
      <span class="etiquette">Modèle cible</span>
      {nomWorkflowClip}
    </span>
  {/if}
  <label class="inline">
    <span class="etiquette">LLM</span>
    <select bind:value={llmChoisi} disabled={llms.length === 0}>
      <option value="">{atelier === 'sequence' ? 'Celui du modèle vidéo' : 'Celui du modèle cible'}</option>
      {#each llms as l (l.fichier)}
        <option value={l.fichier}>{l.specialiste ? '🎯 ' : ''}{l.nom}{l.favori ? ' ★' : ''}{repereVision(l)}</option>
      {/each}
    </select>
  </label>
</div>

{#if atelier === 'sequence'}
  <ScenarioSequence
    {signaler}
    {sujet}
    {complements}
    choix={choixSequence}
    {age}
    {llms}
    bind:llmChoisi
    recu={recuSequence}
    {recuLu}
    {versMontage}
  />
{:else}
  {#if atelier === 'image'}
    <ScenarioEtape numero={1} titre="Départ" />
    <div class="cartes c3" role="radiogroup" aria-label="Départ">
      <CarteChoix
        groupe="depart-image"
        valeur="texte"
        titre="Depuis un texte"
        bind:choisie={() => departCarte, (v) => changerDepartImage(versDepart(v))}
      >
        Tu pars d'une idée, sans image.
      </CarteChoix>
      <CarteChoix
        groupe="depart-image"
        valeur="inspirer"
        titre="Mélanger des images"
        bind:choisie={() => departCarte, (v) => changerDepartImage(versDepart(v))}
      >
        Des références nourrissent le prompt.
      </CarteChoix>
      <CarteChoix
        groupe="depart-image"
        valeur="variation"
        titre="Redessiner une image"
        bind:choisie={() => departCarte, (v) => changerDepartImage(versDepart(v))}
      >
        Une image sert de base.
      </CarteChoix>
    </div>
    {#if departImage}
      <p class="etiquette">
        {departImage === 'inspirer'
          ? `Les images de référence, dans l'ordre (${maxReferences} au plus pour ce modèle)`
          : "L'image à redessiner"}
      </p>
      <CasesRushes max={Math.max(1, maxReferences)} bind:selection={referencesImage} {signaler} />
    {/if}
    <p class="discret">{aideReference('image', departImage, maxReferences)}</p>
  {:else if atelier === 'retouche'}
    <ScenarioEtape numero={1} titre="L'image à retoucher" />
    <p class="discret">{aideReference(atelier, null, 1)}</p>
    <CasesRushes max={1} bind:selection={imageAtelier} {signaler} />
  {:else}
    <ScenarioEtape numero={1} titre="Départ" />
    <div class="cartes c2" role="radiogroup" aria-label="Départ du clip">
      <CarteChoix
        groupe="depart-clip"
        valeur="animer"
        titre="Animer un rush"
        bind:choisie={() => modeClip, (v) => changerModeClip(v)}
      >
        Une image du Plateau prend vie.
      </CarteChoix>
      <CarteChoix
        groupe="depart-clip"
        valeur="texte"
        titre="À partir du texte"
        bind:choisie={() => modeClip, (v) => changerModeClip(v)}
      >
        Tu décris tout, sans image.
      </CarteChoix>
    </div>
    {#if imageRequise}
      <p class="etiquette">Le rush à animer</p>
      <p class="discret">{aideReference(atelier, null, 1)}</p>
      <CasesRushes max={1} bind:selection={imageAtelier} {signaler} />
    {/if}
  {/if}

  <ScenarioEtape numero={2} titre={atelier === 'retouche' ? 'Ta consigne' : 'Ton idée'} />
  <Panneau titre={atelier === 'retouche' ? 'Ce que tu veux changer' : atelier === 'clip' ? 'Ce que tu veux animer' : 'Ce que tu veux voir'}>
  {#if atelier === 'clip'}
    <div class="ligne">
      <label class="inline">
        <span class="etiquette">Durée du clip</span>
        <select bind:value={dureeClip} onchange={garder}>
          {#each montage?.durees ?? [5] as d (d)}
            <option value={d}>{d} s</option>
          {/each}
        </select>
      </label>
    </div>
    <div class="ligne">
      <label class="inline">
        <span class="etiquette">🗣 Dialogue</span>
        <input
          type="text"
          maxlength="300"
          placeholder="Une réplique, mot pour mot (laisse vide : personne ne parle)"
          bind:value={repliqueClip}
          oninput={garder}
        />
      </label>
      {#if motsClip.mots > 0}
        <small class="compte" class:trop={motsClip.trop}>
          {motsClip.mots} mots · {motsClip.permis} au plus pour {dureeClip} s
        </small>
      {/if}
    </div>
    <div class="ligne">
      <label class="inline">
        <span class="etiquette">🎵 Musique d'ambiance</span>
        <input
          type="text"
          maxlength="200"
          placeholder="Une ambiance musicale (laisse vide : pas de musique de fond)"
          bind:value={musiqueClip}
          oninput={garder}
        />
      </label>
    </div>
  {/if}
  <SousOnglets onglets={ongletsIdee} bind:actif={modeIdee} />
  {#if modeIdee === 'preciser'}
    {#key atelier}
      <ScenarioConversation
        forme={atelier}
        workflow={cible}
        llm={llmChoisi || null}
        contexte={complements}
        {sujet}
        images={imagesLlm}
        role={atelier === 'clip' && modeClip === 'animer'
          ? 'animer'
          : atelier === 'retouche'
            ? 'retouche'
            : atelier === 'image'
              ? departImage
              : null}
        sansSujet={atelier === 'image' && departImage !== null}
        duree={atelier === 'clip' ? dureeClip : null}
        replique={atelier === 'clip' ? repliqueClip.trim() : null}
        musique={atelier === 'clip' ? musiqueClip.trim() : null}
        disponible={llms.length > 0}
        {signaler}
        libelleUtiliser={atelier === 'retouche' ? 'Utiliser cette consigne' : 'Utiliser ce prompt'}
        {amorce}
        amorceLue={() => (amorce = '')}
        utiliser={(p, alertes) => {
          final = p;
          finalRetouche = true;
          alertesFinal = alertes;
          const coupes = alertes.length > 0 ? ` · ⚠ ${alertes.join(' ')}` : '';
          signaler(
            `${atelier === 'retouche' ? 'Consigne de la conversation posée dans la consigne finale' : 'Prompt de la conversation posé dans le prompt final'}.${coupes}`,
            alertes.length > 0,
          );
        }}
      />
    {/key}
  {:else if modeIdee === 'guider' && guidable}
    <ScenarioGuide
      {structure}
      {ctx}
      {sujets}
      options={{
        sansSujet: atelier === 'image' && departImage !== null,
        video: atelier === 'clip',
        image: atelier === 'image',
        ignorees: workflowGuide?.rubriquesIgnorees ?? [],
      }}
      aide={atelier === 'clip' ? AIDES_GUIDE.clip : AIDES_GUIDE[departImage ?? 'texte']}
      bind:guide={guides[atelier === 'clip' ? 'clip' : 'image']}
      {choisirSujet}
      poser={changer}
      poserTexte={(t) => {
        texte = t;
        planifier();
      }}
      poserAge={(a) => {
        age = a;
        planifier();
      }}
      utiliser={(lignes) => {
        amorce = lignes.join('\n');
        modeIdee = 'preciser';
      }}
    />
  {:else}
  <textarea
    rows="3"
    maxlength="2000"
    placeholder={atelier === 'retouche'
      ? 'Remplace le ciel par un coucher de soleil orangé'
      : atelier === 'clip'
        ? 'Le renard lève la tête, la neige tombe doucement, le vent souffle au loin'
        : 'Un renard roux assis dans la neige fraîche, lumière dorée du soir'}
    bind:value={idee}
    oninput={garder}
  ></textarea>
  <ScenarioDirections
    ecritureEnCours={ecriture === 'ecrire'}
    atelier={atelierDirections}
    envie={idee}
    {complements}
    workflow={cible}
    llm={llmChoisi || null}
    images={imagesLlm}
    role={atelier === 'clip' && modeClip === 'animer'
      ? 'animer'
      : atelier === 'retouche'
        ? 'retouche'
        : atelier === 'image'
          ? departImage
          : null}
    duree={atelier === 'clip' ? dureeClip : null}
    disponible={llms.length > 0}
    bloque={ecriture !== null}
    bind:etat={directions[atelierDirections]}
    bind:occupe={directionsEnCours}
    {signaler}
    prendre={(d) => {
      idee = versIdee(d);
      garder();
      signaler('Direction posée dans ton idée : retouche-la si tu veux, puis ✨ Écrire le prompt ou 💬.');
    }}
    parler={(d) => {
      amorce = versIdee(d);
      modeIdee = 'preciser';
    }}
  >
    {#snippet avant()}
      <Bouton
        petit
        desactive={ecriture !== null ||
          directionsEnCours ||
          llms.length === 0 ||
          imageManquante ||
          (idee.trim() === '' && complements === '')}
        onclick={() => void ecrire()}
      >
        {ecriture === 'ecrire' ? 'Le LLM écrit…' : '✨ Écrire le prompt'}
      </Bouton>
    {/snippet}
  </ScenarioDirections>
  {#if llms.length === 0}
    <p class="discret">Aucun LLM dans modeles\llm : compose le prompt avec les compléments, il part tel quel.</p>
  {:else}
    <p class="discret">
      Le LLM écrit le prompt pour le modèle cible, d'après ton idée et les compléments, qu'il garde tels quels. Il
      prend la carte graphique le temps d'écrire.
    </p>
  {/if}
  {/if}
</Panneau>
{/if}

{#if atelier !== 'sequence'}
  <ScenarioEtape numero={3} titre="Compléments" />
{/if}
{#if structure && ctx}
  <Panneau titre="Listes guidées">
    <SousOnglets onglets={ongletsAvecCompte} bind:actif={ongletChoisi} />
    {#if onglet === 'sujet'}
      <div class="ligne" role="group" aria-label="Type du sujet">
        <span class="etiquette">Type</span>
        {#each sujets as s (s.id)}
          <button type="button" class="pastille" aria-pressed={sujet === s.id} onclick={() => void choisirSujet(s.id)}>
            {s.nom}
          </button>
        {/each}
      </div>
    {/if}
    {#if onglet === 'sujet' && sujet === 'personnage'}
      <div class="ligne personnages">
        <label>
          <span class="etiquette">Personnage enregistré</span>
          <select value="" onchange={(e) => rappeler(e.currentTarget.value)} disabled={personnages.length === 0}>
            <option value="">{personnages.length === 0 ? 'Aucun pour l’instant' : 'Rappeler…'}</option>
            {#each personnages as p (p.nom)}
              <option value={p.nom}>{p.nom}{p.age !== null ? ` · ${p.age} ans` : ''}</option>
            {/each}
          </select>
        </label>
        <label>
          <span class="etiquette">Nom</span>
          <input type="text" maxlength="40" bind:value={nomPersonnage} placeholder="Léa" />
        </label>
        <Bouton petit desactive={nomPersonnage.trim() === ''} onclick={() => void enregistrer()}>Enregistrer</Bouton>
        {#if personnages.some((p) => p.nom === nomPersonnage.trim())}
          <Bouton genre="discret" petit onclick={() => void supprimer()}>Supprimer</Bouton>
        {/if}
      </div>
      <div class="ligne">
        <label>
          <span class="etiquette">Âge précis</span>
          <input
            type="number"
            min={structure.agePrecis[0]}
            max={structure.agePrecis[1]}
            placeholder="facultatif"
            value={age ?? ''}
            oninput={(e) => {
              const n = Number.parseInt(e.currentTarget.value, 10);
              age = Number.isNaN(n) ? null : n;
              planifier();
            }}
          />
        </label>
        <label class="case">
          <input type="checkbox" bind:checked={toutes} onchange={planifier} />
          Toutes les options (sans le filtre par origine)
        </label>
      </div>
    {/if}
    {#each structure.blocs.filter((b) => atelier === 'clip' || !b.video) as b (b.id)}
      {#each b.groupes.filter((g) => g.onglet === onglet) as g (g.nom)}
        {@const visibles = g.sousListes.filter((s) => sousListeVisible(b.id, s, ctx))}
        {#if visibles.length > 0}
          <details open={visibles.some((s) => choix[b.id]?.[s.nom])}>
            <summary>{g.nom}</summary>
            <div class="grille">
              {#each visibles as s (s.nom)}
                {@const actuelle = choix[b.id]?.[s.nom] ?? ''}
                {@const proposees = valeursProposees(b.id, s, ctx)}
                <label title={s.note}>
                  <span class="etiquette">{s.nom}</span>
                  <select value={actuelle} onchange={(e) => changer(b.id, s.nom, e.currentTarget.value)}>
                    <option value="">—</option>
                    {#if actuelle && !proposees.some((v) => v.nom === actuelle)}
                      <option value={actuelle}>{actuelle} (hors filtre)</option>
                    {/if}
                    {#each proposees as v (v.nom)}
                      <option value={v.nom} disabled={incompatible(structure, choix, b.id, s.nom, v.nom)}>
                        {v.categorie ? `${v.categorie} · ` : ''}{v.nom}
                      </option>
                    {/each}
                  </select>
                  {#if actuelle}
                    {@const d = s.valeurs.find((v) => v.nom === actuelle)?.description}
                    {#if d}<span class="discret">{d}</span>{/if}
                  {/if}
                </label>
              {/each}
            </div>
          </details>
        {/if}
      {/each}
    {/each}
    {#if onglet === 'scene'}
      <label class="texte">
        <span class="etiquette">Texte dans l'image</span>
        <input type="text" maxlength="200" bind:value={texte} oninput={planifier} placeholder="facultatif, mot pour mot" />
      </label>
    {/if}
    {#if onglet === 'decor'}
      <label class="texte">
        <span class="etiquette">Précisions (traduites par le LLM, puis ajoutées au prompt)</span>
        <textarea rows="2" maxlength="500" bind:value={precisions} oninput={planifier}></textarea>
      </label>
      {#if precisions.trim() !== ''}
        <div class="ligne">
          {#if traductionValide}
            <span class="discret">→ {traduction?.en}</span>
          {:else}
            <Bouton petit desactive={ecriture !== null || llms.length === 0} onclick={() => void traduire()}>
              {ecriture === 'traduire' ? 'Traduction…' : 'Traduire et ajouter'}
            </Bouton>
            {#if ecriture === 'traduire'}
              <AnnulerEcriture />
            {/if}
          {/if}
        </div>
      {/if}
    {/if}
    {#if puces.length > 0 || (onglet === 'sujet' && age !== null)}
      <div class="puces" role="group" aria-label="Tes choix">
        {#each puces as c (c.cle)}
          <span class="puce">
            {c.valeur}
            <button type="button" aria-label="Retirer {c.valeur}" onclick={() => changer(c.bloc, c.sousListe, '')}>✕</button>
          </span>
        {/each}
        {#if onglet === 'sujet' && age !== null}
          <span class="puce">
            {age} ans
            <button
              type="button"
              aria-label="Retirer l'âge précis"
              onclick={() => {
                age = null;
                planifier();
              }}>✕</button
            >
          </span>
        {/if}
      </div>
    {/if}
    <div class="actions">
      <Bouton genre="discret" petit onclick={toutEffacer}>Tout réinitialiser</Bouton>
    </div>
  </Panneau>
{:else}
  <p class="discret">Lecture…</p>
{/if}

{#if atelier !== 'sequence'}
<ScenarioEtape numero={4} titre={atelier === 'retouche' ? 'Consigne finale' : 'Prompt final'} />
<Panneau titre={atelier === 'retouche' ? 'Consigne finale' : 'Prompt final'} accentue>
  {#if assemblage}
    <p class="compte">
      {final.split(/\s+/).filter((m) => m !== '').length} mots{assemblage.motsMax ? ` · ${assemblage.motsMax} au plus pour ce modèle` : ''}
    </p>
  {/if}
  <textarea rows="6" value={final} oninput={(e) => retoucherFinal(e.currentTarget.value)}></textarea>
  <div class="alertes">
    {#if erreurAssemblage}
      <ScenarioAlerte titre="Erreur" erreur>{erreurAssemblage}</ScenarioAlerte>
    {/if}
    {#each alertesAffichees as a (a)}
      <ScenarioAlerte>{a}</ScenarioAlerte>
    {/each}
  </div>
  {#if finalRetouche}
    <div class="ligne">
      <Bouton genre="discret" petit onclick={reprendreAssemblage}>Reprendre l'assemblage</Bouton>
    </div>
  {/if}
  <div class="actions">
    {#if imageManquante}
      <span class="discret">{atelier === 'retouche' ? "Choisis l'image à retoucher." : 'Choisis le rush à animer.'}</span>
    {/if}
    <Bouton
      genre="principal"
      desactive={final.trim() === '' || erreurAssemblage !== null || imageManquante}
      onclick={envoyerFinal}
    >
      {atelier === 'retouche' ? '→ Envoyer à la Retouche' : atelier === 'clip' ? '→ Envoyer au Montage' : '→ Envoyer au Plateau'}
    </Bouton>
  </div>
</Panneau>
{/if}

<style>
  .ligne,
  .actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px;
    margin: 8px 0;
  }

  .actions {
    justify-content: flex-end;
  }

  /* Les cartes à onglet du départ : une colonne par choix. */
  .cartes {
    display: grid;
    gap: 12px;
    align-items: stretch;
  }

  .c2 {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }

  .c3 {
    grid-template-columns: repeat(3, minmax(0, 1fr));
  }

  .pastille {
    padding: 4px 12px;
    border: 1px solid var(--ligne);
    background: transparent;
    color: var(--texte-2);
    font: inherit;
    cursor: pointer;
  }

  .pastille[aria-pressed='true'] {
    border-color: var(--cyan);
    color: var(--texte);
  }

  label {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  /* La barre sous les ateliers : brouillon, modèle cible, LLM, comme la maquette. */
  .barre {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px 16px;
    margin-top: -4px;
  }

  .inline {
    flex-direction: row;
    align-items: center;
    gap: 8px;
    font-size: 13px;
  }

  .puce {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 2px 10px;
    border: 1px solid var(--cyan);
    color: var(--cyan);
    font-size: 11.5px;
    letter-spacing: 0.04em;
  }

  .puce.ambre {
    border-color: var(--ambre);
    color: var(--ambre);
  }

  .point {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: currentColor;
  }

  .case {
    flex-direction: row;
    align-items: center;
    gap: 6px;
    font-size: 13px;
  }

  .etiquette {
    font-size: 12px;
    color: var(--texte-2);
  }

  details {
    margin: 10px 0;
  }

  summary {
    cursor: pointer;
  }

  .grille {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
    gap: 10px;
    margin-top: 8px;
  }

  .texte {
    margin: 10px 0;
  }

  textarea {
    width: 100%;
  }

  .compte {
    margin: 0 0 6px;
    color: var(--texte-2);
    font-size: 12px;
  }

  .compte.trop {
    color: var(--orange);
  }

  .inline input[type='text'] {
    min-width: 340px;
  }

  .alertes {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-top: 10px;
  }

  .puces {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 10px;
  }

  .puces .puce {
    border-color: var(--cyan);
    color: var(--cyan);
  }

  .puces .puce button {
    padding: 0;
    border: 0;
    background: none;
    color: inherit;
    font-size: 12px;
    cursor: pointer;
  }
</style>
