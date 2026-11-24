<!--
  Réglages → Modèles image et vidéo, comme en v2 : les réglages de chaque workflow (CFG, sampler, scheduler, shift,
  mégapixels, format, longueur du prompt, checkpoint par défaut), puis ceux de chaque checkpoint de sa famille (LoRA du
  workflow gardée ou non, steps). Seuls les écarts à l'origine sont enregistrés. « Recommander » montre ce que la fiche
  du modèle recommande, avant → après, et n'applique que ce qui est confirmé.
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import {
    etatReglages,
    messageErreur,
    modifierReglagesVariante,
    modifierReglagesWorkflow,
    recommanderWorkflow,
    type EtatReglages,
    type EtatWorkflow,
    type Proposition,
    type Recommandations,
  } from '../lib/api';
  import { ecarts, parCible } from '../lib/optimiseur';
  import { changements, filtrerReglages, nombreModifies, texteChangements, type Saisies } from '../lib/reglages';
  import ApercuPropositions from '../lib/composants/ApercuPropositions.svelte';
  import ASavoir from '../lib/composants/ASavoir.svelte';
  import Bouton from '../lib/composants/Bouton.svelte';
  import ChampReglage from '../lib/composants/ChampReglage.svelte';
  import Panneau from '../lib/composants/Panneau.svelte';
  import RechercheReglage from '../lib/composants/RechercheReglage.svelte';

  interface Props {
    signaler: (texte: string, erreur?: boolean) => void;
    /** Va à la page Modèles ; sans elle, pas de bouton. */
    ouvrirModeles?: (() => void) | undefined;
    recherche: string;
    modifies: boolean;
  }

  let { signaler, ouvrirModeles, recherche = $bindable(), modifies = $bindable() }: Props = $props();

  let etat = $state<EtatReglages | null>(null);
  let id = $state('');
  let saisies = $state<Saisies>({});
  let saisiesVariantes = $state<Record<string, Saisies>>({});

  const workflow = $derived(etat?.workflows.find((w) => w.id === id));
  /** Les variantes qui ont des changements à enregistrer. */
  const variantesModifiees = $derived(
    (workflow?.variantes ?? []).filter((v) => Object.keys(changements(v.params, saisiesVariantes[v.fichier] ?? {})).length > 0),
  );
  /** Tout ce qui diffère de l'enregistré : les réglages du workflow et ceux de ses checkpoints. */
  const nombreChangements = $derived(
    workflow === undefined
      ? 0
      : Object.keys(changements(workflow.params, saisies)).length +
          variantesModifiees.reduce(
            (somme, v) => somme + Object.keys(changements(v.params, saisiesVariantes[v.fichier] ?? {})).length,
            0,
          ),
  );
  const parDefaut = $derived(workflow?.variantes.find((v) => v.origine));
  /** Ce que les filtres laissent voir : les réglages du workflow, puis ceux de chaque checkpoint. */
  const paramsMontres = $derived(workflow ? filtrerReglages(workflow.params, saisies, recherche, modifies) : []);
  const variantesMontrees = $derived(
    (workflow?.variantes ?? [])
      .map((v) => ({ v, params: filtrerReglages(v.params, saisiesVariantes[v.fichier] ?? {}, recherche, modifies) }))
      .filter((x) => x.params.length > 0),
  );
  const nombreMontres = $derived(paramsMontres.length + variantesMontrees.reduce((s, x) => s + x.params.length, 0));
  const nombreModifiesTotal = $derived(
    workflow === undefined
      ? 0
      : nombreModifies(workflow.params, saisies) +
          workflow.variantes.reduce((s, v) => s + nombreModifies(v.params, saisiesVariantes[v.fichier] ?? {}), 0),
  );
  /** Les recommandations de la fiche du workflow affiché, le temps de l'aperçu. */
  let recommandations = $state<Recommandations | null>(null);
  const apercu = $derived(
    recommandations && workflow ? ecarts(recommandations.propositions, workflow.params, saisies) : null,
  );

  async function recommander() {
    if (!workflow) return;
    try {
      recommandations = await recommanderWorkflow(workflow.id);
    } catch (e) {
      signaler(messageErreur(e), true);
    }
  }

  async function appliquerRecommandations(choisies: Proposition[]) {
    if (!workflow) return;
    const groupe = parCible(choisies)[0];
    if (!groupe) return;
    try {
      remplacer(await modifierReglagesWorkflow(workflow.id, groupe.changements));
      const restantes: Saisies = { ...saisies };
      for (const cle of Object.keys(groupe.changements)) delete restantes[cle];
      saisies = restantes;
      recommandations = null;
      signaler(`${choisies.length} réglage${choisies.length > 1 ? 's' : ''} de ${workflow.nom} appliqué${choisies.length > 1 ? 's' : ''}.`);
    } catch (e) {
      signaler(messageErreur(e), true);
    }
  }

  async function relire() {
    try {
      etat = await etatReglages();
      if (!etat.workflows.some((w) => w.id === id)) id = etat.workflows[0]?.id ?? '';
    } catch (e) {
      signaler(messageErreur(e), true);
    }
  }

  function remplacer(mis: EtatWorkflow) {
    if (etat) etat.workflows = etat.workflows.map((w) => (w.id === mis.id ? mis : w));
  }

  function choisir(nouveau: string) {
    id = nouveau;
    saisies = {};
    saisiesVariantes = {};
  }

  /** Un seul « Enregistrer » : le workflow d'abord, puis chaque checkpoint modifié. */
  async function enregistrer() {
    if (!workflow) return;
    try {
      const aEnregistrerWorkflow = Object.keys(changements(workflow.params, saisies)).length > 0;
      if (aEnregistrerWorkflow) {
        remplacer(await modifierReglagesWorkflow(workflow.id, changements(workflow.params, saisies)));
        saisies = {};
      }
      for (const v of variantesModifiees) {
        remplacer(await modifierReglagesVariante(workflow.id, v.fichier, changements(v.params, saisiesVariantes[v.fichier] ?? {})));
        saisiesVariantes = { ...saisiesVariantes, [v.fichier]: {} };
      }
      signaler(`Réglages de ${workflow.nom} enregistrés.`);
    } catch (e) {
      signaler(messageErreur(e), true);
    }
  }

  onMount(() => void relire());
