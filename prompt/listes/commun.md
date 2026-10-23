---
type: liste
titre: Rendu commun - Aria Artiste
tags: [style, lumiere, cadrage, objectif, palette, prompt, liste]
version: "1.0"
---

# Rendu (commun à tous les sujets)

> S'applique quel que soit le sujet. Toutes les sous-listes sont facultatives.
> Le texte à afficher dans l'image n'est pas une liste : c'est un champ libre de l'interface (les modèles Qwen-Image l'écrivent très bien).

---

## Style

| Valeur                        | Prompt image                                          |
| ----------------------------- | ----------------------------------------------------- |
| Photo réaliste                | photorealistic photograph                             |
| Photo de cinéma               | cinematic film still                                  |
| Photo argentique              | analog film photo, 35mm film grain                    |
| Portrait studio               | professional studio portrait photography              |
| Photo de mode                 | high-fashion editorial photography                    |
| Polaroid                      | polaroid photo                                        |
| Manga noir et blanc           | black and white manga illustration with screentones   |
| Planche de manga              | manga panel, inked line art, dynamic composition      |
| Anime                         | anime style illustration, cel shading                 |
| Anime années 90               | 1990s retro anime style                               |
| Seinen réaliste               | detailed seinen manga style                           |
| Webtoon coréen                | Korean webtoon style, clean digital coloring          |
| Ghibli (aquarelle douce)      | soft hand-painted Studio Ghibli-like style            |
| Illustration numérique        | detailed digital illustration                         |
| Concept art                   | video game concept art                                |
| Peinture à l'huile            | oil painting                                          |
| Aquarelle                     | watercolor painting                                   |
| Estampe japonaise (ukiyo-e)   | ukiyo-e woodblock print                               |
| Encre et lavis                | Japanese ink wash painting (sumi-e)                   |
| Art nouveau                   | art nouveau illustration                              |
| Comics américain              | American comic book style, bold ink                   |
| Pixel art                     | pixel art                                             |
| Rendu 3D                      | 3D render, octane render                              |
| Figurine peinte (miniature)   | photo of a painted tabletop miniature figurine        |
| Synthwave                     | synthwave retro-futuristic art                        |
| Cyberpunk                     | cyberpunk aesthetic                                   |
| Croquis au crayon             | pencil sketch                                         |

## Lumière

| Valeur                   | Prompt image                              |
| ------------------------ | ----------------------------------------- |
| Naturelle douce          | soft natural light                        |
| Lumière de fenêtre       | soft window light                         |
| Heure dorée              | warm golden hour light                    |
| Contre-jour              | backlit, rim light                        |
| Clair-obscur             | dramatic chiaroscuro lighting             |
| Lumière de studio        | professional studio lighting, softbox     |
| Néons roses et cyan      | pink and cyan neon lighting               |
| Lumière de bougie        | warm candlelight                          |
| Clair de lune            | cool moonlight                            |
| Volumétrique             | volumetric light rays                     |
| Lumière dure de midi     | harsh midday sunlight, strong shadows     |
| Éclairage de film noir   | film noir lighting with venetian blind shadows |
| Bioluminescence          | glowing bioluminescent light              |
| Lueur de feu             | flickering firelight                      |

## Cadrage

| Valeur               | Prompt image                    |
| -------------------- | ------------------------------- |
| Gros plan            | close-up                        |
| Très gros plan       | extreme close-up                |
| Portrait buste       | head and shoulders portrait     |
| Plan taille          | medium shot, waist up           |
| Plan américain       | cowboy shot, mid-thigh up       |
| Plan en pied         | full-body shot                  |
| Plan large           | wide shot                       |
| Très plan large      | extreme wide shot, epic scale   |
| Plan sur les mains   | close-up on the hands           |

## Angle de vue

| Valeur              | Prompt image                 |
| ------------------- | ---------------------------- |
| À hauteur d'yeux    | eye-level shot               |
| Plongée             | high-angle shot              |
| Contre-plongée      | low-angle shot               |
| Vue de dessus       | top-down bird's-eye view     |
| Vue de profil       | side profile view            |
| Vue de dos          | view from behind             |
| Trois-quarts        | three-quarter view           |
| Angle hollandais    | dutch angle                  |
| Vue subjective      | first-person point of view   |

## Objectif

| Valeur                       | Prompt image                                   |
| ---------------------------- | ---------------------------------------------- |
| Grand-angle                  | wide-angle lens                                |
| 35 mm                        | 35mm lens                                      |
| 50 mm                        | 50mm lens                                      |
| 85 mm portrait               | 85mm portrait lens, shallow depth of field     |
| Téléobjectif                 | telephoto lens, compressed background          |
| Macro                        | macro lens, extreme detail                     |
| Fisheye                      | fisheye lens                                   |
| Arrière-plan flou (bokeh)    | shallow depth of field, creamy bokeh           |
| Tout net                     | deep focus, everything sharp                   |

## Palette

| Valeur                  | Prompt image                          |
| ----------------------- | ------------------------------------- |
| Couleurs vives          | vibrant saturated colors              |
| Pastel                  | soft pastel color palette             |
| Tons chauds             | warm color palette                    |
| Tons froids             | cool blue color palette               |
| Sarcelle et orange      | teal and orange color grading         |
| Désaturée               | desaturated muted colors              |
| Monochrome              | monochrome                            |
| Noir et blanc           | black and white                       |
| Sépia                   | sepia tones                           |
| Rose et cyan néon       | neon pink and cyan palette            |
| Tons terreux            | earthy tones                          |

## Finition

| Valeur                  | Prompt image                              |
| ----------------------- | ----------------------------------------- |
| Très détaillé           | highly detailed, intricate details        |
| Grain de film           | subtle film grain                         |
| Netteté maximale        | ultra sharp focus                         |
| Rendu doux et rêveur    | soft dreamy glow                          |
| Composition symétrique  | symmetrical composition                   |
| Règle des tiers         | rule-of-thirds composition                |
| Minimaliste             | minimalist composition, negative space    |

---

## Incompatibilités

- Style : Manga noir et blanc ↔ Palette : Couleurs vives
- Style : Manga noir et blanc ↔ Palette : Pastel
- Style : Manga noir et blanc ↔ Palette : Rose et cyan néon
- Style : Manga noir et blanc ↔ Palette : Sarcelle et orange
- Objectif : Macro ↔ Cadrage : Très plan large
- Objectif : Macro ↔ Cadrage : Plan large
- Objectif : Téléobjectif ↔ Cadrage : Très gros plan
- Style : Pixel art ↔ Objectif : Arrière-plan flou (bokeh)
- Style : Manga noir et blanc ↔ Lumière : Néons roses et cyan
