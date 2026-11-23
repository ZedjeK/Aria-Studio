<!--
  L'assistant de premier lancement, comme en v2 (web/assistant.js) : ta machine, les moteurs, les packs à cocher,
  leurs licences, puis le téléchargement par la file habituelle. Il s'ouvre tout seul tant qu'il manque de quoi
  générer et qu'il n'a pas été terminé ; « Plus tard » le ferme, Réglages le relance. L'écran de fin récapitule les
  favoris réglés et propose une image d'essai (peu de steps).
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import {
    accepterLicencesAssistant,
    essaiAssistant,
    etatAssistant,
    etatEmplacements,
    installerMoteur,
    lireCommandes,
    lireMoteurs,
    lireTelechargements,
    messageErreur,
    planAssistant,
    telechargerAssistant,
    terminerAssistant,
    type Commande,
    type EtatLieux,
    type EtatMoteurs,
    type FinAssistant,
    type Pack,
    type PlanAssistant,
    type Telechargement,
  } from '../lib/api';
  import {
    ETAPES,
    bilanPlan,
    choixInitial,
    etatTelechargement,
    go,
    packsAFinir,
    suiviTelechargements,
  } from '../lib/assistant';
  import { fractionTache, libelleStatut, manquants, texteCarte } from '../lib/moteurs';
  import { estActive } from '../lib/plateau';
  import Bouton from '../lib/composants/Bouton.svelte';
  import Dialogue from '../lib/composants/Dialogue.svelte';
  import VignetteCreation from '../lib/composants/VignetteCreation.svelte';

  interface Props {
    fermer: (termine: boolean) => void;
  }

  let { fermer }: Props = $props();

  let etape = $state(0);
  let packs = $state<Pack[]>([]);
  let choisis = $state<string[]>([]);
  let moteurs = $state<EtatMoteurs | null>(null);
  let lieux = $state<EtatLieux | null>(null);
  let plan = $state<PlanAssistant | null>(null);
  let acceptees = $state<Record<string, boolean>>({});
  /** Les téléchargements lancés par l'assistant, et les fichiers que la file avait déjà. */
  let taches = $state<string[]>([]);
  let enAttente = $state<string[]>([]);
  let suivis = $state<Telechargement[]>([]);
  let fin = $state<FinAssistant | null>(null);
  let essai = $state<Commande | null>(null);
  let occupe = $state(false);
  let message = $state('');

  const modeles = $derived(lieux?.dossiers.find((d) => d.cle === 'modeles') ?? null);
  const libreModeles = $derived(lieux?.libres['modeles'] ?? null);
  const absents = $derived(moteurs ? manquants(moteurs.moteurs) : []);
  const installation = $derived(moteurs?.tache && !moteurs.tache.fini ? moteurs.tache : null);
  const bilan = $derived(bilanPlan(packs, choisis, plan));
  const aAccepter = $derived(packs.filter((p) => choisis.includes(p.id)));
  const suivi = $derived(suiviTelechargements(suivis));

  const suivant = $derived.by(() => {
    if (occupe) return false;
    switch (etape) {
      case 0:
        return Boolean(moteurs?.carte.ok) && !lieux?.redemarrage;
      case 1:
        return absents.length === 0 && !installation;
      case 2:
        return bilan.suivant;
      case 3:
        return aAccepter.every((p) => acceptees[p.id]);
      case 4:
        return suivi.finis;
      default:
        return true;
    }
  });

  const texteSuivant = $derived(
    etape === 3 ? "J'accepte et je télécharge →" : etape === 4 ? 'Terminer' : etape === 5 ? "C'est parti !" : 'Suivant →',
  );

  async function relirePacks() {
    packs = (await etatAssistant()).packs;
  }

  async function relirePlan() {
    plan = choisis.length > 0 ? await planAssistant(choisis) : null;
  }

  async function relireMoteurs() {
    moteurs = await lireMoteurs();
  }

  async function relireTelechargements() {
    suivis = (await lireTelechargements()).filter((t) => taches.includes(t.id) || enAttente.includes(t.nom));
  }

  async function entrer(n: number) {
    etape = n;
    message = '';
    try {
      if (n === 0) [moteurs, lieux] = await Promise.all([lireMoteurs(), etatEmplacements()]);
      if (n === 1) await relireMoteurs();
      if (n === 2) {
        await relirePacks(); // les moteurs ont pu s'installer depuis l'ouverture
        await relirePlan();
      }
      if (n === 3) {
        acceptees = Object.fromEntries(aAccepter.map((p) => [p.id, p.licenceAcceptee !== null]));
      }
      if (n === 4) await relireTelechargements();
    } catch (e) {
      message = messageErreur(e);
    }
  }

  async function avancer() {
    occupe = true;
    message = '';
    try {
      if (etape === 3 && choisis.length > 0) {
        await accepterLicencesAssistant(choisis);
        const envoi = await telechargerAssistant(choisis);
        taches = envoi.ajoutes;
        enAttente = envoi.dejaEnFile;
      } else if (etape === 4) {
        fin = await terminerAssistant(packsAFinir(packs, choisis));
      } else if (etape === 5) {
        fermer(true);
        return;
      }
      occupe = false;
      await entrer(etape + 1);
    } catch (e) {
      message = messageErreur(e);
    } finally {
      occupe = false;
    }
  }

  function basculer(id: string, coche: boolean) {
    choisis = coche ? [...choisis, id] : choisis.filter((c) => c !== id);
    void relirePlan().catch((e: unknown) => (message = messageErreur(e)));
  }

  async function installer(id: string) {
    try {
      await installerMoteur(id);
      await relireMoteurs();
    } catch (e) {
      message = messageErreur(e);
    }
  }

  async function lancerEssai() {
    message = '';
    try {
      essai = await essaiAssistant();
    } catch (e) {
      message = messageErreur(e);
    }
  }

  async function relireEssai() {
    const id = essai?.id;
    if (!id) return;
    essai = (await lireCommandes()).find((c) => c.id === id) ?? essai;
  }

  // Le suivi en direct : l'installation d'un moteur, les téléchargements, l'image d'essai.
  $effect(() => {
    const minuterie = setInterval(() => {
      const suivre = async () => {
        if (etape === 1 && installation) await relireMoteurs();
        if (etape === 4 && !suivi.finis) await relireTelechargements();
        if (etape === 5 && essai && estActive(essai)) await relireEssai();
      };
      suivre().catch((e: unknown) => (message = messageErreur(e)));
    }, 1000);
    return () => clearInterval(minuterie);
  });

  onMount(() => {
    void (async () => {
      try {
        await relirePacks();
        choisis = choixInitial(packs);
      } catch (e) {
        message = messageErreur(e);
      }
      await entrer(0);
    })();
  });
