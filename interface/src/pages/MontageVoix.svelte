<!--
  Montage → Voix, comme en v2 : la bibliothèque des voix de référence des séquences. Une voix vient d'un essai (un gros
  plan en cabine silencieuse, dont seul le son est gardé) ou d'un fichier audio. Un essai reste un brouillon tant
  qu'il n'est pas gardé sous un nom ; les brouillons de plus d'un jour sont effacés.
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import {
    essayerVoix,
    garderVoix,
    importerVoix,
    lireVoix,
    messageErreur,
    supprimerVoix,
    type OptionsVoix,
    type VoixBibliotheque,
  } from '../lib/api';
  import { lireSeed } from '../lib/plateau';
  import Bouton from '../lib/composants/Bouton.svelte';
  import Panneau from '../lib/composants/Panneau.svelte';
  import SonVoix from '../lib/composants/SonVoix.svelte';
  import ChoixRushes from './ChoixRushes.svelte';

  interface Props {
    signaler: (texte: string, erreur?: boolean) => void;
    /** Une commande vient d'entrer dans la file du Plateau. */
    enFile: () => void;
    /** Change quand la file a fini quelque chose : la bibliothèque se relit. */
    rafraichir: number;
  }

  let { signaler, enFile, rafraichir }: Props = $props();

  let voix = $state<VoixBibliotheque[]>([]);
  let options = $state<OptionsVoix | null>(null);
  let phrase = $state('');
  let description = $state('');
  let personnage = $state<string[]>([]);
  let seedTexte = $state('');
  let noms = $state<Record<string, string>>({});
  let nomImport = $state('');
  let ecoutee = $state<string | null>(null);

  const seed = $derived(lireSeed(seedTexte));
  /** Le dernier essai pas encore gardé : il se garde dans le panneau de l'essai, sous un nom. */
  const brouillon = $derived(voix.find((v) => !v.gardee));
  /** La bibliothèque : les voix gardées, puis les autres essais à garder. */
  const autres = $derived(voix.filter((v) => v.id !== brouillon?.id));

  async function relire() {
    try {
      [voix, options] = await lireVoix(true);
    } catch (e) {
      signaler(messageErreur(e), true);
    }
  }

  async function essayer() {
    if (seed === undefined) return;
    try {
      await essayerVoix({ phrase, description, personnage: personnage[0] ?? null, seed });
      signaler('Essai de voix en file.');
      enFile();
    } catch (e) {
      signaler(messageErreur(e), true);
    }
  }

  async function garder(v: VoixBibliotheque) {
    try {
      await garderVoix(v.id, noms[v.id] ?? v.nom ?? '');
      signaler('Voix gardée dans la bibliothèque.');
      await relire();
    } catch (e) {
      signaler(messageErreur(e), true);
    }
  }

  async function supprimer(v: VoixBibliotheque) {
    try {
      await supprimerVoix(v.id);
      signaler(`${v.nom ?? 'Essai'} mis à la corbeille des voix.`);
      await relire();
    } catch (e) {
      signaler(messageErreur(e), true);
    }
  }

  function importer(e: Event) {
    const champ = e.currentTarget;
    if (!(champ instanceof HTMLInputElement)) return;
    const fichier = champ.files?.[0];
    if (!fichier) return;
    const lecteur = new FileReader();
    lecteur.onload = () => {
      const contenu = lecteur.result;
      if (typeof contenu !== 'string') return;
      void importerVoix(contenu, nomImport || fichier.name.replace(/\.[^.]+$/, ''), fichier.name)
        .then(async () => {
          signaler('Voix ajoutée à la bibliothèque.');
          nomImport = '';
          await relire();
        })
        .catch((err: unknown) => signaler(messageErreur(err), true));
    };
    lecteur.readAsDataURL(fichier);
    champ.value = '';
  }

  $effect(() => {
    // La file a fini quelque chose (un essai, peut-être) : la bibliothèque se relit.
    if (rafraichir >= 0) void relire();
  });

  onMount(() => {
    void relire();
  });
</script>

