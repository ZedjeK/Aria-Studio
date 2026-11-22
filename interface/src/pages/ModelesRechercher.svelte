<!--
  Modèles → Rechercher, comme en v2 (« à la LM Studio ») : modèles image, modèles vidéo, LoRA et LLM sur Civitai et
  Hugging Face, filtrés sur les familles d'Aria, et un lien collé. Le clic « Télécharger » vaut confirmation : la fiche
  est relue à la source puis mise en file. Les modèles adultes ne sont demandés à Civitai que si la case est cochée,
  et leurs vignettes restent floues tant que l'autre case ne l'est pas (deux choix retenus sur ce PC seulement).
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import {
    chercherModeles,
    famillesDecouvrir,
    fichiersDepot,
    messageErreur,
    preparerLien,
    telechargerLien,
    type FamilleDecouvrir,
    type FichierTrouve,
    type FichiersDepot,
    type ModeleTrouve,
    type PropositionLien,
    type SourceRecherche,
    type TypeRecherche,
  } from '../lib/api';
  import { octetsLisibles } from '../lib/jauge';
  import { TYPES, compte, conseil, dossierDeFamille, libelleSource, libelleVram, sources, tris } from '../lib/rechercher';
  import ASavoir from '../lib/composants/ASavoir.svelte';
  import Bouton from '../lib/composants/Bouton.svelte';
  import Panneau from '../lib/composants/Panneau.svelte';
  import VignetteModele from '../lib/composants/VignetteModele.svelte';

  interface Props {
    signaler: (texte: string, erreur?: boolean) => void;
    /** Un fichier vient d'entrer dans la file. */
    ajoute: () => void;
  }

  let { signaler, ajoute }: Props = $props();

  function lireChoix(cle: string): boolean {
    try {
      return localStorage.getItem(cle) === 'oui';
    } catch {
      return false; // stockage indisponible : modèles adultes exclus et vignettes floues
    }
  }

  function retenirChoix(cle: string, oui: boolean) {
    try {
      localStorage.setItem(cle, oui ? 'oui' : 'non');
    } catch {
      // stockage indisponible : le choix vaut pour cette fois
    }
  }

  let type = $state<TypeRecherche>('image');
  let source = $state<SourceRecherche>('civitai');
  let texte = $state('');
  let famille = $state('');
  let tri = $state('telechargements');
  let adulte = $state(lireChoix('aria-adulte'));
  let voirNsfw = $state(lireChoix('aria-nsfw'));
  let familles = $state<FamilleDecouvrir[]>([]);
  /** Toutes les familles, pour ranger un fichier dont le dossier n'est pas deviné. */
  let toutes = $state<FamilleDecouvrir[]>([]);
  let resultats = $state<ModeleTrouve[]>([]);
  let curseur = $state<string | null>(null);
  let cherche = $state(false);
  let occupe = $state(false);
  /** Les fichiers d'un dépôt Hugging Face ouvert, par id. */
  let depots = $state<Record<string, FichiersDepot>>({});
  /** Le dossier choisi pour un fichier sans dossier deviné, par lien. */
  let choix = $state<Record<string, string>>({});
  let lien = $state('');
  let proposition = $state<PropositionLien | null>(null);

  const aide = $derived(TYPES.find((t) => t.id === type)?.aide ?? '');
  const dossiersFamilles = $derived(
    toutes.flatMap((f) => [dossierDeFamille(f, 'image'), dossierDeFamille(f, 'lora')]).sort(),
  );

  async function changerType(t: TypeRecherche) {
    type = t;
    if (!sources(t).includes(source)) source = 'huggingface';
    resultats = [];
    curseur = null;
    try {
      familles = await famillesDecouvrir(t);
      if (!familles.some((f) => f.famille === famille)) famille = '';
    } catch (e) {
      signaler(messageErreur(e), true);
    }
  }

  function changerSource(s: SourceRecherche) {
    source = s;
    if (!tris(s).some((t) => t.id === tri)) tri = 'telechargements';
    resultats = [];
    curseur = null;
  }

  async function chercher(suite = false) {
    if (occupe) return;
    occupe = true;
    try {
      const r = await chercherModeles({
        source,
        type,
        texte,
        famille: type === 'llm' || famille === '' ? null : famille,
        tri,
        curseur: suite ? curseur : null,
        adulte: source === 'civitai' && adulte,
      });
      resultats = suite ? [...resultats, ...r.resultats] : r.resultats;
      curseur = r.curseur;
      cherche = true;
    } catch (e) {
      signaler(messageErreur(e), true);
    } finally {
      occupe = false;
    }
  }

  async function ouvrirDepot(m: ModeleTrouve) {
    if (depots[m.id]) {
      delete depots[m.id];
      return;
    }
    try {
      depots[m.id] = await fichiersDepot(m.id, type, m.famille);
    } catch (e) {
      signaler(messageErreur(e), true);
    }
  }

  /** Met en file le fichier d'un lien (le clic vaut confirmation). */
  async function telecharger(adresse: string, dossier: string | null | undefined, nom: string) {
    if (!dossier) {
      signaler(`Choisis d'abord le dossier de ${nom} (la famille du modèle).`, true);
      return;
    }
    if (occupe) return;
    occupe = true;
    try {
      await telechargerLien(adresse, dossier);
      signaler(`${nom} est en file : son avancement s'affiche en haut de la page.`);
      ajoute();
    } catch (e) {
      signaler(messageErreur(e), true);
    } finally {
      occupe = false;
    }
  }

  async function lireLien() {
    if (occupe || lien.trim() === '') return;
    occupe = true;
    proposition = null;
    try {
      proposition = await preparerLien(lien.trim());
    } catch (e) {
      signaler(messageErreur(e), true);
    } finally {
      occupe = false;
    }
  }

  function dossierDe(adresse: string, devine: string | null): string | null {
    return devine ?? (choix[adresse] || null);
  }

  function principal(fichiers: FichierTrouve[]): FichierTrouve | undefined {
    return fichiers.find((f) => f.principal) ?? fichiers[0];
  }

  onMount(() => {
    void changerType('image');
    void famillesDecouvrir('lora')
      .then((f) => (toutes = f))
      .catch((e: unknown) => signaler(messageErreur(e), true));
  });