</script>

<Dialogue titre="Bienvenue dans Aria Studio" fermer={() => fermer(false)}>
  <ol class="etapes">
    {#each ETAPES as titre, i (titre)}
      <li class:actif={i === etape} class:fait={i < etape}>{titre}</li>
    {/each}
  </ol>

  {#if etape === 0}
    <dl>
      <dt>Carte graphique</dt>
      <dd>{moteurs ? texteCarte(moteurs.carte) : 'Lecture…'}</dd>
      <dt>Testé sur</dt>
      <dd>RTX 4080 SUPER 16 Go, 64 Go de RAM (pas encore d'autre carte vérifiée)</dd>
      <dt>Dossier des modèles</dt>
      <dd class="code">{modeles?.chemin ?? '…'}</dd>
      <dt>Place libre</dt>
      <dd>{libreModeles === null ? 'inconnue' : go(libreModeles)}</dd>
    </dl>
    {#if lieux?.redemarrage}
      <p class="ko">
        Le nouveau dossier des modèles ({lieux.modelesProchain}) vaudra au prochain démarrage : relance Aria, puis rouvre
        l'assistant (Réglages) avant de télécharger.
      </p>
    {:else}
      <p class="discret">Pour mettre les modèles ailleurs : Réglages → Emplacements, avant de télécharger.</p>
    {/if}
  {:else if etape === 1}
    <ul class="liste">
      {#each moteurs?.moteurs ?? [] as m (m.id)}
        <li>
          <span><strong>{m.nom}</strong> <span class="discret">{m.role}</span></span>
          <span class="discret">{libelleStatut(m.statut)}{m.facultatif ? ' · facultatif' : ''}</span>
          {#if m.statut === 'absent'}
            <Bouton
              genre={absents[0]?.id === m.id ? 'principal' : 'secondaire'}
              petit
              desactive={installation !== null}
              onclick={() => void installer(m.id)}>Installer</Bouton
            >
          {/if}
        </li>
      {/each}
    </ul>
    {#if installation}
      <p class="discret">{installation.nom} : {installation.etape}</p>
      <progress max="1" value={fractionTache(installation) ?? 0}></progress>
    {:else if absents.length > 0}
      <p class="ko">À installer : {absents.map((m) => m.nom).join(', ')}</p>
    {/if}
  {:else if etape === 2}
    <ul class="liste">
      {#each packs as p (p.id)}
        <li>
          <label class="pack">
            <input
              type="checkbox"
              checked={p.installe || choisis.includes(p.id)}
              disabled={p.installe}
              onchange={(e) => basculer(p.id, e.currentTarget.checked)}
            />
            <span>
              <strong>{p.nom}</strong>
              {#if p.conseille}<span class="badge">conseillé</span>{/if}
              {#if p.facultatif}<span class="badge">facultatif</span>{/if}
              <span class="discret bloc">{p.usage}</span>
              <span class="discret bloc">
                Licence : {p.licences.map((l) => l.nom).join(', ')} · testé sur {p.testee ?? '—'}
              </span>
              {#if p.moteursAbsents.length > 0}
                <span class="ko bloc">Demande aussi : {p.moteursAbsents.join(', ')} (étape Moteurs)</span>
              {/if}
            </span>
            <span class="discret">
              {p.installe ? 'installé ✓' : p.aTelecharger < p.taille ? `${go(p.aTelecharger)} sur ${go(p.taille)}` : go(p.taille)}
            </span>
          </label>
        </li>
      {/each}
    </ul>
    <p class:ko={bilan.ko}>{bilan.texte}</p>
    {#if bilan.alerte}<p class="ko">{bilan.alerte}</p>{/if}
  {:else if etape === 3}
    {#each aAccepter as p (p.id)}
      <div class="licence">
        <strong>{p.nom}</strong>
        {#each p.licences as l (l.nom)}
          <div>
            <span>{l.nom}</span>
            <span class="discret code bloc">{l.url}</span>
            <span class="discret bloc">{l.resume}</span>
            {#if l.territoiresExclus.length > 0}
              <span class="ko bloc">Utilisation non autorisée : {l.territoiresExclus.join(', ')}</span>
            {/if}
            {#if l.composants.length > 0}
              <ul class="discret">
                {#each l.composants as c (c)}<li>{c}</li>{/each}
              </ul>
            {/if}
          </div>
        {/each}
        <label class="case">
          <input
            type="checkbox"
            checked={acceptees[p.id] ?? false}
            onchange={(e) => (acceptees = { ...acceptees, [p.id]: e.currentTarget.checked })}
          />
          J'ai lu {p.licences.length > 1 ? 'ces licences' : 'cette licence'} et je l'accepte
        </label>
      </div>
    {:else}
      <p class="discret">Rien à télécharger : aucune licence à accepter.</p>
    {/each}
    {#if plan && plan.fichiers.length > 0}
      <p class="discret">
        {aAccepter.map((p) => p.nom).join(', ')} : {plan.fichiers.length} fichier{plan.fichiers.length > 1 ? 's' : ''},
        {go(plan.aTelecharger)} (fichiers communs comptés une fois), dès que tu acceptes.
      </p>
    {/if}
  {:else if etape === 4}
    <p class="discret">
      {taches.length + enAttente.length > 0
        ? 'Chaque fichier est vérifié par son empreinte SHA-256 à l’arrivée.'
        : 'Rien à télécharger : tout est déjà là.'}
    </p>
    <ul class="liste">
      {#each suivis as t (t.id)}
        <li>
          <span>{t.nom}</span>
          <span class="discret">{etatTelechargement(t)}</span>
          {#if t.erreur}<span class="ko bloc">{t.erreur}</span>{/if}
        </li>
      {/each}
    </ul>
    {#if suivi.texte}<p class="discret">{suivi.texte}</p>{/if}
  {:else if fin}
    <dl>
      {#each fin.regles as r (r.usage)}
        <dt>{r.usage}</dt>
        <dd>{r.nom} ✓</dd>
      {:else}
        <dt>Rien de réglé</dt>
        <dd>aucun pack n'est complet pour l'instant</dd>
      {/each}
    </dl>
    {#if fin.manquants.length > 0}
      <p class="ko">
        Pas encore là (téléchargement pas fini ou en erreur) : {fin.manquants.map((m) => `${m.usage} avec ${m.nom}`).join(', ')}.
        Relance l'assistant une fois les fichiers arrivés.
      </p>
    {/if}
    {#if fin.essai}
      <div class="essai">
        <Bouton desactive={essai !== null && estActive(essai)} onclick={() => void lancerEssai()}>
          Faire une image d'essai
        </Bouton>
        {#if essai}
          {#if estActive(essai)}
            <span class="discret">{essai.etat === 'attente' ? "En file d'attente…" : (essai.etape || 'Finalisation')}</span>
            <progress max="1" value={essai.progression}></progress>
          {:else if essai.etat === 'erreur'}
            <span class="ko">{essai.erreur}</span>
          {:else if essai.images[0]}
            <span class="discret">Tout tourne ✓ L'image est aussi dans tes rushes.</span>
            <VignetteCreation id={essai.images[0]} titre="Image d'essai" />
          {/if}
        {/if}
      </div>
    {/if}
  {/if}

  {#snippet pied()}
    {#if message}<span class="ko etat">{message}</span>{/if}
    {#if etape < 5}
      <Bouton genre="discret" onclick={() => fermer(false)}>Plus tard</Bouton>
    {/if}
    <Bouton desactive={occupe || etape === 0 || etape >= 4} onclick={() => void entrer(etape - 1)}>← Précédent</Bouton>
    <Bouton
      genre={etape === 1 && absents.length > 0 ? 'secondaire' : 'principal'}
      desactive={!suivant}
      onclick={() => void avancer()}>{texteSuivant}</Bouton
    >
  {/snippet}
</Dialogue>

<style>
  .etapes {
    display: flex;
    flex-wrap: wrap;
    gap: 6px 14px;
    margin: 0;
    padding: 0;
    list-style: none;
    color: var(--texte-2);
    font-size: 13px;
  }

  .etapes .actif {
    color: var(--cyan);
  }

  .etapes .fait {
    color: var(--texte);
  }

  dl {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 6px 14px;
    margin: 0;
  }

  dt {
    color: var(--texte-2);
  }

  dd {
    margin: 0;
  }

  .liste {
    display: flex;
    flex-direction: column;
    gap: 10px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .liste > li {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 6px 12px;
  }

  .pack {
    display: flex;
    flex: 1;
    align-items: flex-start;
    gap: 10px;
  }

  .pack > span:nth-child(2) {
    flex: 1;
  }

  .bloc {
    display: block;
  }

  .badge {
    margin-left: 6px;
    padding: 0 6px;
    border: 1px solid var(--ligne);
    font-size: 12px;
  }

  .licence {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 10px;
    border: 1px solid var(--ligne);
  }

  .case {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .essai {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 8px;
  }

  progress {
    width: 100%;
    height: 4px;
    accent-color: var(--cyan);
  }

  .etat {
    flex: 1;
  }

  .ko {
    color: var(--orange);
  }
</style>
