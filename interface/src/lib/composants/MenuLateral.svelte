<!--
  Le menu latéral, opaque (le fond animé ne passe jamais dessous) : la marque (logo, ARIA, le nom de l'appli en
  orange), les familles de pages avec leur titre cyan et son filet, puis « Quitter » sous un filet, et en bas la
  version. Le logo sert aussi de poignée pour déplacer la fenêtre.
-->
<script lang="ts" generics="P extends string">
  import Logo from './Logo.svelte';

  interface Famille {
    titre: string;
    pages: { id: P; titre: string }[];
  }

  interface Props {
    nom: string;
    version: string;
    familles: Famille[];
    page: P;
    /** Appli en arrière-plan : la croix range dans l'icône, « Quitter » arrête vraiment. */
    quitter?: (() => void) | undefined;
  }

  let { nom, version, familles, page = $bindable(), quitter }: Props = $props();
</script>

<aside class="menu">
  <div class="marque" data-tauri-drag-region>
    <Logo />
    <div class="noms" data-tauri-drag-region>
      <strong data-tauri-drag-region>Aria</strong>
      <span data-tauri-drag-region>{nom}</span>
    </div>
  </div>

  <nav aria-label="Pages">
    {#each familles as famille (famille.titre)}
      <p class="famille">{famille.titre}</p>
      {#each famille.pages as entree (entree.id)}
        <button
          type="button"
          class="entree"
          class:active={entree.id === page}
          aria-current={entree.id === page ? 'page' : undefined}
          onclick={() => (page = entree.id)}>{entree.titre}</button
        >
      {/each}
    {/each}
  </nav>

  {#if quitter}
    <div class="zone-quitter">
      <button type="button" class="quitter" onclick={quitter}>
        <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 3v9" /><path d="M6.3 6.8a8 8 0 1 0 11.4 0" /></svg>
        Quitter
      </button>
    </div>
  {/if}

  <div class="pied code">
    <span>v{version}</span>
    <span>tout en local</span>
  </div>
</aside>

<style>
  .menu {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow-y: auto;
    padding: 20px 0 16px;
    background: var(--panneau);
    border-right: 1px solid var(--ligne);
  }

  .marque {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 0 18px 10px;
  }

  .noms {
    display: flex;
    flex-direction: column;
  }

  strong {
    color: #fff;
    font-size: 22px;
    font-weight: 700;
    letter-spacing: 0.08em;
    line-height: 1.1;
    text-transform: uppercase;
    text-shadow: 0 0 14px color-mix(in srgb, var(--cyan) 60%, transparent);
  }

  span {
    color: var(--orange);
    font-family: var(--police-code);
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.35em;
    text-transform: uppercase;
  }

  nav {
    display: flex;
    flex-direction: column;
  }

  .famille {
    display: flex;
    align-items: center;
    gap: 10px;
    margin: 0;
    padding: 22px 20px 8px;
    color: var(--cyan);
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.2em;
    text-transform: uppercase;
  }

  .famille::after {
    content: '';
    flex: 1;
    height: 1px;
    background: var(--ligne);
  }

  .entree {
    height: 42px;
    padding: 0 20px;
    border: 0;
    border-left: 3px solid transparent;
    background: none;
    color: var(--texte-2);
    font: 600 12.5px var(--police);
    letter-spacing: 0.12em;
    text-align: left;
    text-transform: uppercase;
    cursor: pointer;
  }

  .entree:hover,
  .entree:focus-visible {
    color: var(--texte);
    outline: none;
  }

  .entree:focus-visible {
    box-shadow: inset 0 0 0 1px var(--cyan);
  }

  .entree.active {
    border-left-color: var(--cyan);
    background: color-mix(in srgb, var(--cyan) 7%, transparent);
    color: var(--cyan);
  }

  .zone-quitter {
    margin: 16px 20px 0;
    padding-top: 14px;
    border-top: 1px solid var(--ligne);
  }

  .quitter {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    padding: 6px 0;
    border: 0;
    background: none;
    color: var(--rouge);
    font: 600 12px var(--police);
    letter-spacing: 0.12em;
    text-transform: uppercase;
    cursor: pointer;
  }

  .quitter svg {
    width: 14px;
    height: 14px;
    fill: none;
    stroke: currentColor;
    stroke-width: 2.2;
    stroke-linecap: round;
  }

  .pied {
    display: flex;
    flex-direction: column;
    margin-top: auto;
    padding: 16px 20px 0;
    color: var(--texte-2);
    font-size: 11px;
  }

  .pied span {
    color: inherit;
    font-weight: 400;
    letter-spacing: normal;
    text-transform: none;
  }
</style>