</script>

<ASavoir>
  Les réglages d'un workflow valent partout où il sert. Les steps se règlent par checkpoint : un checkpoint Turbo ou
  Lightning en demande peu, et intègre parfois déjà la LoRA du workflow.
  {#if etat && !etat.samplersDeComfyui}
    Les samplers proposés sont ceux de secours tant que ComfyUI n'a pas encore démarré.
  {/if}
</ASavoir>

{#if etat && workflow}
  <RechercheReglage bind:recherche bind:modifies nombreModifies={nombreModifiesTotal} {nombreMontres} />

  {#if recommandations && apercu}
    <ApercuPropositions
      titre={`Recommandations de la fiche · ${recommandations.nom}`}
      lignes={apercu.aChanger}
      alertes={recommandations.alertes}
      notes={[
        ...(apercu.dejaBon.length > 0
          ? [`Déjà aux valeurs recommandées : ${apercu.dejaBon.map((p) => p.libelle).join(', ')}.`]
          : []),
        'Les steps se règlent par checkpoint, plus bas : ils ne sont pas proposés ici.',
      ]}
      vide={recommandations.sansRecommandation
        ? 'La fiche de ce modèle ne range pas de recommandation officielle : rien à proposer.'
        : 'Les réglages suivent déjà ce que recommande la fiche de ce modèle.'}
      appliquer={appliquerRecommandations}
      fermer={() => (recommandations = null)}
    />
  {/if}

  <Panneau titre="Réglages d'un modèle" accentue>
    <div class="ligne">
      <label class="choix">
        Workflow
        <select value={id} onchange={(e) => choisir(e.currentTarget.value)}>
          {#each etat.workflows as w (w.id)}
            <option value={w.id}>{w.nom}</option>
          {/each}
        </select>
      </label>
      <Bouton petit onclick={() => void recommander()}>Recommander</Bouton>
    </div>
    <p class="discret">{workflow.famille} · modeles\{workflow.dossierModeles.replaceAll('/', '\\')}</p>
    {#if workflow.params.length === 0}
      <p class="discret">Rien à régler pour ce workflow.</p>
    {:else if nombreMontres === 0}
      <p class="discret">Aucun réglage ne correspond.</p>
    {:else}
      <div class="grille">
        {#each paramsMontres as p (p.cle)}
          <ChampReglage param={p} bind:saisies />
        {/each}
      </div>
    {/if}

    {#if workflow.variantes.length > 0}
      <hr class="sep" />
      {#if parDefaut}
        <p class="etiquette">Checkpoint par défaut</p>
        <p class="defaut">
          ★ <span class="code">{parDefaut.fichier}</span>
          {#if ouvrirModeles}<Bouton genre="discret" petit onclick={ouvrirModeles}>Ouvrir dans Modèles</Bouton>{/if}
        </p>
      {/if}
      <p class="etiquette">Checkpoints de la famille</p>
      <div class="variantes">
        {#each variantesMontrees as { v, params } (v.fichier)}
          {#if params.length > 0}
            <div class="variante">
              <span class="code nom">{v.fichier}{v.origine ? ' · origine' : ''}</span>
              {#each params as p (p.cle)}
                <ChampReglage
                  param={p}
                  bind:saisies={
                    () => saisiesVariantes[v.fichier] ?? {},
                    (nouvelles) => (saisiesVariantes = { ...saisiesVariantes, [v.fichier]: nouvelles })
                  }
                />
              {/each}
            </div>
          {/if}
        {/each}
      </div>
    {/if}

    <div class="pied">
      <span class="discret">{texteChangements(nombreChangements)}</span>
      <Bouton genre="principal" desactive={nombreChangements === 0} onclick={() => void enregistrer()}>Enregistrer</Bouton>
    </div>
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
    flex-direction: column;
    gap: 4px;
    color: var(--texte-2);
    font-size: 13px;
  }

  .choix {
    min-width: 260px;
  }

  .sep {
    margin: 14px 0;
    border: 0;
    border-top: 1px solid var(--ligne);
  }

  .etiquette {
    margin: 0 0 8px;
    color: var(--texte-2);
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.14em;
    text-transform: uppercase;
  }

  .defaut {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 12px;
  }

  .variantes {
    display: grid;
    gap: 8px;
  }

  .variante {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 20px;
    padding: 8px 0;
    border-bottom: 1px solid color-mix(in srgb, var(--ligne) 50%, transparent);
  }

  .variante .nom {
    flex: 1;
    min-width: 220px;
    font-size: 12px;
  }

  .pied {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px 16px;
    justify-content: space-between;
    margin-top: 16px;
  }

  .grille {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(240px, 1fr));
    gap: 14px 20px;
  }
</style>