</script>

<ASavoir>
  Seuls les modèles qui vont dans une famille d'Aria sont montrés, et seuls les fichiers .safetensors et .gguf sont
  proposés : les autres formats peuvent exécuter du code au chargement. Un modèle qui demande une clé (Civitai, ou
  dépôt Hugging Face à conditions) se télécharge avec celle de Réglages → Emplacements et clés.
</ASavoir>

<Panneau titre="Depuis un lien">
  <div class="ligne">
    <input
      class="large"
      type="url"
      placeholder="https://huggingface.co/…/blob/main/… ou https://civitai.com/models/…"
      aria-label="Lien d'un fichier sur Hugging Face ou Civitai"
      bind:value={lien}
      onkeydown={(e) => {
        if (e.key === 'Enter') void lireLien();
      }}
    />
    <Bouton petit desactive={occupe || lien.trim() === ''} onclick={() => void lireLien()}>Lire le lien</Bouton>
  </div>
  {#if proposition}
    {@const f = proposition.fiche}
    <div class="fiche">
      <strong>{f.nom}</strong>
      <span class="discret">
        {octetsLisibles(f.taille)} · {libelleSource(f.source === 'civitai' ? 'civitai' : 'huggingface')}{f.licence
          ? ` · licence ${f.licence}`
          : ' · licence : voir la page'}{f.base ? ` · base ${f.base}` : ''}
      </span>
      <span class="discret code">{f.page}</span>
      {#if f.mots_declencheurs.length > 0}
        <span class="discret">Mots déclencheurs : {f.mots_declencheurs.join(', ')}</span>
      {/if}
      {#if f.force_auteur !== null}
        <span class="discret">Force utilisée par l'auteur : {f.force_auteur} ({f.images_force} image{f.images_force > 1 ? 's' : ''})</span>
      {/if}
      {#if conseil('Steps', f.steps_suggeres)}
        <span class="discret">{conseil('Steps', f.steps_suggeres)}</span>
      {/if}
      {#if conseil('CFG', f.cfg_suggere)}
        <span class="discret">{conseil('CFG', f.cfg_suggere)}</span>
      {/if}
      {#if proposition.jetonRequis}
        <span class="alerte">
          Cette source demande une clé : ajoute-la dans Réglages → Emplacements et clés (et accepte les conditions sur la
          page du modèle).
        </span>
      {/if}
      <div class="ligne">
        {#if proposition.dossier}
          <span class="discret">Dossier : <span class="code">{proposition.dossier}</span></span>
        {:else}
          <select bind:value={choix[lien.trim()]} aria-label="Dossier de destination">
            <option value="">Choisis le dossier…</option>
            {#each dossiersFamilles as d (d)}
              <option value={d}>{d}</option>
            {/each}
          </select>
        {/if}
        <Bouton
          genre="principal"
          petit
          desactive={occupe}
          onclick={() => void telecharger(lien.trim(), dossierDe(lien.trim(), proposition?.dossier ?? null), f.nom)}
        >
          Télécharger
        </Bouton>
      </div>
    </div>
  {/if}
</Panneau>

<Panneau titre="Rechercher">
  <div class="pastilles" role="group" aria-label="Type de modèle">
    {#each TYPES as t (t.id)}
      <button type="button" class="pastille" aria-pressed={type === t.id} onclick={() => void changerType(t.id)}>
        {t.titre}
      </button>
    {/each}
  </div>
  <p class="discret">{aide}</p>
  <div class="ligne">
    <select
      value={source}
      onchange={(e) => changerSource(e.currentTarget.value === 'civitai' ? 'civitai' : 'huggingface')}
      aria-label="Source"
    >
      {#each sources(type) as s (s)}
        <option value={s}>{libelleSource(s)}</option>
      {/each}
    </select>
    {#if type !== 'llm'}
      <select bind:value={famille} aria-label="Famille">
        <option value="">Toutes les familles</option>
        {#each familles as f (f.famille)}
          <option value={f.famille}>{f.famille}</option>
        {/each}
      </select>
    {/if}
    <select bind:value={tri} aria-label="Tri">
      {#each tris(source) as t (t.id)}
        <option value={t.id}>{t.titre}</option>
      {/each}
    </select>
    <input
      class="large"
      type="search"
      placeholder="Nom, auteur…"
      aria-label="Texte cherché"
      bind:value={texte}
      onkeydown={(e) => {
        if (e.key === 'Enter') void chercher();
      }}
    />
    <Bouton petit desactive={occupe} onclick={() => void chercher()}>Chercher</Bouton>
  </div>
  {#if source === 'civitai'}
    <div class="ligne">
      <label class="case">
        <input type="checkbox" bind:checked={adulte} onchange={() => retenirChoix('aria-adulte', adulte)} />
        🔞 Inclure les modèles adultes
      </label>
      <label class="case">
        <input type="checkbox" bind:checked={voirNsfw} onchange={() => retenirChoix('aria-nsfw', voirNsfw)} />
        Montrer les vignettes adultes sans flou
      </label>
    </div>
  {/if}
</Panneau>

{#if cherche && resultats.length === 0}
  <p class="discret">Aucun résultat pour une famille d'Aria.</p>
{/if}

<ul class="resultats">
  {#each resultats as m (m.source + m.id)}
    <li>
      {#if m.apercu}
        <VignetteModele url={m.apercu.url} floue={m.apercu.nsfw && !voirNsfw} />
      {/if}
      <div class="texte">
        <strong>{m.nom}</strong>
        <span class="discret">
          {m.auteur ? `${m.auteur} · ` : ''}{m.famille ? `${m.famille} · ` : ''}⬇ {compte(m.telechargements)} · ♥ {compte(
            m.appreciations,
          )}{m.installe ? ' · déjà installé' : ''}{m.protege ? ' · accès à conditions' : ''}
        </span>
        <span class="discret code">{m.page}</span>
        {#if m.source === 'civitai'}
          {#each m.versions as v (v.id)}
            {@const f = principal(v.fichiers)}
            <div class="version">
              <span>
                {v.nom ?? 'Version'}{v.base ? ` · ${v.base}` : ''}{v.motsDeclencheurs.length > 0
                  ? ` · « ${v.motsDeclencheurs.join(', ')} »`
                  : ''}
              </span>
              {#if f}
                <span class="discret">{f.nom} · {octetsLisibles(f.taille)} · {v.dossier}</span>
                {#if f.installe}
                  <span class="bon">Installé</span>
                {:else}
                  <Bouton petit desactive={occupe} onclick={() => void telecharger(v.lien, v.dossier, f.nom)}>
                    Télécharger
                  </Bouton>
                {/if}
              {/if}
            </div>
          {/each}
        {:else}
          <div class="actions">
            <Bouton genre="discret" petit onclick={() => void ouvrirDepot(m)}>
              {depots[m.id] ? 'Masquer les fichiers' : 'Voir les fichiers'}
            </Bouton>
          </div>
          {#if depots[m.id]}
            {@const d = depots[m.id]}
            {#if d}
              {#if d.vramLibreGo !== null}
                <span class="discret">VRAM disponible pour le LLM : ~{d.vramLibreGo} / {d.vramTotaleGo} Go</span>
              {/if}
              {#each d.fichiers as f (f.nom)}
                <div class="version">
                  <span>{f.nom}</span>
                  <span class="discret">
                    {octetsLisibles(f.taille)}{f.vram ? ` · ${libelleVram(f.vram)}` : ''}{f.dossier
                      ? ` · ${f.dossier}`
                      : ''}
                  </span>
                  {#if f.installe}
                    <span class="bon">Installé</span>
                  {:else if f.lien}
                    {@const adresse = f.lien}
                    {#if !f.dossier}
                      <select bind:value={choix[adresse]} aria-label={`Dossier de ${f.nom}`}>
                        <option value="">Choisis le dossier…</option>
                        {#each dossiersFamilles as dossier (dossier)}
                          <option value={dossier}>{dossier}</option>
                        {/each}
                      </select>
                    {/if}
                    <Bouton
                      petit
                      desactive={occupe}
                      onclick={() => void telecharger(adresse, dossierDe(adresse, f.dossier), f.nom)}
                    >
                      Télécharger
                    </Bouton>
                  {/if}
                </div>
              {:else}
                <span class="discret">Aucun fichier proposé dans ce dépôt.</span>
              {/each}
            {/if}
          {/if}
        {/if}
      </div>
    </li>
  {/each}
</ul>

{#if curseur}
  <div class="actions">
    <Bouton genre="discret" desactive={occupe} onclick={() => void chercher(true)}>Plus de résultats</Bouton>
  </div>
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

  .large {
    flex: 1;
    min-width: 220px;
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

  .case {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 13px;
  }

  .fiche,
  .texte {
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
  }

  .texte {
    flex: 1;
  }

  .resultats {
    display: flex;
    flex-direction: column;
    gap: 14px;
    margin: 12px 0;
    padding: 0;
    list-style: none;
  }

  .resultats li {
    display: flex;
    gap: 12px;
    padding: 12px;
    border: 1px solid var(--ligne);
    background: var(--panneau);
  }

  .version {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px;
    padding-top: 6px;
    border-top: 1px solid var(--ligne);
  }

  .code {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .bon {
    color: var(--cyan);
  }

  .alerte {
    color: var(--orange);
  }
</style>
