<!--
  Casting, comme la maquette : mêmes prompts et mêmes seeds sur plusieurs modèles d'une tâche, puis comparaison à
  l'aveugle. « Nouveau casting » : le panneau accentué « Une tâche, des candidats » (tâches en cartes à onglet,
  candidats à cocher, jeu d'essai, suivi du casting en cours et le seul bouton orange) puis l'historique des castings.
  « Comparer » : notes de 1 à 5 en étoiles, « Révéler les modèles » (le seul orange tant que les noms sont cachés,
  confirmé : ça ne se défait pas), puis le bilan des modèles révélés (le seul orange devient « Choisir par défaut » du
  mieux noté) ; une image de texte vers image se fige comme image de départ des retouches. Les images de départ sont les
  tiennes : figées depuis un casting, importées d'un fichier ou prises dans les Rushes (la v2 en livrait, pas la v3, pour
  une raison de licence). Un casting passe par la file du Plateau : il se suit ici et s'annule. Le Casting LLM fait
  réécrire les idées par chaque candidat, contrôle les prompts, puis les fait rendre par le modèle par défaut de Texte
  vers image.
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import {
    annulerCommande,
    favoriCasting,
    imageCasting,
    importerSourceCasting,
    lireCasting,
    lireCastings,
    lancerCasting,
    messageErreur,
    noterCasting,
    revelerCasting,
    sourceCasting,
    sourceCastingDepuisRush,
    sourceDepuisCasting,
    type EtatCasting,
    type GenerationCasting,
    type ModeleCasting,
    type TacheCasting,
    type VueCasting,
  } from '../lib/api';
  import {
    AVEC_SOURCE,
    aNoter as imagesANoter,
    avancementPrompt,
    blocageChoix,
    candidatsChoisis,
    cleDuMeilleur,
    CONTROLES,
    dateCasting,
    etatCasting,
    moyennes,
    NOMS_TACHES,
    nombreDe,
    resumeTache,
    titreTache,
    totalChoisi,
    unite,
  } from '../lib/casting';
  import ASavoir from '../lib/composants/ASavoir.svelte';
  import Bouton from '../lib/composants/Bouton.svelte';
  import CarteChoix from '../lib/composants/CarteChoix.svelte';
  import Dialogue from '../lib/composants/Dialogue.svelte';
  import MediaCasting from '../lib/composants/MediaCasting.svelte';
  import Panneau from '../lib/composants/Panneau.svelte';
  import SousOnglets from '../lib/composants/SousOnglets.svelte';
  import ChoixRushes from './ChoixRushes.svelte';

  interface Props {
    signaler: (texte: string, erreur?: boolean) => void;
  }

  let { signaler }: Props = $props();

  let etat = $state<EtatCasting | null>(null);
  let sousOnglet = $state('lancer');
  let tache = $state<TacheCasting>('texte_vers_image');
  /** Les candidats décochés, par identifiant (les complets sont cochés d'office). */
  let exclus = $state<Record<string, boolean>>({});
  let affiche = $state<string | null>(null);
  let casting = $state<VueCasting | null>(null);
  let confirmerRevelation = $state(false);
  let occupe = $state(false);
  let lancement = $state(false);
  /** Le casting suivi était en cours au dernier relevé : sa fin se remarque. */
  let suivi = $state<string | null>(null);
  /** L'image de départ qu'on ajoute (son nom), par un fichier ou depuis les Rushes. */
  let sourceVisee = $state<string | null>(null);
  let choixRush = $state(false);
  let rushChoisi = $state<string[]>([]);
  let fichierSource = $state<HTMLInputElement | null>(null);
  /** Change quand une image de départ est remplacée : sa vignette se relit. */
  let versionSources = $state(0);

  const COMMENTAIRES: Record<string, string> = {
    lancer:
      "Chaque modèle part d'une mémoire vidéo vide. La première génération, « à froid », est comptée à part. Tous les modèles cochés passent, dans un ordre tiré au hasard que tu ne vois pas ; le casting passe par la file du Plateau, qui tourne une chose à la fois. Au Casting LLM, les prompts écrits et leurs contrôles se lisent après la révélation.",
    comparer:
      'Les noms des modèles sont cachés tant que tu ne les as pas révélés : note d’abord, le résultat n’est pas faussé.',
  };

  const preparation = $derived(etat?.preparation.taches.find((t) => t.tache === tache) ?? null);
  const estLlm = $derived(preparation?.tache === 'reecriture');
  const choisis = $derived(preparation ? candidatsChoisis(preparation, exclus) : []);
  const blocage = $derived(preparation ? blocageChoix(preparation, choisis) : null);
  const aNoter = $derived(imagesANoter(etat?.castings ?? []));
  const onglets = $derived([
    { id: 'lancer', titre: 'Nouveau casting' },
    { id: 'comparer', titre: aNoter > 0 ? `Comparer · ${aNoter} à noter` : 'Comparer' },
  ]);
  const moyennesModeles = $derived(casting ? moyennes(casting.modeles) : []);
  const meilleure = $derived(Math.max(...moyennesModeles.filter((m): m is number => m !== null)));
  /** Le seul bouton orange du bilan : le mieux noté, s'il n'est pas déjà le modèle par défaut. */
  const cleMeilleur = $derived(casting?.revele ? cleDuMeilleur(casting.modeles, casting.par_defaut) : null);
  const llm = $derived(casting?.tache === 'reecriture');
  const actif = $derived(etat?.actif ?? null);
  const enCours = $derived(actif !== null && (actif.etat === 'attente' || actif.etat === 'en_cours'));

  async function relire() {
    try {
      etat = await lireCastings();
      remarquerFin();
      const ids = etat.castings.map((c) => c.id);
      if (!affiche || !ids.includes(affiche)) affiche = ids[0] ?? null;
      if (affiche) casting = await lireCasting(affiche);
      else casting = null;
    } catch (e) {
      signaler(messageErreur(e), true);
    }
  }

  /** Un casting suivi vient de finir : il se compare, ou son erreur se dit. */
  function remarquerFin() {
    const a = etat?.actif ?? null;
    const encore = a !== null && (a.etat === 'attente' || a.etat === 'en_cours');
    if (suivi && a && a.id === suivi && !encore) {
      if (a.etat === 'termine') {
        signaler(`Casting terminé · ${NOMS_TACHES[a.tache] ?? a.tache} : résultats à comparer.`);
        affiche = a.id;
        sousOnglet = 'comparer';
      } else if (a.etat === 'erreur') {
        signaler(a.erreur ?? 'Le casting a échoué.', true);
      } else {
        signaler('Casting annulé : les générations faites restent à comparer.');
      }
    }
    suivi = encore && a ? a.id : null;
  }

  async function lancer() {
    const p = preparation;
    if (!p) return;
    lancement = true;
    try {
      // Tous les candidats cochés : le cœur prend tous ceux de la tâche ; sinon, la liste choisie.
      const liste = p.tache === 'reecriture' || choisis.length === p.candidats.length ? null : choisis;
      await lancerCasting(tache, liste);
      signaler(`Casting lancé : ${NOMS_TACHES[tache]}.`);
      await relire();
    } catch (e) {
      signaler(messageErreur(e), true);
    } finally {
      lancement = false;
    }
  }

  async function annuler() {
    if (!actif) return;
    try {
      await annulerCommande(actif.commande);
      await relire();
    } catch (e) {
      signaler(messageErreur(e), true);
    }
  }

  async function afficher(id: string) {
    affiche = id;
    try {
      casting = await lireCasting(id);
    } catch (e) {
      signaler(messageErreur(e), true);
    }
  }

  /** Un casting de l'historique : il s'ouvre dans « Comparer ». */
  async function ouvrir(id: string) {
    await afficher(id);
    sousOnglet = 'comparer';
  }

  /** Une action sur le casting affiché ; la page ne bouge pas sous le doigt pendant qu'on note. */
  async function agir(action: (id: string) => Promise<VueCasting>) {
    const id = affiche;
    if (!id || occupe) return;
    occupe = true;
    try {
      const hauteur = window.scrollY;
      casting = await action(id);
      window.scrollTo(0, hauteur);
      if (etat) {
        etat.castings = etat.castings.map((c) =>
          c.id === id && casting ? { ...c, notees: casting.notees, revele: casting.revele, favori: casting.favori } : c,
        );
      }
    } catch (e) {
      signaler(messageErreur(e), true);
    } finally {
      occupe = false;
    }
  }

  function noter(m: ModeleCasting, g: GenerationCasting, note: number) {
    void agir((id) => noterCasting(id, m.cle, g.fichier, note));
  }

  function basculer(id: string) {
    exclus[id] = !exclus[id];
  }

  function importerSource(nom: string) {
    sourceVisee = nom;
    fichierSource?.click();
  }

  function sourceDepuisRushes(nom: string) {
    sourceVisee = nom;
    rushChoisi = [];
    choixRush = true;
  }

  async function poserSource(action: (nom: string) => Promise<EtatCasting>, quoi: string) {
    const nom = sourceVisee;
    if (!nom) return;
    try {
      etat = await action(nom);
      versionSources += 1;
      signaler(`Image de départ « ${nom.replace(/\.png$/, '')} » ${quoi}.`);
    } catch (e) {
      signaler(messageErreur(e), true);
    } finally {
      sourceVisee = null;
    }
  }

  function fichierChoisi(e: Event & { currentTarget: HTMLInputElement }) {
    const fichier = e.currentTarget.files?.[0];
    e.currentTarget.value = '';
    if (!fichier) return;
    const lecteur = new FileReader();
    lecteur.onload = () => {
      if (typeof lecteur.result === 'string') {
        const image = lecteur.result;
        void poserSource((nom) => importerSourceCasting(nom, image, fichier.name), 'importée');
      }
    };
    lecteur.readAsDataURL(fichier);
  }

  function rushValide() {
    const rush = rushChoisi[0];
    choixRush = false;
    if (rush) void poserSource((nom) => sourceCastingDepuisRush(nom, rush), 'prise dans les Rushes');
  }

  function sauter(prompt: string) {
    document.getElementById(`prompt-casting-${prompt}`)?.scrollIntoView({ block: 'start', behavior: 'smooth' });
  }

  function mesure(r: Record<string, number | string | null> | null, cle: string, unite: string): string {
    const v = r?.[cle];
    return v === null || v === undefined ? '—' : `${String(v).replace('.', ',')}${unite}`;
  }

  onMount(() => {
    void relire();
    const minuterie = setInterval(() => {
      if (enCours) void relire();
    }, 2000);
    return () => clearInterval(minuterie);
  });
