<!--
  ✕ Annuler pendant que le LLM écrit (Scénario), comme en v2 : la lecture de sa réponse s'arrête, la connexion est
  coupée et le LLM cesse d'écrire. Affiché seulement le temps d'une écriture ; l'écriture rend alors « Annulé ».
-->
<script lang="ts">
  import { annulerEcriture } from '../api';
  import Bouton from './Bouton.svelte';

  let demande = $state(false);

  async function annuler() {
    demande = true;
    try {
      await annulerEcriture();
    } catch {
      demande = false;
    }
  }
</script>

<Bouton genre="discret" petit desactive={demande} onclick={() => void annuler()}>
  {demande ? 'Annulation…' : '✕ Annuler'}
</Bouton>
