<!--
  Montage → Séquences, comme en v2 (web/sequence.js) : des plans enchaînés (prolonger, couper, transition), tournés
  dans l'ordre et assemblés en un film par le cœur. Les prompts s'écrivent au Scénario ou dans chaque plan ; ici on les
  retouche, on règle le tournage (personnage, voix, checkpoint, résolution, LoRA) et on tourne. Un plan modifié n'est
  plus « à jour » : il sera retourné, avec ceux qui partent de lui.
-->
<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import {
    creerSequence,
    lireRushes,
    lireSequences,
    lireVoix,
    messageErreur,
    modifierSequence,
    restaurerSequence,
    supprimerSequence,
    tournerSequence,
    viderCorbeilleSequences,
    type ChoixWorkflow,
    type Creation,
    type Enchainement,
    type EtatSequences,
    type OptionsMontage,
    type Sequence,
    type VoixBibliotheque,
  } from '../lib/api';
  import { lignesPour, lorasChoisies, type LigneLora } from '../lib/loras';
  import { dureeLisible } from '../lib/montage';
  import { nomCheckpoint } from '../lib/plateau';
  import {
    AIDE_ENCHAINEMENT,
    aTourner,
    avecRush,
    DEPARTS,
    deplacer,
    estimationSequence,
    nouveauPlan,
    parlePlan,
    retirer,
    sequenceVide,
    stepsSequence,
    SUITES,
  } from '../lib/sequences';
  import Bouton from '../lib/composants/Bouton.svelte';
  import ChoixLoras from '../lib/composants/ChoixLoras.svelte';
  import Dialogue from '../lib/composants/Dialogue.svelte';
  import Panneau from '../lib/composants/Panneau.svelte';
  import SonVoix from '../lib/composants/SonVoix.svelte';
  import VideoCreation from '../lib/composants/VideoCreation.svelte';
  import VignetteCreation from '../lib/composants/VignetteCreation.svelte';
  import ChoixRushes from './ChoixRushes.svelte';

  interface Props {
    signaler: (texte: string, erreur?: boolean) => void;
    options: OptionsMontage;
    /** Le workflow qui anime un rush : celui de tous les plans. */
    workflow: ChoixWorkflow;
    /** Une commande vient d'entrer dans la file du Plateau. */
    enFile: () => void;
    /** Change quand la file a fini quelque chose : la séquence ouverte se relit (ses plans tournés). */
    rafraichir: number;
    /** Une séquence à ouvrir (depuis un plan ou un film des Bobines, ou le Scénario) ; remis à null une fois pris en
     * compte. */
    ouvrir?: string | null;
    /** La séquence repart au Scénario (atelier Séquence) pour être retravaillée. */
    retravailler?: (s: Sequence) => void;
  }

  let { signaler, options, workflow, enFile, rafraichir, ouvrir = $bindable(null), retravailler }: Props = $props();
  let cadre = $state<HTMLElement | null>(null);

  /** Une question avant d'agir (abandon de modifications, corbeille, vidage). */
  interface Question {
    titre: string;
    texte: string;
    bouton: string;
    agir: () => void | Promise<void>;
  }

  let etat = $state<EtatSequences | null>(null);
  let courante = $state<Sequence | null>(null);
  let modifiee = $state(false);
  let lignesLoras = $state<Record<string, LigneLora>>({});
  let voixGardees = $state<VoixBibliotheque[]>([]);
  let clipsVoix = $state<Creation[]>([]);
  let erreur = $state('');
  let occupe = $state(false);
  let question = $state<Question | null>(null);
  /** Le rush en cours de choix : le personnage, ou le rush d'un plan. */
  let choixRush = $state<'personnage' | number | null>(null);
  let rushChoisi = $state<string[]>([]);
  let regarde = $state<string | null>(null);
  let corbeilleOuverte = $state(false);

  const opts = $derived(etat?.options ?? null);
  const loras = $derived(lorasChoisies(workflow.loras, lignesLoras));
  const lorasChangees = $derived(courante !== null && JSON.stringify(loras) !== JSON.stringify(courante.loras));
  const nonEnregistree = $derived(modifiee || lorasChangees);
  const enTournage = $derived(courante?.id ? (etat?.enTournage.includes(courante.id) ?? false) : false);
  const bloque = $derived(occupe || enTournage);
  const indices = $derived(courante ? aTourner(courante.plans) : []);
  const qualite = $derived(options.qualites.find((q) => q.cle === courante?.qualite));
  const estimation = $derived(
    courante && qualite && opts ? estimationSequence(courante, indices, qualite, opts) : 0,
  );
  const dureeFilm = $derived(courante?.plans.reduce((t, p) => t + p.duree, 0) ?? 0);
  const stepsParole = $derived(courante && opts && stepsSequence(courante, opts) < opts.stepsParole);

  function afficher(s: Sequence) {
    courante = structuredClone(s);
    modifiee = false;
    erreur = '';
    regarde = null;
    const avant: Record<string, LigneLora> = {};
    for (const l of s.loras) avant[l.fichier] = { actif: true, force: l.force, declencheurs: l.declencheurs };
    lignesLoras = lignesPour(workflow.loras, avant);
    // Une LoRA supprimée du disque n'est plus proposée : elle partira au prochain enregistrement.
    if (courante) courante.loras = lorasChoisies(workflow.loras, lignesLoras);
  }

  /** Une modification ; `toutRetourner` : casting, résolution, format ou checkpoint, toute la séquence est à refaire. */
  function toucher(toutRetourner = false) {
    modifiee = true;
    if (toutRetourner && courante) for (const p of courante.plans) p.a_jour = false;
  }

  function avantDAbandonner(agir: () => void) {
    if (!nonEnregistree) {
      agir();
      return;
    }
    question = {
      titre: 'Modifications non enregistrées',
      texte: `La séquence ouverte${courante?.titre ? ` (« ${courante.titre} »)` : ''} a des changements non enregistrés : les abandonner ?`,
      bouton: 'Abandonner',
      agir,
    };
  }

  async function relire() {
    try {
      etat = await lireSequences();
      const ouverte = courante?.id ? etat.sequences.find((s) => s.id === courante?.id) : undefined;
      if (!courante) afficher(etat.sequences[0] ?? sequenceVide(etat.options.modeleDefaut));
      else if (ouverte && !nonEnregistree) afficher(ouverte); // les plans tournés depuis
    } catch (e) {
      signaler(messageErreur(e), true);
    }
  }

  async function relireVoix() {
    try {
      [voixGardees] = await lireVoix(false);
      // Le son d'un clip tourné (pas d'un film assemblé), parmi les Bobines.
      clipsVoix = (await lireRushes('tous', null, 'clips')).filter((c) => c.mode_video !== 'film').slice(0, 60);
    } catch (e) {
      signaler(messageErreur(e), true);
    }
  }

  function choisirSequence(id: string) {
    avantDAbandonner(() => {
      const s = etat?.sequences.find((x) => x.id === id);
      afficher(s ?? sequenceVide(opts?.modeleDefaut ?? null));
    });
  }

  async function enregistrer(): Promise<boolean> {
    if (!courante) return false;
    occupe = true;
    erreur = '';
    try {
      const donnees: Sequence = { ...$state.snapshot(courante), titre: courante.titre.trim(), loras };
      const s = courante.id ? await modifierSequence(courante.id, donnees) : await creerSequence(donnees);
      afficher(s);
      etat = await lireSequences();
      return true;
    } catch (e) {
      erreur = messageErreur(e);
      return false;
    } finally {
      occupe = false;
    }
  }

  async function tourner(plans: number[] = []) {
    if (!courante) return;
    if (!courante.titre.trim()) {
      erreur = 'Donne un titre à la séquence : ses clips seront rangés dans un dossier à son nom.';
      return;
    }
    if ((nonEnregistree || !courante.id) && !(await enregistrer())) return;
    const id = courante.id;
    if (!id) return;
    // Rien à tourner : « Tout retourner ».
    const demandes = plans.length > 0 || indices.length > 0 ? plans : courante.plans.map((_, k) => k);
    occupe = true;
    try {
      await tournerSequence(id, demandes);
      signaler('Séquence en file : les plans se tournent dans l’ordre, puis le film est assemblé.');
      enFile();
      etat = await lireSequences();
    } catch (e) {
      erreur = messageErreur(e);
    } finally {
      occupe = false;
    }
  }

  function mettreALaCorbeille() {
    const s = courante;
    if (!s?.id) return;
    const id = s.id;
    question = {
      titre: 'Mettre à la corbeille',
      texte: `Mettre la séquence « ${s.titre} » à la corbeille ? Ses plans et son film restent dans les Bobines ; tu pourras la restaurer.`,
      bouton: 'Mettre à la corbeille',
      agir: async () => {
        try {
          await supprimerSequence(id);
          courante = null;
          await relire();
        } catch (e) {
          signaler(messageErreur(e), true);
        }
      },
    };
  }

  function restaurer(id: string) {
    avantDAbandonner(() => {
      void (async () => {
        try {
          const s = await restaurerSequence(id);
          etat = await lireSequences();
          afficher(s);
        } catch (e) {
          signaler(messageErreur(e), true);
        }
      })();
    });
  }

  function vider() {
    const n = etat?.corbeille.length ?? 0;
    question = {
      titre: 'Vider la corbeille des séquences',
      texte: `${n} séquence(s) seront effacées définitivement. Leurs clips et leurs films restent dans les Bobines.`,
      bouton: 'Supprimer définitivement',
      agir: async () => {
        try {
          const effacees = await viderCorbeilleSequences();
          signaler(`${effacees} séquence(s) effacée(s).`);
          etat = await lireSequences();
        } catch (e) {
          signaler(messageErreur(e), true);
        }
      },
    };
  }

  function repondre() {
    const q = question;
    question = null;
    if (q) void q.agir();
  }

  // --- plans ---

  function changerEnchainement(k: number, e: Enchainement) {
    const p = courante?.plans[k];
    if (!p) return;
    p.enchainement = e;
    if (!avecRush(e)) p.rush = null;
    p.a_jour = false;
    toucher();
  }

  function ajouterPlan() {
    courante?.plans.push(nouveauPlan('couper'));
    toucher();
  }

  function monter(k: number, vers: number) {
    if (!courante) return;
    courante.plans = deplacer(courante.plans, k, vers);
    toucher();
  }

  function enlever(k: number) {
    const plans = courante?.plans;
    if (!plans) return;
    question = {
      titre: `Supprimer le plan ${k + 1}`,
      texte: 'Supprimer ce plan ? Son clip reste dans les Bobines.',
      bouton: 'Supprimer',
      agir: () => {
        if (!courante) return;
        courante.plans = retirer(courante.plans, k);
        toucher();
      },
    };
  }

  function ouvrirChoixRush(cible: 'personnage' | number) {
    const actuel = cible === 'personnage' ? courante?.casting.personnage : courante?.plans[cible]?.rush;
    rushChoisi = actuel ? [actuel] : [];
    choixRush = cible;
  }

  function validerRush() {
    const id = rushChoisi[0] ?? null;
    if (courante && choixRush === 'personnage') {
      courante.casting.personnage = id;
      toucher(true);
    } else if (courante && typeof choixRush === 'number') {
      const p = courante.plans[choixRush];
      if (p) {
        p.rush = id;
        p.a_jour = false;
        toucher();
      }
    }
    choixRush = null;
  }

  function retirerRush(cible: 'personnage' | number) {
    if (!courante) return;
    if (cible === 'personnage') {
      courante.casting.personnage = null;
      toucher(true);
    } else {
      const p = courante.plans[cible];
      if (p) {
        p.rush = null;
        p.a_jour = false;
        toucher();
      }
    }
  }

  // Une séquence demandée depuis les Bobines : ouverte (après confirmation s'il y a des changements), et montrée.
  $effect(() => {
    const id = ouvrir;
    if (!id || !etat) return;
    ouvrir = null;
    untrack(() => {
      if (etat?.sequences.some((s) => s.id === id)) {
        choisirSequence(id);
        cadre?.scrollIntoView({ behavior: 'smooth', block: 'start' });
      } else {
        signaler('Séquence introuvable : elle est peut-être à la corbeille des séquences.', true);
      }
    });
  });

  // La file a fini quelque chose : plans tournés, film assemblé, ou voix gardée.
  $effect(() => {
    if (rafraichir > 0) {
      void relire();
      void relireVoix();
    }
  });

  onMount(() => {
    void relire();
    void relireVoix();
  });