</script>

<h1>Casting</h1>
<p class="sous-titre">Fais tourner plusieurs modèles sur les mêmes prompts, et choisis à l'aveugle.</p>
<SousOnglets {onglets} bind:actif={sousOnglet} />
<ASavoir>{COMMENTAIRES[sousOnglet] ?? ''}</ASavoir>

{#if sousOnglet === 'lancer'}
  {#if etat}
    <Panneau titre="Une tâche, des candidats" accentue>
      <div class="cartes" role="radiogroup" aria-label="Tâche du casting">
        {#each etat.preparation.taches as t (t.tache)}
          <CarteChoix groupe="tache-casting" valeur={t.tache} titre={titreTache(t.tache)} bind:choisie={tache}>
            {resumeTache(t)}
          </CarteChoix>
        {/each}
      </div>
      {#if preparation}
        <hr />
        <p class="etiquette">{estLlm ? 'Candidats comparés' : 'Candidats'}</p>
        <ul class="candidats">
          {#each preparation.candidats as c (c.id)}
            <li>
              <label class="coche">
                <input
                  type="checkbox"
                  checked={estLlm || (c.manquants === 0 && !exclus[c.id])}
                  disabled={estLlm || c.manquants > 0 || enCours}
                  onchange={() => basculer(c.id)}
                />
                {c.nom}
              </label>
              {#if c.manquants > 0}
                <span class="puce ambre">
                  incomplet · {nombreDe(c.manquants, 'fichier')} {c.manquants > 1 ? 'manquent' : 'manque'}
                </span>
              {:else}
                <span class="puce">installé</span>
              {/if}
            </li>
          {:else}
            <li class="discret">Aucun candidat pour l'instant.</li>
          {/each}
        </ul>
        {#if estLlm}
          <p class="discret">Tous les LLM installés passent : il n'y a rien à cocher.</p>
        {/if}
        {#if preparation.rendu}
          <p class="discret">
            Chaque candidat réécrit les idées, puis {preparation.rendu.nom} les rend en image : c'est le modèle par défaut
            de Texte vers image.
          </p>
        {/if}

        <div class="essai-boite">
          <strong>{estLlm ? 'Idées à réécrire' : "Jeu d'essai"}</strong>
          <span>
            {#if estLlm}
              {nombreDe(preparation.essais.length, 'idée')}, une seed de rendu : {preparation.seeds[0] ?? ''}.
            {:else}
              {nombreDe(preparation.essais.length, 'prompt')} livré{preparation.essais.length > 1 ? 's' : ''},
              {nombreDe(preparation.seeds.length, 'seed')} chacun ({preparation.seeds.join(' et ')}, les mêmes pour tous
              les modèles).
            {/if}
            {#if choisis.length >= 2}
              <b>{nombreDe(totalChoisi(preparation, choisis), unite(preparation.tache))} en tout</b>{estLlm
                ? ` : ${nombreDe(preparation.candidats.length, 'candidat')} × ${nombreDe(preparation.essais.length, 'idée')}, une réécriture puis un rendu.`
                : ` : ${nombreDe(choisis.length, 'modèle')} × ${nombreDe(preparation.essais.length, 'prompt')} × ${nombreDe(preparation.seeds.length, 'seed')}.`}
            {/if}
          </span>
        </div>
        <div class="essais">
          {#each preparation.essais as e (e.id)}
            <span class="essai" title={e.prompt}><b>{e.id}</b> <i>· {e.teste}</i></span>
          {/each}
        </div>

        {#if preparation.sources.length > 0}
          <hr />
          <p class="etiquette">Images de départ</p>
          <p class="discret">
            Ce sont les tiennes. Chaque prompt du jeu d'essai retouche ou anime l'image de son nom : importe une image, prends-la
            dans les Rushes, ou fige-en une depuis un casting Texte vers image (« Utiliser comme source »).
          </p>
          <div class="sources">
            {#each preparation.sources as s (s.nom)}
              <figure>
                {#if s.presente}
                  {#key versionSources}
                    <MediaCasting lire={() => sourceCasting(s.nom)} fichier={s.nom} titre={s.nom} />
                  {/key}
                {:else}
                  <span class="alerte">à ajouter</span>
                {/if}
                <span class="ajout">
                  <Bouton genre="discret" petit desactive={enCours} onclick={() => importerSource(s.nom)}>Importer…</Bouton>
                  <Bouton genre="discret" petit desactive={enCours} onclick={() => sourceDepuisRushes(s.nom)}>Rushes…</Bouton>
                </span>
                <figcaption>{s.nom.replace(/\.png$/, '')}</figcaption>
              </figure>
            {/each}
          </div>
          <input
            bind:this={fichierSource}
            class="cache"
            type="file"
            accept="image/png,image/jpeg,image/webp"
            onchange={fichierChoisi}
          />
        {/if}

        {#if blocage}
          <p class="alerte">{blocage}{blocage.endsWith('.') ? '' : '.'}</p>
        {:else}
          <p class="discret">
            Le casting démarre si moins de {String(etat.preparation.vramMaxOccupeeGo).replace('.', ',')} Go de VRAM sont
            occupés.
          </p>
        {/if}

        {#if actif && enCours}
          <div class="rendu">
            <span class="etape">
              {NOMS_TACHES[actif.tache] ?? actif.tache} · {actif.fait} / {actif.total} · {actif.courant || 'préparation…'}
            </span>
            <progress max={Math.max(actif.total, 1)} value={actif.fait}></progress>
            <Bouton genre="discret" petit onclick={() => void annuler()}>✕ Annuler</Bouton>
          </div>
        {/if}
        <div class="actions">
          <Bouton genre="principal" desactive={enCours || lancement || blocage !== null} onclick={() => void lancer()}>
            Lancer le casting
          </Bouton>
        </div>
      {/if}
    </Panneau>

    <Panneau titre="Historique">
      {#if etat.castings.length > 0}
        <div class="cartes c3" role="radiogroup" aria-label="Castings faits">
          {#each etat.castings as c (c.id)}
            <CarteChoix groupe="historique" valeur={c.id} titre={dateCasting(c.date)} bind:choisie={() => '', (v) => void ouvrir(v)}>
              {NOMS_TACHES[c.tache] ?? c.tache} · {nombreDe(c.nb_workflows, 'modèle')} ·
              {#if c.revele}
                révélé
              {:else}
                <b>{c.total - c.notees} à noter</b>
              {/if}
            </CarteChoix>
          {/each}
        </div>
      {:else}
        <p class="discret">Aucun casting pour l'instant : le premier que tu lances apparaît ici.</p>
      {/if}
    </Panneau>
  {:else}
    <p class="discret">Lecture…</p>
  {/if}
{:else if etat && etat.castings.length === 0}
  <Panneau titre="Aucun casting pour l'instant">
    <p>Les castings faits avec la version 2 (dans <code>data\bancs\</code>) apparaissent ici.</p>
  </Panneau>
{:else if etat}
  {#if etat.castings.length > 1}
    <label class="choix-casting">
      <span class="discret">Casting affiché</span>
      <select value={affiche ?? ''} onchange={(e) => void afficher(e.currentTarget.value)}>
        {#each etat.castings as c (c.id)}
          <option value={c.id}>{NOMS_TACHES[c.tache] ?? c.tache} · {dateCasting(c.date)} · {etatCasting(c)}</option>
        {/each}
      </select>
    </label>
  {/if}

  {#if casting}
    {@const c = casting}
    <Panneau
      titre={c.revele
        ? `${NOMS_TACHES[c.tache] ?? c.tache} · ${dateCasting(c.date)}`
        : `Comparer à l'aveugle · ${NOMS_TACHES[c.tache] ?? c.tache} · ${nombreDe(c.prompts.length, 'prompt')}`}
      accentue={!c.revele}
    >
      <div class="tete">
        <span>{c.notees} / {c.total} {c.tache.endsWith('video') ? 'clips notés' : 'images notées'}</span>
        <progress max={Math.max(c.total, 1)} value={c.notees}></progress>
        <span class="discret">VRAM au repos : {c.vram_base_go ?? '—'} Go</span>
      </div>
      <div class="sauts">
        {#each c.prompts as p (p.id)}
          {@const a = avancementPrompt(c, p.id)}
          <Bouton genre="discret" petit onclick={() => sauter(p.id)}>
            {p.id} {a.notees === a.total ? '✓' : `${a.notees}/${a.total}`}
          </Bouton>
        {/each}
      </div>

      {#each c.prompts as p (p.id)}
        {@const sert = c.modeles.some((m) => m.generations.some((g) => g.prompt === p.id && g.peut_servir_de_source))}
        <section class="bloc-prompt" id={`prompt-casting-${p.id}`}>
          <h4>
            {p.id} <span class="discret">· {p.teste}</span>
            {#if sert}<span class="sert">sert d'image de départ à la Retouche</span>{/if}
          </h4>
          <p class="discret">{p.prompt}</p>
          <div class="grille" style:grid-template-columns={`repeat(${c.modeles.length + (AVEC_SOURCE.includes(c.tache) ? 1 : 0)}, minmax(0, 1fr))`}>
            {#if AVEC_SOURCE.includes(c.tache) && p.source}
              {@const nom = p.source}
              <div class="colonne">
                <h5>Source</h5>
                <MediaCasting lire={() => sourceCasting(nom)} fichier={nom} titre="source" />
              </div>
            {/if}
            {#each c.modeles as m (m.cle)}
              <div class="colonne">
                <h5>{m.nom} <span class="discret">{c.seeds.length > 1 ? 'seeds' : 'seed'} {c.seeds.join(' · ')}</span></h5>
                <div class="seeds" style:grid-template-columns={`repeat(${Math.max(c.seeds.length, 1)}, minmax(0, 1fr))`}>
                  {#each m.generations.filter((g) => g.prompt === p.id) as g (g.fichier)}
                    <div class="vignette" class:source={g.est_la_source}>
                      <MediaCasting lire={() => imageCasting(c.id, m.cle, g.fichier)} fichier={g.fichier} titre={`seed ${g.seed}`} />
                      <div class="etoiles" role="group" aria-label="Note">
                        {#each [1, 2, 3, 4, 5] as n (n)}
                          <button
                            type="button"
                            class="etoile"
                            class:pleine={(g.note ?? 0) >= n}
                            title={`${n}/5`}
                            disabled={occupe}
                            onclick={() => noter(m, g, n)}>★</button
                          >
                        {/each}
                      </div>
                      {#if g.peut_servir_de_source}
                        <div class="etoiles">
                          {#if g.est_la_source}
                            <span class="ok">source ✓</span>
                          {:else}
                            <Bouton
                              petit
                              desactive={occupe}
                              onclick={() => void agir((id) => sourceDepuisCasting(id, m.cle, g.fichier))}
                            >
                              Utiliser comme source
                            </Bouton>
                          {/if}
                        </div>
                      {/if}
                      {#if g.texte}
                        <p class="discret texte-prompt">{g.texte}</p>
                        {#if g.controles}
                          {@const ctl = g.controles}
                          <p class="discret">
                            {#each CONTROLES.filter(([cle]) => ctl[cle] !== null && ctl[cle] !== undefined) as [cle, nom] (cle)}
                              <span class:ok={ctl[cle] === true} class:alerte={ctl[cle] !== true}>
                                {ctl[cle] === true ? '✓' : '✗'} {nom}
                              </span>
                              ·
                            {/each}
                            {ctl['mots'] ?? '—'} mots
                          </p>
                        {/if}
                      {/if}
                    </div>
                  {/each}
                </div>
              </div>
            {/each}
          </div>
        </section>
      {/each}

      {#if !c.revele}
        <div class="actions">
          <Bouton genre="principal" desactive={occupe} onclick={() => (confirmerRevelation = true)}>Révéler les modèles</Bouton>
        </div>
      {/if}
    </Panneau>

    {#if c.revele}
      <Panneau titre="Bilan · modèles révélés" accentue={cleMeilleur !== null}>
        <table class="resume">
          <thead>
            <tr>
              <th>Modèle</th>
              <th>Note moyenne</th>
              {#if llm}
                <th>Chargement</th><th>Écriture</th><th>Durée moyenne</th><th>Pic de VRAM</th><th>Contrôles</th>
              {:else}
                <th>1re image « à froid »</th><th>À chaud</th><th>Pic de RAM</th><th>VRAM avant → après libération</th>
              {/if}
              <th>Par défaut</th>
            </tr>
          </thead>
          <tbody>
            {#each c.modeles as m, i (m.cle)}
              {@const moyenne = moyennesModeles[i] ?? null}
              <tr>
                <td>{m.nom}</td>
                <td class:meilleure={moyenne !== null && moyenne === meilleure}>
                  {moyenne === null ? '—' : `${moyenne.toFixed(2).replace('.', ',')} ★`}
                </td>
                {#if llm}
                  <td>{mesure(m.resume, 'chargement_s', ' s')}</td>
                  <td>{mesure(m.resume, 'ecriture_tps_moyen', ' tok/s')}</td>
                  <td>{mesure(m.resume, 'duree_moyenne_s', ' s')}</td>
                  <td>{mesure(m.resume, 'pic_vram_go', ' Go')}</td>
                  <td>{mesure(m.resume, 'controles_reussis', '')}</td>
                {:else}
                  <td>{mesure(m.resume, 'temps_froid_s', ' s')}</td>
                  <td>{mesure(m.resume, 'temps_chaud_moyen_s', ' s')}</td>
                  <td>{mesure(m.resume, 'pic_ram_go', ' Go')}</td>
                  <td>{m.liberation?.vram_avant_go ?? '—'} → {m.liberation?.vram_apres_go ?? '—'} Go</td>
                {/if}
                <td>
                  {#if c.par_defaut === m.cle}
                    <span class="ok">★ par défaut</span>
                  {:else if !m.choisissable}
                    <span class="discret">Pas un LLM : il ne peut pas être le LLM par défaut</span>
                  {:else}
                    <Bouton
                      genre={m.cle === cleMeilleur ? 'principal' : 'secondaire'}
                      petit
                      desactive={occupe}
                      onclick={() => void agir((id) => favoriCasting(id, m.cle))}
                    >
                      Choisir par défaut
                    </Bouton>
                    {#if c.favori === m.cle}<span class="discret">choisi ici, changé depuis</span>{/if}
                  {/if}
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
        <p class="discret">
          {llm
            ? "Le LLM par défaut sert aux modèles qui n'ont pas le leur, au Scénario et aux traductions."
            : 'Le modèle par défaut est celui que prennent le Plateau, le Montage et le Scénario pour cette tâche.'}
        </p>
      </Panneau>
    {/if}
  {/if}
{:else}
  <p class="discret">Lecture…</p>
{/if}

{#if confirmerRevelation && casting}
  <Dialogue titre="Révéler les modèles ?" fermer={() => (confirmerRevelation = false)}>
    <ASavoir>
      Tes notes sont enregistrées d'abord. Une fois les noms connus, tu ne peux plus noter à l'aveugle ce casting.
    </ASavoir>
    {#if casting.notees < casting.total}
      <p class="discret">
        Seulement {casting.notees} {casting.tache.endsWith('video') ? 'clips notés' : 'images notées'} sur {casting.total} :
        tu pourras encore noter après, mais plus à l'aveugle.
      </p>
    {/if}
    {#snippet pied()}
      <Bouton genre="discret" onclick={() => (confirmerRevelation = false)}>Pas encore</Bouton>
      <Bouton
        genre="principal"
        onclick={() => {
          confirmerRevelation = false;
          void agir((id) => revelerCasting(id));
        }}>Révéler</Bouton
      >
    {/snippet}
  </Dialogue>
{/if}

{#if choixRush}
  <Dialogue titre={`Image de départ « ${(sourceVisee ?? '').replace(/\.png$/, '')} »`} fermer={() => (choixRush = false)}>
    <ChoixRushes max={1} bind:selection={rushChoisi} {signaler} />
    {#snippet pied()}
      <Bouton genre="discret" onclick={() => (choixRush = false)}>Annuler</Bouton>
      <Bouton genre="principal" desactive={rushChoisi.length === 0} onclick={rushValide}>Choisir</Bouton>
    {/snippet}
  </Dialogue>
{/if}

<style>
  /* Les cartes à onglet : une colonne par choix. */
  .cartes {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(150px, 1fr));
    gap: 12px;
    align-items: stretch;
    margin: 4px 0;
  }

  .cartes.c3 {
    grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
  }

  hr {
    margin: 14px 0 10px;
    border: 0;
    border-top: 1px solid var(--ligne);
  }

  .etiquette {
    margin: 0 0 6px;
    font-size: 12px;
    letter-spacing: 0.06em;
    color: var(--texte-2);
  }

  h4 {
    margin: 0 0 6px;
    font-size: 14px;
  }

  h5 {
    margin: 0 0 6px;
    font-size: 13px;
  }

  .choix-casting {
    display: flex;
    align-items: center;
    gap: 10px;
    margin: 12px 0;
  }

  .candidats {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .candidats li {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .coche {
    display: flex;
    flex: 0 1 auto;
    align-items: center;
    gap: 8px;
  }

  .puce {
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

  .essai-boite {
    display: flex;
    flex-wrap: wrap;
    gap: 6px 14px;
    margin: 12px 0 8px;
    padding: 10px 16px;
    border: 1px solid var(--ligne);
    background: var(--fond);
  }

  .essai-boite strong {
    font-size: 12px;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--texte-2);
  }

  .essais,
  .sauts,
  .tete,
  .actions,
  .rendu {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    margin: 8px 0;
  }

  .actions {
    justify-content: flex-end;
  }

  .rendu {
    gap: 12px;
  }

  .essai {
    padding: 2px 8px;
    border: 1px solid var(--ligne);
    font-size: 12px;
  }

  .cache {
    display: none;
  }

  .ajout {
    display: flex;
    gap: 4px;
  }

  .sources {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(120px, 1fr));
    gap: 8px;
  }

  figure {
    margin: 0;
  }

  figcaption {
    font-size: 12px;
    color: var(--texte-2);
  }

  progress {
    flex: 1;
    min-width: 120px;
    height: 6px;
    accent-color: var(--cyan);
  }

  .resume {
    width: 100%;
    margin: 4px 0 10px;
    border-collapse: collapse;
    font-size: 13px;
  }

  .resume th,
  .resume td {
    padding: 6px 10px 6px 0;
    border-bottom: 1px solid var(--ligne);
    text-align: left;
    vertical-align: top;
  }

  .resume th {
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.12em;
    text-transform: uppercase;
    color: var(--texte-2);
  }

  .meilleure {
    color: var(--cyan);
    font-weight: 600;
  }

  .bloc-prompt {
    margin-top: 18px;
  }

  .sert {
    margin-left: 8px;
    font-size: 12px;
    color: var(--cyan);
  }

  .grille,
  .seeds {
    display: grid;
    gap: 8px;
  }

  .vignette.source {
    outline: 1px solid var(--cyan);
  }

  .etoiles {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: center;
    gap: 2px;
    margin-top: 4px;
  }

  .etoile {
    padding: 0 2px;
    border: none;
    background: none;
    color: var(--texte-2);
    font: inherit;
    font-size: 18px;
    cursor: pointer;
  }

  .etoile.pleine {
    color: var(--ambre);
  }

  .texte-prompt {
    font-size: 12px;
  }

  .ok {
    color: var(--cyan);
  }

  .alerte {
    color: var(--orange);
  }
</style>
