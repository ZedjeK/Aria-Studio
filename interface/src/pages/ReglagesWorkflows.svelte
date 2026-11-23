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
  import { changements, type Saisies } from '../lib/reglages';
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
  let id = $state('');
  let saisies = $state<Saisies>({});
  let saisiesVariantes = $state<Record<string, Saisies>>({});

  const workflow = $derived(etat?.workflows.find((w) => w.id === id));
  const aEnregistrer = $derived(workflow !== undefined && Object.keys(changements(workflow.params, saisies)).length > 0);
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

  async function enregistrer() {
    if (!workflow) return;
    try {
      remplacer(await modifierReglagesWorkflow(workflow.id, changements(workflow.params, saisies)));
      saisies = {};
      signaler(`Réglages de ${workflow.nom} enregistrés.`);
    } catch (e) {
      signaler(messageErreur(e), true);
    }
  }

  async function enregistrerVariante(fichier: string) {
    const variante = workflow?.variantes.find((v) => v.fichier === fichier);
    if (!workflow || !variante) return;
    try {
      remplacer(
        await modifierReglagesVariante(workflow.id, fichier, changements(variante.params, saisiesVariantes[fichier] ?? {})),
      );
      saisiesVariantes = { ...saisiesVariantes, [fichier]: {} };
      signaler(`Réglages de ${fichier} enregistrés.`);
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
  <div class="ligne">
    <label>
      Workflow
      <select value={id} onchange={(e) => choisir(e.currentTarget.value)}>
        {#each etat.workflows as w (w.id)}
          <option value={w.id}>{w.nom}</option>
        {/each}
      </select>
    </label>
    <span class="discret">{workflow.famille} · modeles\{workflow.dossierModeles.replaceAll('/', '\\')}</span>
    <Bouton onclick={() => void recommander()}>Recommander</Bouton>
    <Bouton genre="principal" desactive={!aEnregistrer} onclick={() => void enregistrer()}>Enregistrer</Bouton>
  </div>

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

  <Panneau titre="Réglages du workflow" accentue>
    {#if workflow.params.length === 0}
      <p class="discret">Rien à régler pour ce workflow.</p>
    {:else}
      <div class="grille">
        {#each workflow.params as p (p.cle)}
          <ChampReglage param={p} bind:saisies />
        {/each}
      </div>
    {/if}
  </Panneau>

  {#each workflow.variantes as v (v.fichier)}
    {#if v.params.length > 0}
      <Panneau titre={`${v.fichier}${v.origine ? ' · origine' : ''}`}>
        <div class="grille">
          {#each v.params as p (p.cle)}
            <ChampReglage
              param={p}
              bind:saisies={
                () => saisiesVariantes[v.fichier] ?? {},
                (nouvelles) => (saisiesVariantes = { ...saisiesVariantes, [v.fichier]: nouvelles })
              }
            />
          {/each}
        </div>
        <div class="ligne">
          <Bouton
            petit
            desactive={Object.keys(changements(v.params, saisiesVariantes[v.fichier] ?? {})).length === 0}
            onclick={() => void enregistrerVariante(v.fichier)}>Enregistrer ce checkpoint</Bouton
          >
        </div>
      </Panneau>
    {/if}
  {/each}
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
</style>