</script>

<div bind:this={cadre}></div>
<Panneau titre="Séquences">
  {#if !etat || !courante || !opts}
    <p class="discret">Lecture…</p>
  {:else}
    <div class="ligne">
      <label class="large">
        <span class="etiquette">Séquence</span>
        <select
          value={courante.id ?? ''}
          disabled={bloque}
          onchange={(e) => {
            const cible = e.currentTarget;
            const id = cible.value;
            cible.value = courante?.id ?? '';
            choisirSequence(id);
          }}
        >
          <option value="">＋ Nouvelle séquence</option>
          {#each etat.sequences as s (s.id)}
            <option value={s.id}>{s.titre}</option>
          {/each}
        </select>
      </label>
      <label class="large">
        <span class="etiquette">Titre</span>
        <input
          type="text"
          maxlength={opts.longueurTitre}
          bind:value={courante.titre}
          oninput={() => toucher()}
          disabled={bloque}
          placeholder="Le titre range ses clips dans un dossier des Bobines"
        />
      </label>
      {#if nonEnregistree}<span class="alerte">● non enregistrée</span>{/if}
    </div>

    {#if courante.alerte}
      <p class="alerte">{courante.alerte}</p>
    {/if}
    {#if enTournage}
      <p class="discret">En tournage : la séquence se débloque à la fin (ou à l'annulation, dans la file).</p>
    {/if}

    <h3>Casting et tournage</h3>
    <div class="casting">
      <div class="emplacement">
        {#if courante.casting.personnage}
          <VignetteCreation id={courante.casting.personnage} titre="Personnage" />
          <Bouton genre="discret" petit desactive={bloque} onclick={() => retirerRush('personnage')}>✕ Retirer</Bouton>
        {:else}
          <Bouton genre="discret" desactive={bloque} onclick={() => ouvrirChoixRush('personnage')}>Personnage…</Bouton>
        {/if}
        <label
          class="case discret"
          title="Départs et transitions reçoivent aussi le personnage (les plans coupés toujours, les plans prolongés jamais : leurs images ancrées le tiennent déjà)"
        >
          <input
            type="checkbox"
            bind:checked={courante.casting.reference_partout}
            onchange={() => toucher(true)}
            disabled={bloque}
          />
          sur tous les plans
        </label>
      </div>
      <div class="grille">
        <label>
          <span class="etiquette">Voix de référence</span>
          <select bind:value={courante.casting.voix} onchange={() => toucher(true)} disabled={bloque}>
            <option value="auto">Auto : le premier plan qui parle</option>
            <option value={null}>Aucune référence</option>
            {#if voixGardees.length > 0}
              <optgroup label="Bibliothèque">
                {#each voixGardees as v (v.id)}
                  <option value={`voix:${v.id}`}>{v.source === 'fichier' ? '📁' : '🎙️'} {v.nom ?? v.id}</option>
                {/each}
              </optgroup>
            {/if}
            {#if clipsVoix.length > 0}
              <optgroup label="Son d'un clip">
                {#each clipsVoix as c (c.id)}
                  <option value={c.id}>{(c.idee || c.id).slice(0, 50)}</option>
                {/each}
              </optgroup>
            {/if}
          </select>
        </label>
        <label>
          <span class="etiquette">Résolution</span>
          <select bind:value={courante.qualite} onchange={() => toucher(true)} disabled={bloque}>
            {#each options.qualites as q (q.cle)}
              <option value={q.cle} title={q.aide}>{q.nom} · {q.aide}</option>
            {/each}
          </select>
        </label>
        <label>
          <span class="etiquette">Checkpoint</span>
          <select
            value={courante.modele ?? opts.modeleDefaut ?? ''}
            onchange={(e) => {
              if (courante) courante.modele = e.currentTarget.value || null;
              toucher(true);
            }}
            disabled={bloque || workflow.modeles.length === 0}
          >
            {#each workflow.modeles as fichier (fichier)}
              <option value={fichier}>{nomCheckpoint(fichier)}</option>
            {/each}
          </select>
        </label>
        {#if courante.plans[0]?.enchainement === 'texte'}
          <label>
            <span class="etiquette">Format</span>
            <select bind:value={courante.format} onchange={() => toucher(true)} disabled={bloque}>
              {#each options.formats as f (f)}
                <option value={f}>{f}</option>
              {/each}
            </select>
          </label>
        {/if}
      </div>
    </div>
    {#if courante.casting.voix?.startsWith('voix:')}
      <SonVoix id={courante.casting.voix.slice(5)} />
    {/if}
    {#if courante.casting.contrainte}
      <p class="discret">Contrainte de tous les plans : {courante.casting.contrainte}</p>
    {/if}
    <ChoixLoras
      famille={workflow.famille}
      proposees={workflow.loras}
      forceMax={options.animer.loraForceMax}
      bind:lignes={lignesLoras}
    />

    <h3>Plans</h3>
    <ol class="plans">
      {#each courante.plans as p, k (k)}
        <li class:a-jour={!indices.includes(k)}>
          <div class="ligne">
            <strong>Plan {k + 1}</strong>
            <select
              value={p.enchainement}
              onchange={(e) => changerEnchainement(k, e.currentTarget.value as Enchainement)}
              disabled={bloque}
              aria-label="Enchaînement du plan {k + 1}"
            >
              {#each k === 0 ? DEPARTS : SUITES as [valeur, texte] (valeur)}
                <option value={valeur}>{texte}</option>
              {/each}
            </select>
            <select
              bind:value={p.duree}
              onchange={() => {
                p.a_jour = false;
                toucher();
              }}
              disabled={bloque}
              aria-label="Durée du plan {k + 1}"
            >
              {#each options.durees as d (d)}
                <option value={d}>{d} s</option>
              {/each}
            </select>
            <span class="discret etat-plan">
              {indices.includes(k) ? 'à tourner' : 'à jour'}{parlePlan(p) && stepsParole
                ? ` · voix soignée (${opts.stepsParole} steps)`
                : ''}{(p.prompt ?? '').trim() ? '' : ' · prompt à écrire'}
            </span>
            <span class="actions-plan">
              <Bouton genre="discret" petit desactive={bloque || k === 0} onclick={() => monter(k, k - 1)}>↑</Bouton>
              <Bouton
                genre="discret"
                petit
                desactive={bloque || k === courante.plans.length - 1}
                onclick={() => monter(k, k + 1)}>↓</Bouton
              >
              {#if p.clip}
                <Bouton genre="discret" petit onclick={() => (regarde = regarde === p.clip ? null : p.clip)}>
                  {regarde === p.clip ? 'Masquer' : 'Voir'}
                </Bouton>
                <Bouton genre="discret" petit desactive={bloque} onclick={() => void tourner([k])}>Retourner</Bouton>
              {/if}
              <Bouton genre="discret" petit desactive={bloque || courante.plans.length === 1} onclick={() => enlever(k)}
                >✕</Bouton
              >
            </span>
          </div>
          <p class="discret">{AIDE_ENCHAINEMENT[p.enchainement]}</p>
          <div class="corps-plan">
            {#if avecRush(p.enchainement)}
              <div class="emplacement">
                {#if p.rush}
                  <VignetteCreation id={p.rush} titre={k === 0 ? 'Rush de départ' : "Rush d'arrivée"} />
                  <Bouton genre="discret" petit desactive={bloque} onclick={() => retirerRush(k)}>✕ Retirer</Bouton>
                {:else}
                  <Bouton genre="discret" desactive={bloque} onclick={() => ouvrirChoixRush(k)}>
                    {k === 0 ? 'Rush de départ…' : "Rush d'arrivée…"}
                  </Bouton>
                {/if}
              </div>
            {/if}
            <div class="textes">
              <input
                type="text"
                maxlength={opts.longueurIdee}
                bind:value={p.idee}
                oninput={() => {
                  p.a_jour = false;
                  toucher();
                }}
                disabled={bloque}
                placeholder="Ce qui se passe (un résumé, pour s'y retrouver ; facultatif)"
              />
              <textarea
                rows="4"
                value={p.prompt ?? ''}
                class:manquant={!(p.prompt ?? '').trim()}
                oninput={(e) => {
                  p.prompt = e.currentTarget.value;
                  p.a_jour = false;
                  toucher();
                }}
                disabled={bloque}
                placeholder="Prompt du plan, tourné tel quel : il arrive du Scénario (découpage), ou écris-le ici (en anglais)."
              ></textarea>
            </div>
          </div>
          {#if p.clip && regarde === p.clip}
            <VideoCreation id={p.clip} />
          {/if}
        </li>
      {/each}
    </ol>
    <Bouton genre="discret" desactive={bloque || courante.plans.length >= opts.plansMax} onclick={ajouterPlan}>
      ＋ Ajouter un plan
    </Bouton>

    {#if erreur}
      <p class="alerte">{erreur}</p>
    {/if}
    <div class="actions">
      <span class="discret">
        {indices.length > 0
          ? `${indices.length} plan${indices.length > 1 ? 's' : ''} à tourner · ≈ ${dureeLisible(estimation)} sur la carte de la mesure`
          : 'Tous les plans sont à jour'} · film d'environ {dureeFilm} s
      </span>
      <Bouton genre="discret" desactive={bloque || !courante.id} onclick={mettreALaCorbeille}>Corbeille</Bouton>
      {#if retravailler}
        <Bouton
          genre="discret"
          desactive={bloque || !courante.id || nonEnregistree}
          onclick={() => {
            if (courante) retravailler?.($state.snapshot(courante));
          }}
        >
          ✍️ Au Scénario
        </Bouton>
      {/if}
      <Bouton desactive={bloque} onclick={() => void enregistrer()}>Enregistrer</Bouton>
      <Bouton genre="principal" desactive={bloque || workflow.modeles.length === 0} onclick={() => void tourner()}>
        {indices.length > 0 ? 'Tourner la séquence' : 'Tout retourner'}
      </Bouton>
    </div>

    {#if courante.film}
      <h3>Le film</h3>
      <VideoCreation id={courante.film} />
    {/if}

    {#if etat.corbeille.length > 0}
      <details bind:open={corbeilleOuverte}>
        <summary>Séquences à la corbeille ({etat.corbeille.length})</summary>
        <ul class="corbeille">
          {#each etat.corbeille as s (s.id)}
            <li>
              <span>{s.titre}</span>
              <span class="discret">
                {s.plans} plan{s.plans > 1 ? 's' : ''}{s.date
                  ? ` · le ${new Date(s.date).toLocaleDateString('fr-FR')}`
                  : ''}
              </span>
              <Bouton genre="discret" petit onclick={() => restaurer(s.id)}>Restaurer</Bouton>
            </li>
          {/each}
        </ul>
        <Bouton genre="discret" petit onclick={vider}>Vider la corbeille</Bouton>
      </details>
    {/if}
  {/if}
</Panneau>

{#if choixRush !== null}
  <Dialogue titre={choixRush === 'personnage' ? 'Quel personnage pour la séquence ?' : 'Quel rush pour ce plan ?'} fermer={() => (choixRush = null)}>
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
  .actions,
  .corps-plan {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px;
    margin: 8px 0;
  }

  .actions {
    justify-content: flex-end;
  }

  .large {
    flex: 1;
    min-width: 220px;
  }

  label {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  label.case {
    flex-direction: row;
    align-items: center;
  }

  .etiquette {
    font-size: 12px;
    color: var(--texte-2);
  }

  h3 {
    margin: 16px 0 6px;
    font-size: 14px;
  }

  .casting {
    display: flex;
    flex-wrap: wrap;
    gap: 16px;
  }

  .emplacement {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 6px;
  }

  .grille {
    display: grid;
    flex: 1;
    grid-template-columns: repeat(auto-fill, minmax(200px, 1fr));
    gap: 10px;
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
    padding: 10px;
    border: 1px solid var(--ligne);
  }

  .plans li.a-jour {
    border-color: var(--cyan);
  }

  .etat-plan {
    flex: 1;
  }

  .actions-plan {
    display: flex;
    gap: 4px;
  }

  .corps-plan {
    align-items: flex-start;
  }

  .textes {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: 6px;
    min-width: 240px;
  }

  textarea.manquant {
    border-color: var(--orange);
  }

  .corbeille {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 0;
    list-style: none;
  }

  .corbeille li {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .alerte {
    color: var(--orange);
  }
</style>
