<!--
  Scénario, atelier Séquence, comme en v2 (web/decoupage.js). Le fil de l'histoire (facultatif), puis les plans, le
  cœur de l'atelier : chacun a son type, sa durée, « ce qui se passe », sa réplique (mot pour mot, au début, pendant
  l'action ou à la fin, voix off possible) et son prompt anglais, modifiable. ✨ d'un plan l'écrit seul (le LLM connaît
  le fil et les plans voisins), 💬 Préciser l'écrit en conversation sous ce plan, ✨ Tout écrire écrit tous les plans
  d'un coup. 💬 M'aider à préciser l'histoire écrit le fil en conversation. Le personnage et le décor viennent des
  compléments. Puis la séquence part au Montage (créée, ou mise à jour), prête à tourner. Le brouillon est gardé.
-->
<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import {
    creerSequence,
    decouperSequence,
    estAnnulation,
    lireSequences,
    messageErreur,
    modifierSequence,
    optionsMontage,
    type ChoixListes,
    type ChoixLlm,
    type Enchainement,
    type MomentReplique,
    type Sequence,
  } from '../lib/api';
  import {
    MOMENTS,
    NOTES_TYPES,
    PLANS_MAX,
    PLANS_MIN,
    TYPES_DEPART,
    TYPES_SUITE,
    alertesJusqua,
    alertesRenumerotees,
    compterMots,
    corrigerEnchainements,
    depuisBrouillon,
    depuisSequence,
    deplacer,
    etatPlan,
    formatCompte,
    majAlertes,
    motsReplique,
    nouveauPlan,
    plansDemande,
    plansPourMontage,
    poserPlan,
    redimensionner,
    remplisAuDela,
    retirer,
    sansPrompt,
    traductionValide,
    type EtatDecoupage,
  } from '../lib/decoupage';
  import { etatVide, versIdee } from '../lib/directions';
  import AnnulerEcriture from '../lib/composants/AnnulerEcriture.svelte';
  import Bouton from '../lib/composants/Bouton.svelte';
  import Dialogue from '../lib/composants/Dialogue.svelte';
  import Panneau from '../lib/composants/Panneau.svelte';
  import VignetteCreation from '../lib/composants/VignetteCreation.svelte';
  import ChoixRushes from './ChoixRushes.svelte';
  import ScenarioConversation from './ScenarioConversation.svelte';
  import ScenarioAlerte from './ScenarioAlerte.svelte';
  import ScenarioDirections from './ScenarioDirections.svelte';
  import ScenarioEtape from './ScenarioEtape.svelte';

  interface Props {
    signaler: (texte: string, erreur?: boolean) => void;
    sujet: string;
    /** Ce que les compléments imposent déjà : la conversation de l'histoire en part. */
    complements: string;
    /** Les choix des compléments (sans le Plan d'un clip seul) : le personnage et le décor en sont tirés. */
    choix: ChoixListes;
    age: number | null;
    llms: ChoixLlm[];
    /** Le LLM choisi ; vide : celui du modèle vidéo. */
    llmChoisi: string;
    /** Une séquence du Montage à retravailler. */
    recu: Sequence | null;
    recuLu: () => void;
    /** Ouvre la séquence créée ou mise à jour dans le Montage. */
    versMontage: (id: string) => void;
  }

  let {
    signaler,
    sujet,
    complements,
    choix,
    age,
    llms,
    llmChoisi = $bindable(),
    recu,
    recuLu,
    versMontage,
  }: Props = $props();

  const BROUILLON = 'aria-decoupage';

  interface Question {
    titre: string;
    texte: string;
    bouton: string;
    agir: () => void | Promise<void>;
  }

  function relireBrouillon(): EtatDecoupage {
    try {
      const brut = localStorage.getItem(BROUILLON);
      return depuisBrouillon(brut ? JSON.parse(brut) : null);
    } catch {
      return depuisBrouillon(null);
    }
  }

  let e = $state<EtatDecoupage>(relireBrouillon());
  let durees = $state<number[]>([5, 8, 10, 15]);
  let formats = $state<string[]>([]);
  /** Le LLM écrit : tous les plans, ou celui-ci. */
  let ecriture = $state<'tout' | number | null>(null);
  let envoi = $state(false);
  let erreurEnvoi = $state('');
  let question = $state<Question | null>(null);
  /** Le rush en cours de choix : la référence du personnage, ou l'arrivée d'une transition. */
  let choixRush = $state<'reference' | number | null>(null);
  let rushChoisi = $state<string[]>([]);
  /** La conversation ouverte : celle du fil de l'histoire, ou celle d'un plan. */
  let aide = $state<'histoire' | number | null>(null);

  const dureeTotale = $derived(e.plans.reduce((t, p) => t + p.duree, 0));
  /** « 🧭 Proposer des directions » pour le fil de l'histoire, gardées tant qu'on reste dans l'atelier. */
  let directions = $state(etatVide());
  let directionsEnCours = $state(false);
  /** Une direction emmenée dans 💬 M'aider à préciser l'histoire. */
  let amorceHistoire = $state('');
  const occupe = $derived(ecriture !== null || envoi || directionsEnCours);

  // Le brouillon suit chaque changement.
  $effect(() => {
    const brut = JSON.stringify($state.snapshot(e));
    try {
      localStorage.setItem(BROUILLON, brut);
    } catch {
      // stockage indisponible : le brouillon vaut pour cette fois
    }
  });

  function demande(plan: number | null) {
    return {
      synopsis: e.synopsis,
      sujet,
      choix,
      age,
      plans: plansDemande(e.plans),
      reference: e.reference,
      contrainte: e.contrainte,
      llm: llmChoisi || null,
      musique: e.avecMusique ? e.musique.trim() || 'music that fits the story, your choice' : null,
      plan,
      contrainteEn: e.contrainteEn,
      format: formatCompte(e.plans) ? e.format : null,
    };
  }

  async function ecrire(plan: number | null) {
    if (occupe) return;
    ecriture = plan ?? 'tout';
    try {
      const r = await decouperSequence(demande(plan));
      if (plan === null) {
        e.plans = e.plans.map((p, k) => {
          const ecrit = r.plans[k];
          return ecrit ? poserPlan(p, ecrit) : p;
        });
      } else {
        const ecrit = r.plans[0];
        const p = e.plans[plan];
        if (ecrit && p) e.plans[plan] = poserPlan(p, ecrit);
      }
      e.alertes = majAlertes(e.alertes, r.alertes, plan);
      e.contrainteEn = r.contrainteEn;
      signaler(
        plan === null
          ? `${r.llm} · tous les plans écrits en ${r.dureeS} s : relis-les, retouche si besoin.`
          : `${r.llm} · plan ${plan + 1} écrit en ${r.dureeS} s.`,
      );
    } catch (err) {
      signaler(estAnnulation(err) ? 'Écriture annulée.' : messageErreur(err), !estAnnulation(err));
    } finally {
      ecriture = null;
    }
  }

  function toutEcrire() {
    const ecrits = e.plans.filter((p) => p.prompt.trim()).length;
    if (ecrits === 0) {
      void ecrire(null);
      return;
    }
    question = {
      titre: 'Tout écrire',
      texte: `✨ Tout écrire réécrit aussi ${ecrits > 1 ? `les ${ecrits} plans qui ont` : 'le plan qui a'} déjà un prompt, tes retouches à la main comprises. Continuer ?`,
      bouton: 'Tout réécrire',
      agir: () => ecrire(null),
    };
  }

  function changerNombre(n: number) {
    const remplis = remplisAuDela(e.plans, n);
    const appliquer = () => {
      e.plans = redimensionner(e.plans, n, e.reference);
      e.alertes = alertesJusqua(e.alertes, n);
    };
    if (remplis === 0) {
      appliquer();
      return;
    }
    question = {
      titre: `Passer à ${n} plans`,
      texte: `Passer à ${n} plans supprime ${remplis > 1 ? `${remplis} plans déjà remplis` : 'un plan déjà rempli'} (ce qui s'y passe, réplique, prompt). Continuer ?`,
      bouton: 'Supprimer',
      agir: appliquer,
    };
  }

  /** Les plans dépliés, dans l'ordre des plans (le premier l'est au départ). */
  let ouverts = $state<boolean[]>([true]);
  const estOuvert = (k: number): boolean => ouverts[k] ?? false;
  const ouvertsAjustes = (): boolean[] => e.plans.map((_, i) => estOuvert(i));
  const libelleType = (p: { enchainement: Enchainement }): string =>
    [...TYPES_DEPART, ...TYPES_SUITE].find(([valeur]) => valeur === p.enchainement)?.[1] ?? p.enchainement;

  function basculer(k: number) {
    const o = ouvertsAjustes();
    o[k] = !o[k];
    ouverts = o;
  }

  /** ↑ ↓ : le plan change de rang avec son voisin (ses alertes, sa conversation et son pli le suivent). */
  function deplacerPlan(k: number, delta: -1 | 1) {
    const vers = k + delta;
    if (vers < 0 || vers >= e.plans.length) return;
    e.plans = corrigerEnchainements(deplacer(e.plans, k, delta), e.reference);
    ouverts = deplacer(ouvertsAjustes(), k, delta);
    e.alertes = alertesRenumerotees(e.alertes, (r) => (r === k ? vers : r === vers ? k : r));
    if (aide === k) aide = vers;
    else if (aide === vers) aide = k;
  }

  function supprimerPlan(k: number) {
    if (e.plans.length <= PLANS_MIN) return;
    const p = e.plans[k];
    if (!p) return;
    const appliquer = () => {
      e.plans = corrigerEnchainements(retirer(e.plans, k), e.reference);
      ouverts = retirer(ouvertsAjustes(), k);
      e.alertes = alertesRenumerotees(e.alertes, (r) => (r === k ? null : r > k ? r - 1 : r));
      if (aide === k) aide = null;
      else if (typeof aide === 'number' && aide > k) aide -= 1;
    };
    if (!(p.note.trim() || p.replique.trim() || p.prompt.trim())) {
      appliquer();
      return;
    }
    question = {
      titre: `Supprimer le plan ${k + 1}`,
      texte: `Le plan ${k + 1} est déjà rempli (ce qui s'y passe, réplique, prompt) : le supprimer ? Les plans suivants prennent un rang de moins.`,
      bouton: 'Supprimer',
      agir: appliquer,
    };
  }

  function ajouterPlan() {
    if (e.plans.length >= PLANS_MAX) return;
    ouverts = [...ouvertsAjustes(), true];
    e.plans = [...e.plans, nouveauPlan(e.plans.length, e.reference)];
  }

  function changerEnchainement(k: number, valeur: Enchainement) {
    const p = e.plans[k];
    if (!p) return;
    p.enchainement = valeur;
    if (valeur !== 'transition') p.rush = null;
  }

  function ouvrirChoixRush(cible: 'reference' | number) {
    const actuel = cible === 'reference' ? e.reference : (e.plans[cible]?.rush ?? null);
    rushChoisi = actuel ? [actuel] : [];
    choixRush = cible;
  }

  function validerRush() {
    const id = rushChoisi[0] ?? null;
    if (choixRush === 'reference') {
      e.reference = id;
    } else if (typeof choixRush === 'number') {
      const p = e.plans[choixRush];
      if (p) p.rush = id;
    }
    choixRush = null;
  }

  function retirerReference() {
    e.reference = null;
    const premier = e.plans[0];
    if (premier?.enchainement === 'rush') premier.enchainement = 'texte';
  }

  /** Envoi au Montage : la première fois, la séquence est créée ; ensuite, elle est mise à jour (ses réglages du
   * Montage restent, et un plan dont le prompt n'a pas changé garde son clip). */
  async function envoyer(mettreAJour: boolean) {
    erreurEnvoi = '';
    const titre = e.titre.trim();
    const manquant = sansPrompt(e.plans);
    if (!titre) {
      erreurEnvoi = 'Donne un titre à la séquence : ses plans seront rangés dans un dossier à son nom.';
      return;
    }
    if (manquant >= 0) {
      erreurEnvoi = `Plan ${manquant + 1} : pas encore de prompt (✨ Écrire, ou écris-le).`;
      return;
    }
    envoi = true;
    try {
      const existante =
        mettreAJour && e.sequenceId ? (await lireSequences()).sequences.find((s) => s.id === e.sequenceId) : undefined;
      const plans = plansPourMontage(e.plans, e.reference, existante?.plans ?? []);
      const contrainte = e.contrainte.trim();
      const traduite = traductionValide(e) && e.contrainteEn ? { contrainte_en: e.contrainteEn } : {};
      const s =
        existante && existante.id
          ? await modifierSequence(existante.id, {
              ...existante,
              titre,
              format: e.format,
              llm: llmChoisi || null,
              plans,
              casting: { ...existante.casting, personnage: e.reference, contrainte },
              ...traduite,
            })
          : await creerSequence({
              id: null,
              titre,
              qualite: 'rapide',
              format: e.format,
              llm: llmChoisi || null,
              modele: null,
              loras: [],
              film: null,
              casting: { personnage: e.reference, voix: 'auto', ambiance: '', contrainte, reference_partout: true },
              plans,
              ...traduite,
            });
      const id = s.id;
      if (!id) return;
      e.sequenceId = id;
      s.plans.forEach((plan, k) => {
        const p = e.plans[k];
        if (p) p.idMontage = plan.id ?? null;
      });
      versMontage(id);
    } catch (err) {
      erreurEnvoi = messageErreur(err);
    } finally {
      envoi = false;
    }
  }

  function repondre() {
    const q = question;
    question = null;
    if (q) void q.agir();
  }

  // Une séquence du Montage à retravailler : elle remplace l'atelier (après confirmation s'il y a déjà un travail).
  $effect(() => {
    const s = recu;
    if (!s) return;
    untrack(() => {
      recuLu();
      const remplis = e.plans.some((p) => p.note.trim() || p.prompt.trim());
      const charger = () => {
        e = depuisSequence(s, e);
        signaler(`« ${s.titre} » est revenue au Scénario : le prochain envoi la met à jour.`);
      };
      if (remplis && e.sequenceId !== s.id) {
        question = {
          titre: 'Retravailler une séquence',
          texte: `Le Scénario a déjà une séquence en cours d'écriture : la remplacer par « ${s.titre} » ?`,
          bouton: 'Remplacer',
          agir: charger,
        };
      } else {
        charger();
      }
    });
  });

  onMount(() => {
    void (async () => {
      try {
        const o = await optionsMontage();
        durees = o.durees;
        formats = o.formats;
      } catch (err) {
        signaler(messageErreur(err), true);
      }
    })();
  });
