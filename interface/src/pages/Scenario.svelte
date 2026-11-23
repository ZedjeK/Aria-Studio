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
  import { rushesMax, workflowsPour } from '../lib/plateau';
  import { etatVide, versIdee, type EtatDirections } from '../lib/directions';
  import AnnulerEcriture from '../lib/composants/AnnulerEcriture.svelte';
  import ASavoir from '../lib/composants/ASavoir.svelte';
  import Bouton from '../lib/composants/Bouton.svelte';
  import Panneau from '../lib/composants/Panneau.svelte';
  import SousOnglets from '../lib/composants/SousOnglets.svelte';
  import ChoixRushes from './ChoixRushes.svelte';
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
    { id: 'image', titre: 'Image' },
    { id: 'retouche', titre: 'Retouche' },
    { id: 'clip', titre: 'Clip' },
    { id: 'sequence', titre: 'Séquence' },
  ];
  /** Le brouillon des autres ateliers : leur idée et leur prompt final. */
  let brouillons = $state<Record<string, { idee: string; final: string; finalRetouche: boolean }>>({});
  let workflowsRetouche = $state<ChoixWorkflow[]>([]);
  let workflowRetouche = $state<string | null>(null);
  let montage = $state<OptionsMontage | null>(null);
  let modeClip = $state<'animer' | 'texte'>('animer');
  let dureeClip = $state(5);
  /** L'image à retoucher, ou le rush à animer. */
  let imageAtelier = $state<string[]>([]);
  const proposesClip = $derived(montage ? (modeClip === 'animer' ? montage.animer : montage.texte) : null);
  const workflowClip = $derived(proposesClip ? (proposesClip.defaut ?? proposesClip.workflows[0]?.id ?? null) : null);
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
  let ongletChoisi = $state<string>('sujet');
  const onglet = $derived<OngletScenario>(
    ongletChoisi === 'decor' || ongletChoisi === 'scene' ? ongletChoisi : 'sujet',
  );

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
        }),
      );
    } catch {
      // stockage indisponible : le brouillon vaut pour cette fois
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

  const DEPARTS_IMAGE: [ModeReference | null, string][] = [
    [null, 'À partir de rien'],
    ['inspirer', "S'inspirer d'images"],
    ['variation', 'Redessiner une image'],
  ];

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
<p class="sous-titre">Compose ton prompt avec les listes guidées, puis envoie-le au Plateau.</p>
<ASavoir>
  Chaque liste ajoute sa traduction anglaise au prompt. Les règles (genre, âge, tenues qui se remplacent,
  incompatibilités) guident le choix ici ; le prompt est contrôlé par le filtre AS-18 avant d'être proposé.
  Retouche : le LLM écrit la consigne d'après l'image, qu'il voit. Clip : le prompt d'un clip avec le son, pour
  animer un rush ou à partir du texte. L'atelier Séquence découpe une histoire en plans pour le Montage.
</ASavoir>

<Panneau titre="Sujet et modèle">
  <div class="pastilles" role="group" aria-label="Atelier">
    {#each ateliers as a (a.id)}
      <button
        type="button"
        class="pastille"
        aria-pressed={atelier === a.id}
        onclick={() => changerAtelier(a.id)}
      >
        {a.titre}
      </button>
    {/each}
  </div>
  <div class="ligne">
    <div class="pastilles" role="group" aria-label="Sujet">
      {#each sujets as s (s.id)}
        <button type="button" class="pastille" aria-pressed={sujet === s.id} onclick={() => void choisirSujet(s.id)}>
          {s.nom}
        </button>
      {/each}
    </div>
    {#if atelier === 'retouche'}
      <label>
        <span class="etiquette">Modèle de retouche</span>
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
    {:else if atelier === 'clip'}
      <div class="pastilles" role="group" aria-label="Départ du clip">
        <button
          type="button"
          class="pastille"
          aria-pressed={modeClip === 'animer'}
          onclick={() => {
            modeClip = 'animer';
            planifier();
          }}
        >
          Animer un rush
        </button>
        <button
          type="button"
          class="pastille"
          aria-pressed={modeClip === 'texte'}
          onclick={() => {
            modeClip = 'texte';
            imageAtelier = [];
            planifier();
          }}
        >
          À partir du texte
        </button>
      </div>
      <label>
        <span class="etiquette">Durée</span>
        <select bind:value={dureeClip} onchange={garder}>
          {#each montage?.durees ?? [5] as d (d)}
            <option value={d}>{d} s</option>
          {/each}
        </select>
      </label>
    {/if}
    {#if atelier === 'image'}
    <label>
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
    {/if}
  </div>
</Panneau>

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
<Panneau titre={atelier === 'retouche' ? 'Ce que tu veux changer' : 'Ton idée'}>
  {#if atelier === 'image'}
    <div class="ligne" role="group" aria-label="Départ">
      {#each DEPARTS_IMAGE as [mode, texte] (texte)}
        <Bouton genre={departImage === mode ? 'principal' : 'discret'} petit onclick={() => changerDepartImage(mode)}>
          {texte}
        </Bouton>
      {/each}
    </div>
    {#if departImage}
      <p class="etiquette">
        {departImage === 'inspirer'
          ? `Les images de référence, dans l'ordre (${maxReferences} au plus pour ce modèle)`
          : "L'image à redessiner"}
      </p>
      <ChoixRushes max={Math.max(1, maxReferences)} bind:selection={referencesImage} {signaler} />
    {/if}
    <p class="discret">{aideReference('image', departImage, maxReferences)}</p>
  {/if}
  {#if imageRequise}
    <p class="etiquette">{atelier === 'retouche' ? "L'image à retoucher" : 'Le rush à animer'}</p>
    <p class="discret">{aideReference(atelier, null, 1)}</p>
    <ChoixRushes max={1} bind:selection={imageAtelier} {signaler} />
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
  <div class="ligne">
    <label>
      <span class="etiquette">LLM</span>
      <select bind:value={llmChoisi} disabled={llms.length === 0}>
        <option value="">Celui du modèle cible</option>
        {#each llms as l (l.fichier)}
          <option value={l.fichier}>{l.specialiste ? '🎯 ' : ''}{l.nom}{l.favori ? ' ★' : ''}</option>
        {/each}
      </select>
    </label>
    <Bouton
      desactive={ecriture !== null ||
        directionsEnCours ||
        llms.length === 0 ||
        imageManquante ||
        (idee.trim() === '' && complements === '')}
      onclick={() => void ecrire()}
    >
      {ecriture === 'ecrire' ? 'Le LLM écrit…' : '✨ Écrire le prompt'}
    </Bouton>
    {#if ecriture === 'ecrire'}
      <AnnulerEcriture />
    {/if}
  </div>
  <ScenarioDirections
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
  />
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

{#if structure && ctx}
  <Panneau titre="Compléments">
    <SousOnglets onglets={ongletsAvecCompte} bind:actif={ongletChoisi} />
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
    {#if onglet === 'decor'}
      <label class="texte">
        <span class="etiquette">Texte dans l'image</span>
        <input type="text" maxlength="200" bind:value={texte} oninput={planifier} placeholder="facultatif, mot pour mot" />
      </label>
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
    <div class="actions">
      <Bouton genre="discret" petit onclick={toutEffacer}>Tout réinitialiser</Bouton>
    </div>
  </Panneau>
{:else}
  <p class="discret">Lecture…</p>
{/if}

{#if atelier !== 'sequence'}
<Panneau titre={atelier === 'retouche' ? 'Consigne finale' : 'Prompt final'} accentue>
  {#if erreurAssemblage}
    <p class="alerte">{erreurAssemblage}</p>
  {/if}
  {#each assemblage?.alertes ?? [] as a (a)}
    <p class="discret">⚠ {a}</p>
  {/each}
  {#each alertesFinal as a (a)}
    <p class="alerte-ecriture">⚠ {a}</p>
  {/each}
  <textarea rows="6" value={final} oninput={(e) => retoucherFinal(e.currentTarget.value)}></textarea>
  <div class="ligne">
    {#if assemblage}
      <span class="discret">
        {final.split(/\s+/).filter((m) => m !== '').length} mots{assemblage.motsMax ? ` · ${assemblage.motsMax} au plus pour ce modèle` : ''}
      </span>
    {/if}
    {#if finalRetouche}
      <Bouton genre="discret" petit onclick={reprendreAssemblage}>Reprendre l'assemblage</Bouton>
    {/if}
  </div>
  <div class="actions">
    {#if imageManquante}
      <span class="discret">{atelier === 'retouche' ? "Choisis l'image à retoucher." : 'Choisis le rush à animer.'}</span>
    {/if}
    <Bouton
      genre="principal"
      desactive={final.trim() === '' || erreurAssemblage !== null || imageManquante}
      onclick={envoyerFinal}
    >
      {atelier === 'retouche' ? 'Envoyer à la Retouche' : atelier === 'clip' ? 'Envoyer au Montage' : 'Envoyer au Plateau'}
    </Bouton>
  </div>
</Panneau>
{/if}

<style>
  .ligne,
  .actions,
  .pastilles {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px;
    margin: 8px 0;
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

  .alerte {
    color: var(--orange);
  }
  .alerte-ecriture {
    margin: 4px 0;
    color: var(--orange);
    font-size: 13px;
  }
</style>