<Panneau titre="Essai de voix" accentue>
  <p class="discret">
    Un essai fait dire une phrase au personnage en cabine silencieuse
    {options ? `(≈ ${Math.round(options.estimationS / 60)} min)` : ''} : seul le son est gardé, si tu le gardes.
  </p>
  <div class="grille">
    <label>
      <span class="etiquette">Phrase d'essai</span>
      <input type="text" maxlength={options?.longueurPhrase ?? 200} placeholder={options?.phraseDefaut ?? ''} bind:value={phrase} />
    </label>
    <label>
      <span class="etiquette">Qui parle, et comment</span>
      <input
        type="text"
        maxlength={options?.longueurDescription ?? 300}
        placeholder="une femme d'une trentaine d'années, voix douce et un peu grave"
        bind:value={description}
      />
    </label>
    <label>
      <span class="etiquette">Seed</span>
      <input type="text" inputmode="numeric" placeholder="au hasard" bind:value={seedTexte} />
    </label>
  </div>
  <details>
    <summary>Personnage (facultatif)</summary>
    <ChoixRushes max={1} bind:selection={personnage} {signaler} />
  </details>
  <div class="actions">
    <Bouton petit desactive={seed === undefined} onclick={() => void essayer()}>
      {brouillon ? '🔁 Autre prise' : '🎙️ Tourner l’essai'}
    </Bouton>
  </div>
  {#if brouillon}
    <div class="brouillon">
      <span class="discret">
        Essai à garder{brouillon.duree_son ? ` · ${brouillon.duree_son} s` : ''}{brouillon.phrase ? ` · « ${brouillon.phrase} »` : ''}
      </span>
      <SonVoix id={brouillon.id} />
      <div class="actions">
        <label class="nom">
          <span class="etiquette">Nom de la voix</span>
          <input type="text" maxlength={options?.longueurNom ?? 40} placeholder="Voix de Maya, version calme" bind:value={noms[brouillon.id]} />
        </label>
        <Bouton genre="principal" onclick={() => void garder(brouillon)}>✓ Garder</Bouton>
        <Bouton genre="discret" petit onclick={() => void supprimer(brouillon)}>✕ Jeter</Bouton>
      </div>
    </div>
  {/if}
</Panneau>

<Panneau titre={`Bibliothèque de voix · ${autres.length}`}>
  <ul class="voix">
    {#each autres as v (v.id)}
      <li>
        <div class="texte">
          <strong>{v.nom ?? 'Essai à garder'}</strong>
          <span class="discret">
            {v.source === 'fichier' ? 'fichier audio' : 'essai'}{v.duree_son ? ` · ${v.duree_son} s` : ''}{v.phrase
              ? ` · « ${v.phrase} »`
              : ''}
          </span>
          {#if ecoutee === v.id}
            <SonVoix id={v.id} />
          {/if}
        </div>
        <div class="actions">
          <Bouton genre="discret" petit onclick={() => (ecoutee = ecoutee === v.id ? null : v.id)}>
            {ecoutee === v.id ? 'Fermer' : '▶ Écouter'}
          </Bouton>
          {#if !v.gardee}
            <input type="text" maxlength={options?.longueurNom ?? 40} placeholder="Nom" bind:value={noms[v.id]} />
            <Bouton petit onclick={() => void garder(v)}>Garder</Bouton>
          {/if}
          <Bouton genre="discret" petit onclick={() => void supprimer(v)}>✕ Corbeille</Bouton>
        </div>
      </li>
    {:else}
      <li class="discret">Aucune voix gardée pour l'instant : tourne un essai, ou importe un fichier audio.</li>
    {/each}
  </ul>
  <div class="actions import">
    <input type="text" maxlength={options?.longueurNom ?? 40} placeholder="Nom de la voix importée" bind:value={nomImport} aria-label="Nom de la voix importée" />
    <label class="bouton-fichier">
      📁 Importer un fichier audio…
      <input type="file" accept=".wav,.mp3,.flac,.ogg,audio/*" onchange={importer} />
    </label>
    <span class="discret">{options?.formats.join(', ') ?? 'wav, mp3, flac, ogg'}, {options?.dureeMin ?? 3} s au moins</span>
  </div>
</Panneau>

<style>
  .grille {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
    gap: 10px;
    margin: 10px 0;
  }

  label {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .etiquette {
    color: var(--texte-2);
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.14em;
    text-transform: uppercase;
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px;
    margin: 8px 0;
  }

  .brouillon {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-top: 12px;
    padding: 12px 14px;
    border: 1px solid var(--ligne);
    background: var(--fond);
  }

  .nom {
    min-width: 240px;
  }

  .import {
    margin-top: 14px;
  }

  .bouton-fichier {
    position: relative;
    flex-direction: row;
    padding: 6px 14px;
    border: 1px solid var(--ligne);
    color: var(--texte-2);
    cursor: pointer;
  }

  .bouton-fichier input {
    position: absolute;
    inset: 0;
    opacity: 0;
    cursor: pointer;
  }

  .voix {
    display: flex;
    flex-direction: column;
    gap: 10px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .voix li {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 12px;
  }

  .texte {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: 2px;
    min-width: 200px;
  }

  summary {
    cursor: pointer;
  }
</style>
