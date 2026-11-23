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
  import { changements, groupesLlm, nomConsigne, texteChangements, type Saisies } from '../lib/reglages';
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
  /** Les réglages et les consignes qui diffèrent de ce qui est enregistré. */
  const nombreChangements = $derived(
    llm === undefined ? 0 : Object.keys(changements(llm.params, saisies)).length + Object.keys(consignes).length,
  );
  const groupes = $derived(llm ? groupesLlm(llm.params) : []);
  /** La consigne affichée dans le panneau des consignes : une seule à la fois, choisie dans la liste. */
  let consigneChoisie = $state('');
  const consigne = $derived(llm?.consignes.find((c) => c.cle === consigneChoisie) ?? llm?.consignes[0]);

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

  /** Une consigne remise à l'origine, ou réécrite par rapport à son origine. */
  function reecrite(c: { cle: string; defaut: string; modifie: boolean }): boolean {
    return c.modifie || (consignes[c.cle] != null && consignes[c.cle] !== c.defaut);
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
  <p class="discret">Aucun LLM dans modelesllm. Les LLM téléchargés par la version 2 sont repris.</p>
{:else if etat && llm}
  {#if etat.noteDefaut}<p class="discret">{etat.noteDefaut}</p>{/if}

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

  <Panneau titre="Cible" accentue>
    <div class="ligne">
      <label class="modele">
        Modèle
        <select value={fichier} onchange={(e) => choisir(e.currentTarget.value)}>
          {#each etat.llm as l (l.fichier)}
            <option value={l.fichier}>{l.nom}{l.fichier === etat.defautLlm ? ' (par défaut)' : ''}</option>
          {/each}
        </select>
      </label>
      <Bouton petit desactive={analyse} onclick={() => void adapter()}>
        {analyse ? 'Analyse…' : '⚡ Adapter à ma machine'}
      </Bouton>
    </div>
    {#each groupes as g (g.titre)}
      <hr class="sep" />
      <p class="etiquette">{g.titre}</p>
      <div class="grille">
        {#each g.params as p (p.cle)}
          <ChampReglage param={p} bind:saisies />
        {/each}
      </div>
    {/each}
    <div class="pied">
      <span class="discret">{texteChangements(nombreChangements)}</span>
      <Bouton genre="principal" desactive={nombreChangements === 0} onclick={() => void enregistrer()}>Enregistrer</Bouton>
    </div>
  </Panneau>

  {#if consigne}
    <Panneau titre="Consignes du LLM">
      <div class="ligne">
        <label class="modele">
          Consigne
          <select value={consigne.cle} onchange={(e) => (consigneChoisie = e.currentTarget.value)}>
            {#each llm.consignes as c (c.cle)}
              <option value={c.cle}>{nomConsigne(c.cle)}{reecrite(c) ? ' · réécrite' : ''}</option>
            {/each}
          </select>
        </label>
        <Bouton genre="discret" petit desactive={!reecrite(consigne)} onclick={() => (consignes = { ...consignes, [consigne.cle]: null })}>
          ↺ Consigne d'origine
        </Bouton>
      </div>
      <textarea
        rows="10"
        aria-label={nomConsigne(consigne.cle)}
        value={consigne.cle in consignes ? (consignes[consigne.cle] ?? consigne.defaut) : consigne.valeur}
        oninput={(e) => (consignes = { ...consignes, [consigne.cle]: e.currentTarget.value })}
      ></textarea>
      <p class="discret verrou">🔒 Une clause sur les mineurs est ajoutée à la fin : elle n'est pas modifiable.</p>
      <details>
        <summary class="discret">Voir la clause</summary>
        <p class="code clause">{etat.clauseMineurs}</p>
      </details>
    </Panneau>
  {/if}
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
    flex-direction: column;
    gap: 4px;
    color: var(--texte-2);
    font-size: 13px;
  }

  .modele {
    min-width: 280px;
  }

  .sep {
    margin: 14px 0;
    border: 0;
    border-top: 1px solid var(--ligne);
  }

  .etiquette {
    margin: 0 0 10px;
    color: var(--texte-2);
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.14em;
    text-transform: uppercase;
  }

  .pied {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px 16px;
    justify-content: space-between;
    margin-top: 16px;
  }

  .verrou {
    margin: 8px 0 0;
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
