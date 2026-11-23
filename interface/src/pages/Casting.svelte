<!--
  Casting, comme en v2 (web/casting.js) : mêmes prompts et mêmes seeds sur plusieurs modèles d'une tâche, puis
  comparaison à l'aveugle. « Nouveau casting » dit ce qu'un casting comparerait et ce qui l'empêche de partir ;
  « Comparer » montre les castings faits : notes de 1 à 5, révélation (elle ne se défait pas), modèle par défaut, et
  une image de texte vers image figée comme image de départ des retouches. Un casting passe par la file du Plateau :
  il se suit ici et s'annule. Le Casting LLM vient avec la suite du portage.
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import {
    annulerCommande,
    favoriCasting,
    imageCasting,
    lireCasting,
    lireCastings,
    lancerCasting,
    messageErreur,
    noterCasting,
    revelerCasting,
    sourceCasting,
    sourceDepuisCasting,
    type EtatCasting,
    type GenerationCasting,
    type ModeleCasting,
    type TacheCasting,
    type VueCasting,
  } from '../lib/api';
  import {
    AVEC_SOURCE,
    avancementPrompt,
    CONTROLES,
    dateCasting,
    etatCasting,
    moyennes,
    NOMS_TACHES,
    nombreDe,
    resumeTache,
    unite,
  } from '../lib/casting';
  import ASavoir from '../lib/composants/ASavoir.svelte';
  import Bouton from '../lib/composants/Bouton.svelte';
  import Dialogue from '../lib/composants/Dialogue.svelte';
  import MediaCasting from '../lib/composants/MediaCasting.svelte';
  import Panneau from '../lib/composants/Panneau.svelte';
  import SousOnglets from '../lib/composants/SousOnglets.svelte';

  interface Props {
    signaler: (texte: string, erreur?: boolean) => void;
  }

  let { signaler }: Props = $props();

  let etat = $state<EtatCasting | null>(null);
  let sousOnglet = $state('lancer');
  let tache = $state<TacheCasting>('texte_vers_image');
  let affiche = $state<string | null>(null);
  let casting = $state<VueCasting | null>(null);
  let confirmerRevelation = $state(false);
  let occupe = $state(false);
  let lancement = $state(false);
  /** Le casting suivi était en cours au dernier relevé : sa fin se remarque. */
  let suivi = $state<string | null>(null);

  const preparation = $derived(etat?.preparation.taches.find((t) => t.tache === tache) ?? null);
  const aNoter = $derived(etat?.castings.filter((c) => !c.revele).length ?? 0);
  const onglets = $derived([
    { id: 'lancer', titre: 'Nouveau casting' },
    { id: 'comparer', titre: aNoter > 0 ? `Comparer · ${aNoter} à noter` : 'Comparer' },
  ]);
  const moyennesModeles = $derived(casting ? moyennes(casting.modeles) : []);
  const meilleure = $derived(Math.max(...moyennesModeles.filter((m): m is number => m !== null)));
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
    lancement = true;
    try {
      await lancerCasting(tache);
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

  function reveler() {
    if (casting && casting.notees < casting.total) {
      confirmerRevelation = true;
      return;
    }
    void agir((id) => revelerCasting(id));
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
<p class="sous-titre">Compare les modèles d'une tâche à l'aveugle, sur les mêmes prompts et les mêmes seeds.</p>
<SousOnglets {onglets} bind:actif={sousOnglet} />

{#if sousOnglet === 'lancer'}
  <ASavoir>
    Tous les modèles de la tâche passent, dans un ordre tiré au hasard que tu ne vois pas. Le casting passe par la file
    du Plateau, qui tourne une chose à la fois. Le Casting LLM arrive avec la suite du portage.
  </ASavoir>
  {#if actif && enCours}
    <Panneau titre={`Casting en cours · ${NOMS_TACHES[actif.tache] ?? actif.tache}`}>
      <progress max={Math.max(actif.total, 1)} value={actif.fait}></progress>
      <div class="actions">
        <span class="discret">{actif.fait} / {actif.total} · {actif.courant || 'préparation…'}</span>
        <Bouton genre="discret" petit onclick={() => void annuler()}>✕ Annuler</Bouton>
      </div>
    </Panneau>
  {/if}
  {#if etat}
    <div class="taches" role="radiogroup" aria-label="Tâche du casting">
      {#each etat.preparation.taches as t (t.tache)}
        <label class="carte-tache" class:bloquee={t.blocage !== null}>
          <input type="radio" name="tache-casting" value={t.tache} bind:group={tache} />
          <strong>{NOMS_TACHES[t.tache]}</strong>
          <small class="discret">{resumeTache(t)}</small>
        </label>
      {/each}
    </div>
    {#if preparation}
      <Panneau titre={NOMS_TACHES[preparation.tache]} accentue>
        <h3>{preparation.tache === 'reecriture' ? 'Candidats comparés' : 'Modèles comparés'}</h3>
        <ul class="candidats">
          {#each preparation.candidats as c (c.id)}
            <li>
              <strong>{c.nom}</strong>
              <span class:alerte={c.manquants > 0} class:ok={c.manquants === 0}>
                {c.manquants > 0 ? `incomplet · ${nombreDe(c.manquants, 'fichier')} ${c.manquants > 1 ? 'manquent' : 'manque'}` : 'prêt'}
              </span>
            </li>
          {:else}
            <li class="discret">Aucun candidat pour l'instant.</li>
          {/each}
        </ul>
        {#if preparation.rendu}
          <p class="discret">
            Chaque candidat réécrit les idées, puis {preparation.rendu.nom} les rend en image : c'est le modèle par défaut
            de Texte vers image.
          </p>
        {/if}
        <h3>{preparation.tache === 'reecriture' ? 'Idées à réécrire' : "Jeu d'essai"}</h3>
        <div class="essais">
          {#each preparation.essais as e (e.id)}
            <span class="essai" title={e.prompt}><b>{e.id}</b> <i>· {e.teste}</i></span>
          {/each}
        </div>
        <p class="discret">
          {preparation.tache === 'reecriture'
            ? `Une seed de rendu : ${preparation.seeds[0] ?? ''}.`
            : `${nombreDe(preparation.seeds.length, 'seed')} par prompt : ${preparation.seeds.join(' et ')}. Les mêmes pour tous les modèles.`}
        </p>
        {#if preparation.sources.length > 0}
          <h3>Images de départ</h3>
          <div class="sources">
            {#each preparation.sources as s (s.nom)}
              <figure>
                {#if s.presente}
                  <MediaCasting lire={() => sourceCasting(s.nom)} fichier={s.nom} titre={s.nom} />
                {:else}
                  <span class="alerte">absente</span>
                {/if}
                <figcaption>{s.nom.replace(/\.png$/, '')}</figcaption>
              </figure>
            {/each}
          </div>
        {/if}
        {#if preparation.candidats.length >= 2}
          <p>
            <strong>{nombreDe(preparation.total, unite(preparation.tache))}</strong>
            <span class="discret">
              · {preparation.tache === 'reecriture'
                ? `${nombreDe(preparation.candidats.length, 'candidat')} × ${nombreDe(preparation.essais.length, 'idée')} : une réécriture, puis un rendu`
                : `${nombreDe(preparation.candidats.length, 'modèle')} × ${nombreDe(preparation.essais.length, 'prompt')} × ${nombreDe(preparation.seeds.length, 'seed')}`}
            </span>
          </p>
        {/if}
        {#if preparation.blocage}
          <p class="alerte">{preparation.blocage}.</p>
        {:else}
          <p class="discret">
            Le casting démarre si moins de {String(etat.preparation.vramMaxOccupeeGo).replace('.', ',')} Go de VRAM sont
            occupés.
          </p>
        {/if}
        <div class="actions">
          <Bouton
            genre="principal"
            desactive={enCours || lancement || preparation.blocage !== null || preparation.tache === 'reecriture'}
            onclick={() => void lancer()}
          >
            Lancer le casting
          </Bouton>
        </div>
      </Panneau>
    {/if}
  {:else}
    <p class="discret">Lecture…</p>
  {/if}
{:else if etat && etat.castings.length === 0}
  <Panneau titre="Aucun casting pour l'instant">
    <p>Les castings faits avec la version 2 (dans <code>data\bancs\</code>) apparaissent ici.</p>
  </Panneau>
{:else if etat}
  <div class="castings" role="radiogroup" aria-label="Casting affiché">
    {#each etat.castings as c (c.id)}
      <label class="carte-tache">
        <input type="radio" name="casting-affiche" value={c.id} checked={c.id === affiche} onchange={() => void afficher(c.id)} />
        <strong>{NOMS_TACHES[c.tache] ?? c.tache}</strong>
        <small class="discret">{dateCasting(c.date)} · {nombreDe(c.nb_workflows, 'modèle')}</small>
        <small class:ok={c.revele} class:alerte={!c.revele}>{etatCasting(c)}</small>
      </label>
    {/each}
  </div>

  {#if casting}
    {@const c = casting}
    <Panneau titre={`${NOMS_TACHES[c.tache] ?? c.tache} · ${dateCasting(c.date)}${c.revele ? '' : " · à l'aveugle"}`} accentue={!c.revele}>
      <div class="tete">
        <span>{c.notees} / {c.total} {c.tache.endsWith('video') ? 'clips notés' : 'images notées'}</span>
        <progress max={Math.max(c.total, 1)} value={c.notees}></progress>
        <span class="discret">VRAM au repos : {c.vram_base_go ?? '—'} Go</span>
        {#if !c.revele}
          <Bouton genre="principal" desactive={occupe} onclick={reveler}>Révéler les modèles</Bouton>
        {/if}
      </div>
      <div class="sauts">
        {#each c.prompts as p (p.id)}
          {@const a = avancementPrompt(c, p.id)}
          <Bouton genre="discret" petit onclick={() => sauter(p.id)}>
            {p.id} {a.notees === a.total ? '✓' : `${a.notees}/${a.total}`}
          </Bouton>
        {/each}
      </div>

      {#if c.revele}
        <table class="resume">
          <thead>
            <tr>
              <th>Modèle</th>
              <th>Note moyenne</th>
              {#if llm}
                <th>Chargement</th><th>Écriture</th><th>Durée moyenne</th><th>Pic de VRAM</th><th>Contrôles</th>
              {:else}
                <th>À froid</th><th>À chaud</th><th>Pic de RAM</th><th>VRAM avant → après libération</th>
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
                  {moyenne === null ? '—' : moyenne.toFixed(2).replace('.', ',')}
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
                  {:else}
                    <Bouton petit desactive={occupe} onclick={() => void agir((id) => favoriCasting(id, m.cle))}>
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
      {/if}

      {#each c.prompts as p (p.id)}
        {@const sert = c.modeles.some((m) => m.generations.some((g) => g.prompt === p.id && g.peut_servir_de_source))}
        <section class="bloc-prompt" id={`prompt-casting-${p.id}`}>
          <h3>
            {p.id} <span class="discret">· {p.teste}</span>
            {#if sert}<span class="sert">sert d'image de départ à la Retouche</span>{/if}
          </h3>
          <p class="discret">{p.prompt}</p>
          <div class="grille" style:grid-template-columns={`repeat(${c.modeles.length + (AVEC_SOURCE.includes(c.tache) ? 1 : 0)}, minmax(0, 1fr))`}>
            {#if AVEC_SOURCE.includes(c.tache) && p.source}
              {@const nom = p.source}
              <div class="colonne">
                <h4>Source</h4>
                <MediaCasting lire={() => sourceCasting(nom)} fichier={nom} titre="source" />
              </div>
            {/if}
            {#each c.modeles as m (m.cle)}
              <div class="colonne">
                <h4>{m.nom} <span class="discret">{c.seeds.length > 1 ? 'seeds' : 'seed'} {c.seeds.join(' · ')}</span></h4>
                <div class="seeds" style:grid-template-columns={`repeat(${Math.max(c.seeds.length, 1)}, minmax(0, 1fr))`}>
                  {#each m.generations.filter((g) => g.prompt === p.id) as g (g.fichier)}
                    <div class="vignette" class:source={g.est_la_source}>
                      <MediaCasting lire={() => imageCasting(c.id, m.cle, g.fichier)} fichier={g.fichier} titre={`seed ${g.seed}`} />
                      <div class="etoiles" role="group" aria-label="Note">
                        {#each [1, 2, 3, 4, 5] as n (n)}
                          <button
                            type="button"
                            class="etoile"
                            class:allumee={(g.note ?? 0) >= n}
                            title={`${n}/5`}
                            disabled={occupe}
                            onclick={() => noter(m, g, n)}>★</button
                          >
                        {/each}
                        {#if g.peut_servir_de_source}
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
                        {/if}
                      </div>
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
    </Panneau>
  {/if}
{:else}
  <p class="discret">Lecture…</p>
{/if}

{#if confirmerRevelation && casting}
  <Dialogue titre="Révéler les modèles" fermer={() => (confirmerRevelation = false)}>
    <p>
      Seulement {casting.notees} {casting.tache.endsWith('video') ? 'clips notés' : 'images notées'} sur {casting.total}.
    </p>
    <p class="discret">
      Une fois les noms affichés, ce casting n'est plus à l'aveugle : ça ne se défait pas. Tes notes restent, et tu peux
      encore noter après.
    </p>
    {#snippet pied()}
      <Bouton genre="discret" onclick={() => (confirmerRevelation = false)}>Continuer à noter</Bouton>
      <Bouton
        genre="principal"
        onclick={() => {
          confirmerRevelation = false;
          void agir((id) => revelerCasting(id));
        }}>Révéler quand même</Bouton
      >
    {/snippet}
  </Dialogue>
{/if}

<style>
  .taches,
  .castings {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
    gap: 8px;
    margin: 12px 0;
  }

  .carte-tache {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 8px 10px;
    border: 1px solid var(--ligne);
    cursor: pointer;
  }

  .carte-tache:has(input:checked) {
    border-color: var(--cyan);
  }

  .carte-tache.bloquee strong {
    color: var(--texte-2);
  }

  .carte-tache input {
    position: absolute;
    opacity: 0;
    pointer-events: none;
  }

  h3 {
    margin: 14px 0 6px;
    font-size: 14px;
  }

  h4 {
    margin: 0 0 6px;
    font-size: 13px;
  }

  .candidats {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .candidats li {
    display: flex;
    gap: 10px;
  }

  .essais,
  .sauts,
  .tete,
  .actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    margin: 8px 0;
  }

  .actions {
    justify-content: flex-end;
  }

  .essai {
    padding: 2px 8px;
    border: 1px solid var(--ligne);
    font-size: 12px;
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
    margin: 10px 0;
    border-collapse: collapse;
    font-size: 13px;
  }

  .resume th,
  .resume td {
    padding: 4px 8px;
    border-bottom: 1px solid var(--ligne);
    text-align: left;
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
    gap: 2px;
    margin-top: 4px;
  }

  .etoile {
    padding: 0 2px;
    border: none;
    background: none;
    color: var(--texte-2);
    font: inherit;
    font-size: 16px;
    cursor: pointer;
  }

  .etoile.allumee {
    color: var(--orange);
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
