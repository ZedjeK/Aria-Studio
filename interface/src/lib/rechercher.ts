// Modèles → Rechercher : types, sources, tris, dossiers et lisibilité des résultats.

import type { FamilleDecouvrir, FichierTrouve, SourceRecherche, TypeRecherche } from './api';

export interface TypePropose {
  id: TypeRecherche;
  titre: string;
  aide: string;
}

/** Les types cherchés, comme les pastilles de la v2. */
export const TYPES: TypePropose[] = [
  {
    id: 'image',
    titre: 'Modèles image',
    aide: "Checkpoints qui vont dans un de tes workflows d'image : le dossier de leur famille est choisi pour toi.",
  },
  {
    id: 'video',
    titre: 'Modèles vidéo',
    aide: 'Checkpoints pour tes workflows vidéo : le dossier de leur famille est choisi pour toi.',
  },
  { id: 'lora', titre: 'LoRA', aide: "LoRA pour une famille d'Aria : choisis la famille pour ne voir que les siennes." },
  { id: 'llm', titre: 'LLM (GGUF)', aide: 'LLM pour llama.cpp, sur Hugging Face : la VRAM libre dit lesquels tiennent.' },
];

/** Les LLM ne se cherchent que sur Hugging Face. */
export function sources(type: TypeRecherche): SourceRecherche[] {
  return type === 'llm' ? ['huggingface'] : ['civitai', 'huggingface'];
}

export function libelleSource(source: SourceRecherche): string {
  return source === 'civitai' ? 'Civitai' : 'Hugging Face';
}

/** Les tris d'une source ; « tendances » n'existe que sur Hugging Face. */
export function tris(source: SourceRecherche): { id: string; titre: string }[] {
  const communs = [
    { id: 'telechargements', titre: 'Les plus téléchargés' },
    { id: 'recents', titre: 'Les plus récents' },
    { id: 'notes', titre: 'Les mieux notés' },
  ];
  return source === 'huggingface' ? [...communs, { id: 'tendances', titre: 'Tendances' }] : communs;
}

/** Le dossier d'une famille pour ce type : ses LoRA ou ses checkpoints. */
export function dossierDeFamille(famille: FamilleDecouvrir, type: TypeRecherche): string {
  return `${famille.nature}/${famille.famille}/${type === 'lora' ? 'loras' : 'modeles'}`;
}

const VERDICTS: Record<NonNullable<FichierTrouve['vram']>, string> = {
  tient: 'tient dans la VRAM',
  juste: 'juste dans la VRAM',
  deborde: 'déborde de la VRAM',
};

/** Ce que la VRAM libre dit d'un GGUF. */
export function libelleVram(vram: FichierTrouve['vram']): string {
  return vram ? VERDICTS[vram] : '';
}

/** Un compte court : 950, 12,3 k, 1,2 M. */
export function compte(n: number): string {
  const virgule = (x: number) => x.toFixed(1).replace('.', ',').replace(',0', '');
  if (n < 1000) return String(n);
  if (n < 1_000_000) return `${virgule(n / 1000)} k`;
  return `${virgule(n / 1_000_000)} M`;
}

/** Ce qu'une page source conseille, en une ligne (rien s'il n'y a rien). */
export function conseil(nom: string, s: { statut: string; valeurs: number[] } | null): string {
  if (!s || s.statut === 'aucun' || s.valeurs.length === 0) return '';
  const valeurs = s.valeurs.join(' ou ');
  return s.statut === 'unique' ? `${nom} conseillés par la page : ${valeurs}` : `${nom} lus sur la page (à vérifier) : ${valeurs}`;
}