</script>

<ScenarioEtape numero={1} titre="Le fil de l'histoire" />
<Panneau titre="Ton histoire (facultatif)">
  <textarea
    rows="3"
    maxlength="2000"
    bind:value={e.synopsis}
    aria-label="Fil de l'histoire (facultatif : les plans peuvent le raconter seuls)"
    placeholder="Une pilote rentre au hangar après une mission ratée ; elle dit « Plus jamais ça »."
  ></textarea>
  <ScenarioDirections
    atelier="sequence"
    envie={e.synopsis}
    {complements}
    workflow={null}
    llm={llmChoisi || null}
    disponible={llms.length > 0}
    bloque={ecriture !== null || envoi}
    ecritureEnCours={ecriture !== null}
    bind:etat={directions}
    bind:occupe={directionsEnCours}
    {signaler}
    prendre={(d) => {
      e.synopsis = versIdee(d);
      signaler("Direction posée dans le fil de l'histoire : retouche-la si tu veux, puis ✨ Tout écrire.");
    }}
    parler={(d) => {
      amorceHistoire = versIdee(d);
      aide = 'histoire';
    }}
  >
    {#snippet avant()}
      <Bouton petit desactive={occupe || llms.length === 0} onclick={toutEcrire}>
        {ecriture === 'tout' ? 'Le LLM écrit…' : '✨ Tout écrire'}
      </Bouton>
      <Bouton petit desactive={llms.length === 0} onclick={() => (aide = aide === 'histoire' ? null : 'histoire')}>
        {aide === 'histoire' ? "Fermer l'aide" : "💬 M'aider à préciser"}
      </Bouton>
    {/snippet}
  </ScenarioDirections>
  {#if aide === 'histoire'}
    <ScenarioConversation
      forme="sequence"
      workflow={null}
      llm={llmChoisi || null}
      contexte={complements}
      {sujet}
      format={formatCompte(e.plans) ? e.format : null}
      disponible={llms.length > 0}
      {signaler}
      libelleUtiliser="Utiliser cette histoire"
      amorce={amorceHistoire}
      amorceLue={() => (amorceHistoire = '')}
      utiliser={(p) => {
        e.synopsis = p;
        signaler('Histoire posée dans le fil : ✨ Tout écrire la répartit dans les plans.');
      }}
    />
  {/if}
  <div class="emplacement">
    <span class="etiquette">Personnage de référence</span>
    {#if e.reference}
      <VignetteCreation id={e.reference} titre="Référence du personnage" />
      <Bouton genre="discret" petit desactive={occupe} onclick={retirerReference}>✕ Retirer</Bouton>
    {:else}
      <Bouton genre="discret" petit desactive={occupe} onclick={() => ouvrirChoixRush('reference')}>Choisir une image…</Bouton>
      <span class="discret">Sans image, la description des compléments est répétée dans les plans.</span>
    {/if}
  </div>
</Panneau>

<ScenarioEtape numero={2} titre="Les plans" />
<div class="ligne">
  <label class="inline">
    <span class="etiquette">Nombre de plans</span>
    <select value={e.plans.length} onchange={(ev) => changerNombre(Number(ev.currentTarget.value))} disabled={occupe}>
      {#each Array.from({ length: PLANS_MAX - PLANS_MIN + 1 }, (_, i) => i + PLANS_MIN) as n (n)}
        <option value={n}>{n}</option>
      {/each}
    </select>
  </label>
  <span class="puce">Durée du film estimée : {dureeTotale} s</span>
</div>

<ol class="plans">
  {#each e.plans as p, k (k)}
    {@const [classe, etat] = etatPlan(p)}
    {@const replique = motsReplique(p)}
    {@const ouvert = estOuvert(k)}
    <li class={classe}>
      <button type="button" class="entete" aria-expanded={ouvert} aria-controls="plan-{k}" onclick={() => basculer(k)}>
        <b>Plan {k + 1}</b>
        <span class="puce">{libelleType(p)}</span>
        <span class="puce">{p.duree} s</span>
        <span class="puce {classe}">{etat}</span>
        <span class="resume">{p.note.trim() || 'Rien encore : ouvre le plan pour le remplir.'}</span>
        <span class="petit-texte" aria-hidden="true">{ouvert ? '▾' : '▸'}</span>
      </button>
      {#if ouvert}
        <div class="corps-plan" id="plan-{k}">
          <div class="grille g3">
            <div class="champ">
              <label for="type-{k}">Enchaînement</label>
              <select
                id="type-{k}"
                value={p.enchainement}
                onchange={(ev) => changerEnchainement(k, ev.currentTarget.value as Enchainement)}
                disabled={occupe}
              >
                {#each k === 0 ? TYPES_DEPART : TYPES_SUITE as [valeur, texte] (valeur)}
                  <option value={valeur} disabled={valeur === 'rush' && !e.reference}>{texte}</option>
                {/each}
              </select>
            </div>
            <div class="champ">
              <label for="duree-{k}">Durée</label>
              <select id="duree-{k}" bind:value={p.duree} disabled={occupe}>
                {#each durees as d (d)}
                  <option value={d}>{d} s</option>
                {/each}
              </select>
            </div>
            <div class="champ">
              <label for="note-{k}">Ce qui se passe</label>
              <textarea id="note-{k}" rows="2" maxlength="500" bind:value={p.note} placeholder={NOTES_TYPES[p.enchainement]}
              ></textarea>
            </div>
          </div>
          {#if p.enchainement === 'transition'}
            <div class="emplacement">
              {#if p.rush}
                <VignetteCreation id={p.rush} titre="Rush d'arrivée" />
                <Bouton genre="discret" petit desactive={occupe} onclick={() => (p.rush = null)}>✕ Retirer</Bouton>
              {:else}
                <Bouton genre="discret" petit desactive={occupe} onclick={() => ouvrirChoixRush(k)}>Rush d'arrivée…</Bouton>
                <span class="discret">facultatif : sinon, à choisir au Montage</span>
              {/if}
            </div>
          {/if}
          <label class="case">
            <input type="checkbox" bind:checked={p.dialogue} />
            🗣 Dialogue
          </label>
          {#if p.dialogue}
            <input
              type="text"
              maxlength="300"
              bind:value={p.replique}
              aria-label="Réplique du plan {k + 1}"
              placeholder="Ce que dit le personnage, mot pour mot : « Tu m'as encore attendu ? »"
            />
            <div class="ligne">
              <span class="discret">Quand :</span>
              {#each MOMENTS as [valeur, texte] (valeur)}
                <label class="case">
                  <input
                    type="radio"
                    name="moment-{k}"
                    value={valeur}
                    checked={p.moment === valeur}
                    onchange={() => (p.moment = valeur as MomentReplique)}
                  />
                  {texte}
                </label>
              {/each}
              <label class="case" title="On entend la voix, ses lèvres restent fermées">
                <input type="checkbox" bind:checked={p.off} />
                voix off
              </label>
              {#if replique.mots > 0}
                <small class:trop={replique.trop}>{replique.mots} mots · {replique.permis} au plus pour {p.duree} s</small>
              {/if}
            </div>
          {/if}
          <div class="champ">
            <label for="prompt-{k}">
              Prompt du plan, en anglais, tourné tel quel{compterMots(p.prompt) > 0 ? ` · ${compterMots(p.prompt)} mots` : ''}
            </label>
            <textarea
              id="prompt-{k}"
              rows="3"
              bind:value={p.prompt}
              placeholder="✨ ou 💬 l'écrivent ici ; tu peux le retoucher, ou l'écrire toi-même."
            ></textarea>
          </div>
          <div class="ligne">
            <Bouton petit desactive={occupe || llms.length === 0} onclick={() => void ecrire(k)}>
              {ecriture === k ? 'Le LLM écrit…' : '✨ Écrire'}
            </Bouton>
            <Bouton petit desactive={llms.length === 0} onclick={() => (aide = aide === k ? null : k)}>
              {aide === k ? 'Fermer' : '💬 Préciser'}
            </Bouton>
            <Bouton genre="discret" petit desactive={occupe || k === 0} onclick={() => deplacerPlan(k, -1)}>↑</Bouton>
            <Bouton genre="discret" petit desactive={occupe || k === e.plans.length - 1} onclick={() => deplacerPlan(k, 1)}>↓</Bouton>
            <Bouton genre="discret" petit desactive={occupe || e.plans.length <= PLANS_MIN} onclick={() => supprimerPlan(k)}>
              ✕ Supprimer
            </Bouton>
            {#if ecriture === k}
              <AnnulerEcriture />
            {/if}
          </div>
          {#if aide === k}
            <ScenarioConversation
              forme="plan"
              workflow={null}
              llm={llmChoisi || null}
              contexte=""
              {sujet}
              sequence={demande(k)}
              disponible={llms.length > 0}
              {signaler}
              libelleUtiliser="Utiliser pour ce plan"
              utiliser={(texte, alertes) => {
                const cible = e.plans[k];
                if (cible) cible.prompt = texte;
                e.alertes = majAlertes(e.alertes, alertes, k);
                signaler(`Plan ${k + 1} : prompt de la conversation posé.`, alertes.length > 0);
              }}
            />
          {/if}
        </div>
      {/if}
    </li>
  {/each}
</ol>
<div class="ligne">
  <Bouton genre="discret" petit desactive={occupe || e.plans.length >= PLANS_MAX} onclick={ajouterPlan}>＋ Ajouter un plan</Bouton>
  {#if ecriture === 'tout'}
    <AnnulerEcriture />
  {/if}
</div>

<div class="grille g2">
  <div class="champ">
    <label class="case">
      <input type="checkbox" bind:checked={e.avecMusique} />
      🎵 Musique d'ambiance
    </label>
    {#if e.avecMusique}
      <input type="text" maxlength="200" bind:value={e.musique} aria-label="Laquelle" placeholder="au choix du LLM" />
    {/if}
  </div>
  <div class="champ">
    <label for="contrainte">🔒 Contraintes (tous les plans)</label>
    <input id="contrainte" type="text" maxlength="300" bind:value={e.contrainte} placeholder="facultatif : pas de texte à l'écran" />
  </div>
  {#if formatCompte(e.plans) && formats.length > 0}
    <div class="champ">
      <label for="format">Format</label>
      <select id="format" bind:value={e.format}>
        {#each formats as f (f)}
          <option value={f}>{f}</option>
        {/each}
      </select>
    </div>
  {/if}
</div>

{#if e.alertes.length > 0}
  <div class="alertes">
    {#each e.alertes as a (a)}
      <ScenarioAlerte titre="À vérifier">{a}</ScenarioAlerte>
    {/each}
  </div>
{/if}

<ScenarioEtape numero={3} titre="Au Montage" />
<Panneau titre="La séquence" accentue>
  <div class="champ">
    <label for="titre-sequence">Titre de la séquence</label>
    <input
      id="titre-sequence"
      type="text"
      maxlength="40"
      bind:value={e.titre}
      placeholder="Le titre range ses clips dans un dossier des Bobines"
    />
  </div>
  <p class="discret resume-sequence">{e.plans.length} plans · {dureeTotale} s</p>
  {#if erreurEnvoi}
    <ScenarioAlerte titre="Erreur" erreur>{erreurEnvoi}</ScenarioAlerte>
  {/if}
  <div class="actions">
    {#if e.sequenceId}
      <Bouton desactive={occupe} onclick={() => void envoyer(false)}>＋ Nouvelle séquence</Bouton>
    {/if}
    <Bouton genre="principal" desactive={occupe} onclick={() => void envoyer(true)}>
      {e.sequenceId ? `→ Mettre à jour${e.titre.trim() ? ` « ${e.titre.trim()} »` : ''}` : '→ Créer dans le Montage'}
    </Bouton>
  </div>
</Panneau>

{#if choixRush !== null}
  <Dialogue
    titre={choixRush === 'reference' ? 'Quelle image de référence pour le personnage ?' : 'Sur quel rush finit la transition ?'}
    fermer={() => (choixRush = null)}
  >
    <ChoixRushes max={1} bind:selection={rushChoisi} {signaler} />
    {#snippet pied()}
      <Bouton genre="discret" onclick={() => (choixRush = null)}>Annuler</Bouton>
      <Bouton genre="principal" desactive={rushChoisi.length === 0} onclick={validerRush}>Choisir</Bouton>
    {/snippet}
  </Dialogue>
{/if}

{#if question}
  <Dialogue titre={question.titre} fermer={() => (question = null)}>
    <p>{question.texte}</p>
    {#snippet pied()}
      <Bouton genre="discret" onclick={() => (question = null)}>Annuler</Bouton>
      <Bouton genre="principal" onclick={repondre}>{question?.bouton}</Bouton>
    {/snippet}
  </Dialogue>
{/if}

<style>
  .ligne,
  .actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px 16px;
  }

  .actions {
    justify-content: flex-end;
    margin-top: 16px;
  }

  .grille {
    display: grid;
    gap: 12px 16px;
  }

  .g2 {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }

  .g3 {
    grid-template-columns: repeat(3, minmax(0, 1fr));
  }

  .champ {
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
  }

  .champ > label {
    color: var(--texte-2);
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.14em;
    text-transform: uppercase;
  }

  .champ > label.case {
    color: var(--texte);
    font-size: 13px;
    font-weight: 500;
    letter-spacing: normal;
    text-transform: none;
  }

  .champ select,
  .champ input:not([type='checkbox']) {
    width: 100%;
  }

  label.case,
  label.inline {
    display: flex;
    flex-direction: row;
    align-items: center;
    gap: 8px;
    font-size: 13px;
  }

  .etiquette {
    font-size: 12px;
    color: var(--texte-2);
  }

  .emplacement {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px;
    margin-top: 12px;
  }

  textarea,
  input[type='text'] {
    width: 100%;
  }

  .puce {
    display: inline-flex;
    align-items: center;
    padding: 2px 10px;
    border: 1px solid var(--ligne);
    color: var(--texte-2);
    font-size: 11.5px;
    letter-spacing: 0.04em;
  }

  .puce.pret {
    border-color: var(--cyan);
    color: var(--cyan);
  }

  .puce.perime {
    border-color: var(--ambre);
    color: var(--ambre);
  }

  .plans {
    display: flex;
    flex-direction: column;
    gap: 12px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .plans li {
    border: 1px solid var(--ligne);
    background: var(--fond);
  }

  .entete {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 12px;
    width: 100%;
    padding: 8px 12px;
    border: 0;
    background: none;
    color: var(--texte);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  .entete b {
    color: var(--cyan);
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  .resume {
    flex: 1;
    min-width: 120px;
    overflow: hidden;
    color: var(--texte-2);
    font-size: 12px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .petit-texte {
    color: var(--texte-2);
    font-size: 12px;
  }

  .corps-plan {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 12px;
    border-top: 1px solid var(--ligne);
  }

  small.trop {
    color: var(--orange);
  }

  .alertes {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .resume-sequence {
    margin: 8px 0 0;
  }
</style>
