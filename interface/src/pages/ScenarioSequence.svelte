<!--
  Scénario, atelier Séquence, comme en v2 (web/decoupage.js). Le fil de l'histoire (facultatif), puis les plans, le
  cœur de l'atelier : chacun a son type, sa durée, « ce qui se passe », sa réplique (mot pour mot, au début, pendant
  l'action ou à la fin, voix off possible) et son prompt anglais, modifiable. ✨ d'un plan l'écrit seul (le LLM connaît
  le fil et les plans voisins), ✨ Tout écrire écrit tous les plans d'un coup. Le personnage et le décor viennent des
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
    compterMots,
    depuisBrouillon,
    depuisSequence,
    etatPlan,
    formatCompte,
    majAlertes,
    motsReplique,
    plansDemande,
    plansPourMontage,
    poserPlan,
    redimensionner,
    remplisAuDela,
    sansPrompt,
    traductionValide,
    type EtatDecoupage,
  } from '../lib/decoupage';
  import AnnulerEcriture from '../lib/composants/AnnulerEcriture.svelte';
  import Bouton from '../lib/composants/Bouton.svelte';
  import Dialogue from '../lib/composants/Dialogue.svelte';
  import Panneau from '../lib/composants/Panneau.svelte';
  import VignetteCreation from '../lib/composants/VignetteCreation.svelte';
  import ChoixRushes from './ChoixRushes.svelte';

  interface Props {
    signaler: (texte: string, erreur?: boolean) => void;
    sujet: string;
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

  let { signaler, sujet, choix, age, llms, llmChoisi = $bindable(), recu, recuLu, versMontage }: Props = $props();

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

  const dureeTotale = $derived(e.plans.reduce((t, p) => t + p.duree, 0));
  const occupe = $derived(ecriture !== null || envoi);

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

<Panneau titre="La séquence">
  <label>
    <span class="etiquette">Fil de l'histoire (facultatif : les plans peuvent le raconter seuls)</span>
    <textarea
      rows="3"
      maxlength="2000"
      bind:value={e.synopsis}
      placeholder="Une pilote rentre au hangar après une mission ratée ; elle dit « Plus jamais ça »."
    ></textarea>
  </label>
  <div class="ligne">
    <div class="emplacement">
      <span class="etiquette">Personnage de référence</span>
      {#if e.reference}
        <VignetteCreation id={e.reference} titre="Référence du personnage" />
        <Bouton genre="discret" petit desactive={occupe} onclick={retirerReference}>✕ Retirer</Bouton>
      {:else}
        <Bouton genre="discret" desactive={occupe} onclick={() => ouvrirChoixRush('reference')}>Choisir une image…</Bouton>
        <span class="discret">Sans image, la description des compléments est répétée dans les plans.</span>
      {/if}
    </div>
    <div class="grille">
      <label>
        <span class="etiquette">Contrainte de tous les plans</span>
        <input type="text" maxlength="300" bind:value={e.contrainte} placeholder="facultatif : pas de texte à l'écran" />
      </label>
      <label class="case">
        <input type="checkbox" bind:checked={e.avecMusique} />
        Musique d'ambiance
      </label>
      {#if e.avecMusique}
        <label>
          <span class="etiquette">Laquelle</span>
          <input type="text" maxlength="200" bind:value={e.musique} placeholder="au choix du LLM" />
        </label>
      {/if}
      {#if formatCompte(e.plans) && formats.length > 0}
        <label>
          <span class="etiquette">Format</span>
          <select bind:value={e.format}>
            {#each formats as f (f)}
              <option value={f}>{f}</option>
            {/each}
          </select>
        </label>
      {/if}
      <label>
        <span class="etiquette">Plans</span>
        <select value={e.plans.length} onchange={(ev) => changerNombre(Number(ev.currentTarget.value))} disabled={occupe}>
          {#each Array.from({ length: PLANS_MAX - PLANS_MIN + 1 }, (_, i) => i + PLANS_MIN) as n (n)}
            <option value={n}>{n}</option>
          {/each}
        </select>
      </label>
      <label>
        <span class="etiquette">LLM</span>
        <select bind:value={llmChoisi} disabled={llms.length === 0}>
          <option value="">Celui du modèle vidéo</option>
          {#each llms as l (l.fichier)}
            <option value={l.fichier}>{l.nom}{l.favori ? ' ★' : ''}</option>
          {/each}
        </select>
      </label>
    </div>
  </div>
  <div class="actions">
    <span class="discret">Film d'environ {dureeTotale} s</span>
    <Bouton genre="principal" desactive={occupe || llms.length === 0} onclick={toutEcrire}>
      {ecriture === 'tout' ? 'Le LLM écrit…' : '✨ Tout écrire'}
    </Bouton>
    {#if ecriture !== null}
      <AnnulerEcriture />
    {/if}
  </div>
</Panneau>

<ol class="plans">
  {#each e.plans as p, k (k)}
    {@const [classe, etat] = etatPlan(p)}
    {@const replique = motsReplique(p)}
    <li class={classe}>
      <div class="ligne">
        <strong>Plan {k + 1}</strong>
        <select
          value={p.enchainement}
          onchange={(ev) => changerEnchainement(k, ev.currentTarget.value as Enchainement)}
          disabled={occupe}
          aria-label="Type du plan {k + 1}"
        >
          {#each k === 0 ? TYPES_DEPART : TYPES_SUITE as [valeur, texte] (valeur)}
            <option value={valeur} disabled={valeur === 'rush' && !e.reference}>{texte}</option>
          {/each}
        </select>
        <select bind:value={p.duree} disabled={occupe} aria-label="Durée du plan {k + 1}">
          {#each durees as d (d)}
            <option value={d}>{d} s</option>
          {/each}
        </select>
        <span class="etat discret">{etat}</span>
        <Bouton petit desactive={occupe || llms.length === 0} onclick={() => void ecrire(k)}>
          {ecriture === k ? 'Le LLM écrit…' : '✨ Écrire'}
        </Bouton>
      </div>
      <div class="corps">
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
        <textarea rows="2" maxlength="500" bind:value={p.note} placeholder={NOTES_TYPES[p.enchainement]}></textarea>
      </div>
      <label class="case">
        <input type="checkbox" bind:checked={p.dialogue} />
        🗣 Dialogue
      </label>
      {#if p.dialogue}
        <input
          type="text"
          maxlength="300"
          bind:value={p.replique}
          placeholder="Ce que dit le personnage, mot pour mot : « Tu m'as encore attendu ? »"
        />
        <div class="ligne">
          <span class="discret">Quand :</span>
          {#each MOMENTS as [valeur, texte] (valeur)}
            <label class="case">
              <input type="radio" name="moment-{k}" value={valeur} checked={p.moment === valeur} onchange={() => (p.moment = valeur as MomentReplique)} />
              {texte}
            </label>
          {/each}
          <label class="case" title="On entend la voix, ses lèvres restent fermées">
            <input type="checkbox" bind:checked={p.off} />
            voix off
          </label>
          {#if replique.mots > 0}
            <small class:alerte={replique.trop}>{replique.mots} mots · {replique.permis} au plus pour {p.duree} s</small>
          {/if}
        </div>
      {/if}
      <label>
        <span class="etiquette">
          Prompt du plan, en anglais, tourné tel quel{compterMots(p.prompt) > 0 ? ` · ${compterMots(p.prompt)} mots` : ''}
        </span>
        <textarea
          rows="3"
          bind:value={p.prompt}
          placeholder="✨ l'écrit ici ; tu peux le retoucher, ou l'écrire toi-même."
        ></textarea>
      </label>
    </li>
  {/each}
</ol>

{#if e.alertes.length > 0}
  <Panneau titre="À vérifier">
    <ul class="alertes">
      {#each e.alertes as a (a)}
        <li>{a}</li>
      {/each}
    </ul>
  </Panneau>
{/if}

<Panneau titre="Au Montage" accentue>
  <div class="ligne">
    <label class="large">
      <span class="etiquette">Titre de la séquence</span>
      <input type="text" maxlength="40" bind:value={e.titre} placeholder="Le titre range ses clips dans un dossier des Bobines" />
    </label>
  </div>
  {#if erreurEnvoi}
    <p class="alerte">{erreurEnvoi}</p>
  {/if}
  <div class="actions">
    {#if e.sequenceId}
      <Bouton genre="discret" desactive={occupe} onclick={() => void envoyer(false)}>Nouvelle séquence</Bouton>
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
  .actions,
  .corps {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px;
    margin: 8px 0;
  }

  .actions {
    justify-content: flex-end;
  }

  .corps {
    align-items: flex-start;
  }

  .corps textarea {
    flex: 1;
    min-width: 240px;
  }

  label {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  label.case {
    flex-direction: row;
    align-items: center;
    gap: 6px;
    font-size: 13px;
  }

  .large {
    flex: 1;
    min-width: 240px;
  }

  .etiquette {
    font-size: 12px;
    color: var(--texte-2);
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

  textarea,
  input[type='text'] {
    width: 100%;
  }

  .plans {
    display: flex;
    flex-direction: column;
    gap: 12px;
    margin: 0 0 16px;
    padding: 0;
    list-style: none;
  }

  .plans li {
    padding: 10px;
    border: 1px solid var(--ligne);
  }

  .plans li.pret {
    border-color: var(--cyan);
  }

  .plans li.perime {
    border-color: var(--orange);
  }

  .etat {
    flex: 1;
  }

  .alertes {
    margin: 0;
    padding-left: 18px;
  }

  .alerte {
    color: var(--orange);
  }
</style>
