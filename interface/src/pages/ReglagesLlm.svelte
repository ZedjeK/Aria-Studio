<!--
  Réglages → LLM, comme en v2 : les réglages de chaque GGUF installé (températures, échantillonnage par mode, longueur,
  contexte, réflexion, moteur, vision) et ses consignes réécrites. Seuls les écarts à l'origine sont enregistrés ; la
  clause de protection des mineurs, elle, termine toujours la consigne et ne se modifie pas. « ⚡ Adapter à ma machine »
  propose, pour tous les LLM installés, les réglages officiels et ceux que la carte graphique demande : avant → après,
  rien sans confirmation.
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import {
    adapterAMaMachine,
    etatReglages,
    messageErreur,
    modifierReglagesLlm,
    type Adaptation,
    type EtatReglages,
    type Proposition,
  } from '../lib/api';
  import { parCible, texteMateriel } from '../lib/optimiseur';
  import { changements, nomConsigne, type Saisies } from '../lib/reglages';
  import ApercuPropositions from '../lib/composants/ApercuPropositions.svelte';
  import ASavoir from '../lib/composants/ASavoir.svelte';
  import Bouton from '../lib/composants/Bouton.svelte';
  import ChampReglage from '../lib/composants/ChampReglage.svelte';
  import Panneau from '../lib/composants/Panneau.svelte';

  interface Props {
    signaler: (texte: string, erreur?: boolean) => void;
  }

  let { signaler }: Props = $props();

  let etat = $state<EtatReglages | null>(null);
  let fichier = $state('');
  let saisies = $state<Saisies>({});
  let consignes = $state<Record<string, string | null>>({});

  const llm = $derived(etat?.llm.find((l) => l.fichier === fichier));
  const aEnregistrer = $derived(
    llm !== undefined && (Object.keys(changements(llm.params, saisies)).length > 0 || Object.keys(consignes).length > 0),
  );

  async function relire() {
    try {
      etat = await etatReglages();
      if (!etat.llm.some((l) => l.fichier === fichier)) {
        fichier = (etat.llm.find((l) => l.fichier === etat?.defautLlm) ?? etat.llm[0])?.fichier ?? '';
      }
    } catch (e) {
      signaler(messageErreur(e), true);
    }
  }

  function choisir(nouveau: string) {
    fichier = nouveau;
    saisies = {};
    consignes = {};
  }

  async function enregistrer() {
    if (!llm || !etat) return;
    try {
      const mis = await modifierReglagesLlm(llm.fichier, changements(llm.params, saisies), consignes);
      etat.llm = etat.llm.map((l) => (l.fichier === mis.fichier ? mis : l));
      saisies = {};
      consignes = {};
      signaler(`Réglages de ${mis.nom} enregistrés.`);
    } catch (e) {
      signaler(messageErreur(e), true);
    }
  }

  /** ⚡ Adapter à ma machine : les propositions, le temps de l'aperçu. */
  let adaptation = $state<Adaptation | null>(null);
  let analyse = $state(false);

  async function adapter() {
    analyse = true;
    try {
      adaptation = await adapterAMaMachine();
    } catch (e) {
      signaler(messageErreur(e), true);
    } finally {
      analyse = false;
    }
  }

  async function appliquerAdaptation(choisies: Proposition[]) {
    try {
      for (const g of parCible(choisies)) await modifierReglagesLlm(g.cible, g.changements, {});
      adaptation = null;
      saisies = {};
      await relire();
      signaler(`${choisies.length} réglage${choisies.length > 1 ? 's' : ''} appliqué${choisies.length > 1 ? 's' : ''}.`);
    } catch (e) {
      signaler(messageErreur(e), true);
    }
  }

  onMount(() => void relire());
</script>

<ASavoir>
  Chaque GGUF a ses propres réglages ; ceux que tu ne touches pas gardent leur valeur d'origine. Changer le contexte,
  le moteur ou la vision redémarre llama-server. La clause de protection des mineurs termine toujours la consigne.
</ASavoir>

{#if etat && etat.llm.length === 0}
  <p class="discret">Aucun LLM dans modeles\llm. Les LLM téléchargés par la version 2 sont repris.</p>
{:else if etat && llm}
  {#if etat.noteDefaut}<p class="discret">{etat.noteDefaut}</p>{/if}
  <div class="ligne">
    <label>
      LLM
      <select value={fichier} onchange={(e) => choisir(e.currentTarget.value)}>
        {#each etat.llm as l (l.fichier)}
          <option value={l.fichier}>{l.nom}{l.fichier === etat.defautLlm ? ' (par défaut)' : ''}</option>
        {/each}
      </select>
    </label>
    <Bouton desactive={analyse} onclick={() => void adapter()}>
      {analyse ? 'Analyse…' : '⚡ Adapter à ma machine'}
    </Bouton>
    <Bouton genre="principal" desactive={!aEnregistrer} onclick={() => void enregistrer()}>Enregistrer</Bouton>
  </div>

  {#if adaptation}
    <ApercuPropositions
      titre="⚡ Adapter à ma machine"
      entete={texteMateriel(adaptation.materiel)}
      lignes={adaptation.propositions.map((p) => ({ proposition: p, avant: p.actuel }))}
      alertes={adaptation.alertes}
      notes={adaptation.sansRecommandation.length > 0
        ? [
            `Sans recommandation officielle rangée (seul le matériel compte) : ${adaptation.sansRecommandation.join(', ')}.`,
          ]
        : []}
      vide="Rien à proposer : tes réglages suivent déjà les recommandations."
      appliquer={appliquerAdaptation}
      fermer={() => (adaptation = null)}
    />
  {/if}

  <Panneau titre="Réglages" accentue>
    <div class="grille">
      {#each llm.params as p (p.cle)}
        <ChampReglage param={p} bind:saisies />
      {/each}
    </div>
  </Panneau>

  {#each llm.consignes as c (c.cle)}
    <Panneau titre={nomConsigne(c.cle)}>
      <textarea
        rows="10"
        value={c.cle in consignes ? (consignes[c.cle] ?? c.defaut) : c.valeur}
        oninput={(e) => (consignes = { ...consignes, [c.cle]: e.currentTarget.value })}
      ></textarea>
      <div class="ligne">
        {#if c.modifie || (consignes[c.cle] != null && consignes[c.cle] !== c.defaut)}
          <span class="discret">Réécrite.</span>
          <Bouton genre="discret" petit onclick={() => (consignes = { ...consignes, [c.cle]: null })}>Revenir à l'origine</Bouton>
        {:else}
          <span class="discret">Consigne d'origine.</span>
        {/if}
      </div>
    </Panneau>
  {/each}

  <Panneau titre="Clause verrouillée">
    <p class="code clause">{etat.clauseMineurs}</p>
  </Panneau>
{:else}
  <p class="discret">Lecture…</p>
{/if}

<style>
  .ligne {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px 16px;
    margin: 8px 0;
  }

  .ligne label {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .grille {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(240px, 1fr));
    gap: 14px 20px;
  }

  textarea {
    width: 100%;
    font-family: var(--police-code);
    font-size: 12px;
    color: var(--texte);
    background: var(--fond);
    border: 1px solid var(--ligne);
    padding: 8px;
    resize: vertical;
  }

  .clause {
    font-size: 12px;
    color: var(--texte-2);
  }
</style>
