<!--
  Le logo de l'appli dans le menu latéral : src/lib/logo.svg, copie de docs/logo.svg écrite par tools/icone.ps1 avec
  l'icône Windows. Le dessin ne se change que dans docs/logo.svg.
-->
<script lang="ts">
  import logo from '../logo.svg';

  interface Props {
    taille?: number;
  }

  let { taille = 44 }: Props = $props();
</script>

<!-- Dans le menu latéral, le logo sert aussi de poignée pour déplacer la fenêtre. -->
<img src={logo} width={taille} height={taille} alt="" draggable="false" data-tauri-drag-region />

<style>
  img {
    display: block;
    flex: none;
  }
</style>
