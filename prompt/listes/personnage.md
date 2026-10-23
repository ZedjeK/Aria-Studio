---
type: liste
titre: Personnage - Aria Artiste
tags: [personnage, physique, tenue, pose, prompt, liste]
version: "2.0"
origine: "Allure des personas - Aria Compagnon v1.1, reprise et étendue"
---

# Personnage

> Une valeur par sous-liste, toutes facultatives. La colonne « Prompt image » est la traduction fixe envoyée au modèle ; vide = rien n'est ajouté.
> **Remplace** : la valeur prend la place de la sous-liste indiquée (ex. un trait marquant « Nez retroussé » remplace la sous-liste Nez ; une robe remplace le Bas).
> **Adulte** : `oui` = valeur masquée quand l'âge choisi est mineur (enfant, ado). Si toutes les valeurs d'une sous-liste sont adultes, la sous-liste entière disparaît.
> **Genre** : `femme` ou `homme` = valeur proposée seulement pour ce genre ; vide = pour tous.
> **Famille** (Origine) : europe, asie-est, asie-sud-est, asie-sud, moyen-orient, afrique, ameriques, oceanie ; une origine métisse en cumule plusieurs.
> **Origines** (Teint, Forme des yeux, Couleur des yeux, Nez) : familles pour lesquelles la valeur est proposée quand une origine est choisie ; vide = pour toutes. C'est une aide au choix, pas une règle : la case « Toutes les options » de l'interface lève le filtre.
> **Avec <sous-liste>** (ex. « Avec Silhouette » sur Taille, Poitrine, Hanches ; « Avec Longueur des cheveux » sur Coiffure) : valeurs de cette sous-liste avec lesquelles la valeur reste cohérente ; vide = toutes. C'est une règle : une valeur qui ne va pas avec le choix fait est masquée et refusée.
> **Description** : une phrase affichée sous le menu (Silhouette).
> Les traductions sont neutres : le genre et l'âge ajoutent eux-mêmes « woman », « man », « girl »…
> Incompatibilités en fin de fichier ; une valeur présente dans plusieurs sous-listes se précise en « Sous-liste : Valeur ».

---

## Genre

| Valeur     | Prompt image |
| ---------- | ------------ |
| Femme      | woman        |
| Homme      | man          |
| Androgyne  | androgynous person |

## Âge

> « Prompt » remplace le nom donné par le genre. Un âge précis (ex. 12) peut être saisi en plus dans l'interface.

| Valeur                      | Prompt femme                        | Prompt homme                      | Prompt androgyne                       | Mineur |
| --------------------------- | ----------------------------------- | --------------------------------- | -------------------------------------- | ------ |
| Petit enfant (3-5 ans)      | little girl                         | little boy                        | small child                            | oui    |
| Enfant (6-11 ans)           | young girl                          | young boy                         | child                                  | oui    |
| Ado (12-17 ans)             | teenage girl                        | teenage boy                       | teenager                               | oui    |
| Jeune adulte (18-25 ans)    | young woman in her early twenties   | young man in his early twenties   | young adult in their early twenties    |        |
| Adulte (26-39 ans)          | woman in her thirties               | man in his thirties               | adult in their thirties                |        |
| Quarantaine (40-49 ans)     | woman in her forties                | man in his forties                | person in their forties                |        |
| Âge mûr (50-64 ans)         | mature woman in her fifties         | mature man in his fifties         | mature person in their fifties         |        |
| Âgé (65 ans et plus)        | elderly woman                       | elderly man                       | elderly person                         |        |

## Origine

> 17 origines bien distinctes pour le modèle (simplifiées le 2026-10-20) ; une nuance plus fine (« moitié coréenne », « bretonne »…) passe par les Précisions.

| Valeur                 | Prompt image                            | Famille          |
| ---------------------- | --------------------------------------- | ---------------- |
| Européenne de l'Ouest  | Western European                        | europe           |
| Nordique               | Scandinavian                            | europe           |
| Méditerranéenne        | Mediterranean                           | europe           |
| Slave                  | Slavic Eastern European                 | europe           |
| Japonaise              | Japanese                                | asie-est         |
| Coréenne               | Korean                                  | asie-est         |
| Chinoise               | Chinese                                 | asie-est         |
| Asiatique du Sud-Est   | Southeast Asian                         | asie-sud-est     |
| Eurasienne             | Eurasian, half European half East Asian | europe, asie-est |
| Sud-asiatique          | South Asian                             | asie-sud         |
| Moyen-orientale        | Middle Eastern                          | moyen-orient     |
| Africaine              | Black African                           | afrique          |
| Afro-descendante       | African American                        | afrique          |
| Latino-américaine      | Latina, Latin American                  | ameriques        |
| Amérindienne           | Native American                         | ameriques        |
| Polynésienne           | Polynesian                              | oceanie          |
| Métisse euro-africaine | mixed-race, half European half African  | europe, afrique  |

## Silhouette

| Valeur          | Prompt image                          | Adulte | Genre | Description                                                                                    |
| --------------- | ------------------------------------- | ------ | ----- | ---------------------------------------------------------------------------------------------- |
| Menue           | petite slender build                  |        |       | Petite et fine, avec une ossature délicate.                                                    |
| Fine et élancée | slim and slender build                |        |       | Mince, avec des lignes allongées ; avec la Taille « Grande », elle devient longiligne.         |
| Athlétique      | athletic toned build                  |        |       | Un corps entraîné et tonique, aux muscles dessinés mais sans volume.                           |
| Musclé(e)       | muscular build, broad shoulders       | oui    |       | Des muscles bien visibles et des épaules larges, mais naturels.                                |
| Bodybuilder     | heavily muscular bodybuilder physique | oui    |       | Une musculature massive et sculptée, poussée à l'extrême.                                      |
| Pulpeuse        | curvy voluptuous build                | oui    | femme | Des courbes généreuses et bien marquées (poitrine, hanches).                                   |
| En sablier      | hourglass figure                      | oui    | femme | Épaules et hanches de même largeur, taille très marquée : la proportion compte, pas le volume. |
| Trapu(e)        | stocky sturdy build                   |        |       | Compact et solide, avec une charpente large et forte.                                          |
| Ronde           | soft rounded plus-size build          |        |       | Une silhouette douce et arrondie, en surpoids, aux formes amples.                              |
| Bedonnant       | slightly pot-bellied build            | oui    | homme | Un corps ordinaire avec un petit ventre.                                                       |

## Taille

| Valeur      | Prompt image | Avec Silhouette                                                                             |
| ----------- | ------------ | ------------------------------------------------------------------------------------------- |
| Très petite | very short   |                                                                                             |
| Petite      | short        |                                                                                             |
| Moyenne     |              |                                                                                             |
| Grande      | tall         | Fine et élancée, Athlétique, Musclé(e), Bodybuilder, Pulpeuse, En sablier, Ronde, Bedonnant |
| Très grande | very tall    | Fine et élancée, Athlétique, Musclé(e), Bodybuilder, Pulpeuse, En sablier, Ronde, Bedonnant |

## Poitrine

| Valeur         | Prompt image    | Adulte | Genre | Avec Silhouette                                                                                       |
| -------------- | --------------- | ------ | ----- | ----------------------------------------------------------------------------------------------------- |
| Menue          | small bust      | oui    | femme | Menue, Fine et élancée, Athlétique, Musclé(e), Bodybuilder, En sablier, Trapu(e), Ronde, Bedonnant    |
| Moyenne        | medium bust     | oui    | femme |                                                                                                       |
| Généreuse      | large bust      | oui    | femme | Fine et élancée, Athlétique, Musclé(e), Bodybuilder, Pulpeuse, En sablier, Trapu(e), Ronde, Bedonnant |
| Très généreuse | very large bust | oui    | femme | Pulpeuse, En sablier, Ronde                                                                           |

## Hanches

| Valeur        | Prompt image         | Adulte | Genre | Avec Silhouette                                                               |
| ------------- | -------------------- | ------ | ----- | ----------------------------------------------------------------------------- |
| Étroites      | narrow hips          | oui    |       | Menue, Fine et élancée, Athlétique, Musclé(e), Bodybuilder, Bedonnant         |
| Harmonieuses  | proportionate hips   | oui    |       |                                                                               |
| Marquées      | curvy hips           | oui    | femme | Fine et élancée, Athlétique, Musclé(e), Pulpeuse, En sablier, Trapu(e), Ronde |
| Très marquées | very wide curvy hips | oui    | femme | Pulpeuse, En sablier, Ronde                                                   |

## Teint

| Valeur                            | Prompt image       | Origines                                                          |
| --------------------------------- | ------------------ | ----------------------------------------------------------------- |
| Porcelaine                        | porcelain skin     | europe, asie-est                                                  |
| Clair                             | fair skin          | europe, asie-est, asie-sud-est, moyen-orient, ameriques           |
| Clair rosé                        | fair rosy skin     | europe, asie-est                                                  |
| Clair avec des taches de rousseur | fair freckled skin | europe                                                            |
| Doré                              | golden skin        | asie-est, asie-sud-est, ameriques, oceanie                        |
| Olive                             | olive skin         | europe, moyen-orient, asie-sud, ameriques                         |
| Mat                               | light tan skin     | europe, moyen-orient, asie-sud, asie-sud-est, ameriques, oceanie  |
| Hâlé                              | tanned skin        |                                                                   |
| Caramel                           | caramel skin       | moyen-orient, asie-sud, asie-sud-est, ameriques, afrique, oceanie |
| Cuivré                            | copper-bronze skin | ameriques, asie-sud, afrique, oceanie                             |
| Brun                              | brown skin         | afrique, asie-sud, asie-sud-est, ameriques, oceanie               |
| Ébène                             | deep ebony skin    | afrique                                                           |

## Forme du visage

| Valeur       | Prompt image                        |
| ------------ | ----------------------------------- |
| Ovale        | oval face                           |
| Rond         | round face                          |
| En cœur      | heart-shaped face                   |
| Carré        | square face                         |
| Allongé      | long face                           |
| En losange   | diamond-shaped face                 |
| Triangulaire | triangular face with a pointed chin |
| Anguleux     | angular face with sharp features    |
| Juvénile     | soft youthful face                  |

## Forme des yeux

| Valeur                     | Prompt image          | Origines                                                                  |
| -------------------------- | --------------------- | ------------------------------------------------------------------------- |
| En amande                  | almond-shaped eyes    |                                                                           |
| En amande relevés          | upturned almond eyes  |                                                                           |
| Paupière simple (monolide) | monolid eyes          | asie-est, asie-sud-est                                                    |
| Paupière double fine       | subtle double eyelids | asie-est, asie-sud-est                                                    |
| Ronds                      | round eyes            | europe, moyen-orient, asie-sud, asie-sud-est, afrique, ameriques, oceanie |
| Grands et expressifs       | large expressive eyes | europe, moyen-orient, asie-sud, asie-sud-est, afrique, ameriques, oceanie |
| Tombants                   | downturned eyes       |                                                                           |
| Enfoncés                   | deep-set eyes         | europe, moyen-orient, asie-sud, afrique, ameriques, oceanie               |
| En amande étirés (félins)  | feline cat-like eyes  |                                                                           |

## Couleur des yeux

| Valeur                  | Prompt image                              | Origines                                  |
| ----------------------- | ----------------------------------------- | ----------------------------------------- |
| Marron foncé            | dark brown eyes                           |                                           |
| Noisette                | hazel eyes                                |                                           |
| Ambre                   | amber eyes                                | europe, moyen-orient, asie-sud, ameriques |
| Vert                    | green eyes                                | europe, moyen-orient                      |
| Vert d'eau              | pale aqua-green eyes                      | europe                                    |
| Bleu clair              | light blue eyes                           | europe                                    |
| Bleu gris               | blue-grey eyes                            | europe                                    |
| Gris                    | grey eyes                                 | europe                                    |
| Noirs                   | very dark, almost black eyes              |                                           |
| Bleu nuit               | deep dark blue eyes                       | europe                                    |
| Vairons                 | heterochromia, one brown and one blue eye | europe                                    |
| Rouge (fantasy)         | glowing red eyes                          |                                           |
| Violet (fantasy)        | violet eyes                               |                                           |
| Doré lumineux (fantasy) | luminous golden eyes                      |                                           |
| Cybernétiques           | glowing cybernetic eyes                   |                                           |

## Expression du regard

| Valeur       | Prompt image             | Adulte |
| ------------ | ------------------------ | ------ |
| Rieur        | smiling eyes             |        |
| Perçant      | piercing gaze            |        |
| Mélancolique | melancholic gaze         |        |
| Malicieux    | mischievous glint in the eyes |   |
| Doux         | gentle gaze              |        |
| Froid        | cold gaze                |        |
| Curieux      | curious gaze             |        |
| Rêveur       | dreamy gaze              |        |
| Méfiant      | wary gaze                |        |
| Déterminé    | determined gaze          |        |
| Provocateur  | provocative gaze         | oui    |
| Séducteur    | seductive gaze           | oui    |
| Intense      | intense gaze             |        |
| Fatigué      | tired eyes               |        |

## Expression du visage

| Valeur              | Prompt image                  |
| ------------------- | ----------------------------- |
| Neutre              |                               |
| Sourire éclatant    | bright wide smile             |
| Sourire timide      | shy smile                     |
| Sourire en coin     | smirk                         |
| Rire aux éclats     | laughing out loud             |
| Sérieuse            | serious expression            |
| Déterminée          | determined expression         |
| Boudeuse            | pouting                       |
| Surprise            | surprised expression          |
| Triste              | sad expression                |
| En larmes           | crying, tears on the cheeks   |
| En colère           | angry expression              |
| Pensive             | pensive expression            |
| Sereine             | serene expression             |
| Effrayée            | frightened expression         |
| Gênée               | embarrassed, blushing         |
| Moqueuse            | teasing expression            |
| Fatiguée            | exhausted expression          |

## Nez

| Valeur            | Prompt image         | Origines                                                      |
| ----------------- | -------------------- | ------------------------------------------------------------- |
| Fin et droit      | thin straight nose   |                                                               |
| Droit             | straight nose        |                                                               |
| Retroussé         | small upturned nose  |                                                               |
| Arrondi           | rounded nose         |                                                               |
| Large et doux     | soft wide nose       | asie-est, asie-sud-est, asie-sud, afrique, ameriques, oceanie |
| Aquilin           | aquiline nose        | europe, moyen-orient, asie-sud, ameriques                     |
| Légèrement busqué | slightly hooked nose | europe, moyen-orient, asie-sud                                |

## Lèvres

| Valeur                        | Prompt image                                 | Adulte |
| ----------------------------- | -------------------------------------------- | ------ |
| Fines                         | thin lips                                    |        |
| Pleines                       | full lips                                    |        |
| Pulpeuses                     | plump lips                                   | oui    |
| En cœur                       | heart-shaped lips with a defined cupid's bow |        |
| Larges                        | wide lips                                    |        |
| Lèvre inférieure plus charnue | fuller lower lip                             | oui    |

## Sourcils

| Valeur            | Prompt image           |
| ----------------- | ---------------------- |
| Fins et arqués    | thin arched eyebrows   |
| Droits            | straight eyebrows      |
| Épais et naturels | thick natural eyebrows |
| Doux et légers    | soft light eyebrows    |
| Marqués           | bold defined eyebrows  |
| Broussailleux     | bushy eyebrows         |

## Pilosité du visage

| Valeur               | Prompt image               | Adulte | Genre |
| -------------------- | -------------------------- | ------ | ----- |
| Rasé de près         | clean-shaven               |        | homme |
| Barbe de trois jours | light stubble              | oui    | homme |
| Barbe courte         | short neat beard           | oui    | homme |
| Barbe fournie        | full thick beard           | oui    | homme |
| Moustache            | mustache                   | oui    | homme |
| Bouc                 | goatee                     | oui    | homme |
| Favoris              | long sideburns             | oui    | homme |

## Trait marquant

| Valeur                                | Prompt image                      | Remplace       | Adulte |
| ------------------------------------- | --------------------------------- | -------------- | ------ |
| Pommettes hautes                      | high cheekbones                   |                |        |
| Fossettes au sourire                  | dimples when smiling              |                |        |
| Grain de beauté au-dessus de la lèvre | beauty mark above the lip         |                |        |
| Grain de beauté sur la joue           | beauty mark on the cheek          |                |        |
| Taches de rousseur sur le nez         | freckles across the nose          |                |        |
| Nez retroussé                         | small upturned nose               | Nez            |        |
| Nez aquilin                           | aquiline nose                     | Nez            |        |
| Nez fin et droit                      | thin straight nose                | Nez            |        |
| Lèvres pleines                        | full lips                         | Lèvres         |        |
| Lèvres fines                          | thin lips                         | Lèvres         |        |
| Mâchoire marquée                      | defined jawline                   |                | oui    |
| Sourcils épais                        | thick eyebrows                    | Sourcils       |        |
| Sourcils fins et arqués               | thin arched eyebrows              | Sourcils       |        |
| Yeux en amande                        | almond-shaped eyes                | Forme des yeux |        |
| Cils très longs                       | very long eyelashes               |                |        |
| Dents du bonheur                      | small gap between the front teeth |                |        |
| Sourire en coin                       | crooked half smile                |                |        |
| Menton fendu                          | cleft chin                        |                |        |
| Petite cicatrice au sourcil           | small scar through the eyebrow    |                |        |
| Cicatrice sur la joue                 | scar across the cheek             |                |        |
| Oreilles pointues d'elfe              | pointed elf ears                  |                |        |
| Cornes de démon                       | small curved demon horns          |                |        |
| Oreilles de chat (kemonomimi)         | cat ears                          |                |        |

## Couleur des cheveux

| Valeur                   | Prompt image                         |
| ------------------------ | ------------------------------------ |
| Noir corbeau             | raven black hair                     |
| Noir aux reflets bleutés | black hair with a blue sheen         |
| Brun foncé               | dark brown hair                      |
| Châtain                  | chestnut brown hair                  |
| Châtain clair            | light brown hair                     |
| Blond cendré             | ash blonde hair                      |
| Blond doré               | golden blonde hair                   |
| Blond platine            | platinum blonde hair                 |
| Auburn                   | auburn hair                          |
| Roux cuivré              | copper red hair                      |
| Roux flamboyant          | vibrant fiery red hair               |
| Poivre et sel            | salt-and-pepper hair                 |
| Gris                     | grey hair                            |
| Blanc                    | white hair                           |
| Balayage caramel         | brown hair with caramel balayage     |
| Bordeaux                 | dyed burgundy hair                   |
| Gris argenté teint       | dyed silver-grey hair                |
| Rose pastel              | pastel pink hair                     |
| Bleu électrique          | electric blue hair                   |
| Violet                   | purple hair                          |
| Vert menthe              | mint green hair                      |
| Blanc neige (anime)      | snow-white anime hair                |
| Dégradé de couleurs      | ombre gradient hair                  |
| Mèches colorées          | dark hair with colorful dyed streaks |

## Longueur des cheveux

| Valeur          | Prompt image                      |
| --------------- | --------------------------------- |
| Rasés           | buzz cut                          |
| Très courts     | very short hair                   |
| Courts          | short hair                        |
| Carré           | chin-length bob                   |
| Carré plongeant | angled bob, longer in front       |
| Mi-longs        | shoulder-length hair              |
| Longs           | long hair                         |
| Très longs      | very long hair reaching the waist |

## Texture des cheveux

| Valeur           | Prompt image             |
| ---------------- | ------------------------ |
| Raides           | straight hair            |
| Lisses et soyeux | sleek silky hair         |
| Ondulés          | wavy hair                |
| Bouclés          | curly hair               |
| Frisés           | tightly curled hair      |
| Crépus           | coily afro-textured hair |
| Ébouriffés       | messy tousled hair       |

## Coiffure

| Valeur                        | Prompt image                     | Genre | Avec Longueur des cheveux                                                |
| ----------------------------- | -------------------------------- | ----- | ------------------------------------------------------------------------ |
| Lâchés                        | hair worn down                   |       | Courts, Carré, Carré plongeant, Mi-longs, Longs, Très longs              |
| Queue de cheval haute         | high ponytail                    |       | Mi-longs, Longs, Très longs                                              |
| Queue de cheval basse         | low ponytail                     |       | Mi-longs, Longs, Très longs                                              |
| Chignon flou                  | messy bun                        |       | Mi-longs, Longs, Très longs                                              |
| Chignon strict                | sleek tight bun                  |       | Mi-longs, Longs, Très longs                                              |
| Tresse                        | single braid                     |       | Mi-longs, Longs, Très longs                                              |
| Deux tresses                  | two braids                       |       | Mi-longs, Longs, Très longs                                              |
| Couettes hautes (twin tails)  | high twin tails                  |       | Carré, Carré plongeant, Mi-longs, Longs, Très longs                      |
| Couettes basses               | low pigtails                     |       | Carré, Carré plongeant, Mi-longs, Longs, Très longs                      |
| Demi-queue                    | half-up half-down hairstyle      |       | Carré, Carré plongeant, Mi-longs, Longs, Très longs                      |
| Frange droite                 | hair down with blunt bangs       |       | Courts, Carré, Carré plongeant, Mi-longs, Longs, Très longs              |
| Frange rideau                 | hair down with curtain bangs     |       | Courts, Carré, Carré plongeant, Mi-longs, Longs, Très longs              |
| Rasés sur un côté             | side-shaved undercut             |       | Très courts, Courts, Carré, Carré plongeant, Mi-longs, Longs, Très longs |
| Undercut                      | undercut hairstyle               |       | Très courts, Courts, Carré, Carré plongeant, Mi-longs, Longs             |
| Coupe militaire               | military crew cut                | homme | Rasés, Très courts, Courts                                               |
| Mèche sur le côté             | side-swept fringe                |       | Très courts, Courts, Carré, Carré plongeant, Mi-longs                    |
| Chignon de samouraï           | samurai topknot                  |       | Mi-longs, Longs, Très longs                                              |
| Plaqués en arrière            | slicked-back hair                |       | Très courts, Courts, Carré, Carré plongeant, Mi-longs, Longs             |
| Dreadlocks                    | dreadlocks                       |       | Courts, Mi-longs, Longs, Très longs                                      |
| Attachés avec une pince       | hair clipped up with a claw clip |       | Mi-longs, Longs, Très longs                                              |
| Bandeau                       | hair held back with a headband   |       | Très courts, Courts, Carré, Carré plongeant, Mi-longs, Longs, Très longs |
| Ramenés sur une épaule        | hair swept over one shoulder     |       | Mi-longs, Longs, Très longs                                              |
| Chignon tenu par une baguette | bun held with a hair stick       |       | Mi-longs, Longs, Très longs                                              |

## Signe particulier

| Valeur                             | Prompt image                   | Adulte |
| ---------------------------------- | ------------------------------ | ------ |
| Aucun                              |                                |        |
| Tatouage discret au poignet        | small tattoo on the wrist      | oui    |
| Tatouage sur la nuque              | tattoo on the nape of the neck | oui    |
| Tatouage à la cheville             | ankle tattoo                   | oui    |
| Grand tatouage sur le bras         | large tattoo on the arm        | oui    |
| Tatouage dans le dos               | tattoo on the back             | oui    |
| Bras entièrement tatoué            | full sleeve tattoo             | oui    |
| Piercing au nez                    | nose piercing                  | oui    |
| Plusieurs piercings aux oreilles   | multiple ear piercings         | oui    |
| Piercing au nombril                | navel piercing                 | oui    |
| Cicatrice au genou                 | scar on the knee               |        |
| Cicatrice sur l'épaule             | scar on the shoulder           |        |
| Lunettes                           | wearing glasses                |        |
| Lunettes rondes                    | wearing round glasses          |        |
| Cache-œil                          | wearing an eyepatch            |        |
| Bras cybernétique                  | cybernetic prosthetic arm      |        |
| Taches de rousseur sur les épaules | freckled shoulders             |        |
| Tache de naissance                 | birthmark                      |        |
| Grain de beauté dans le cou        | beauty mark on the neck        |        |
| Ongles vernis de noir              | black nail polish              | oui    |
| Pansement sur la joue              | adhesive bandage on the cheek  |        |
| Mains abîmées par le métier        | work-worn hands                | oui    |

## Motif de tatouage

> Proposé seulement si le signe particulier est un tatouage.

| Valeur                   | Prompt image                   |
| ------------------------ | ------------------------------ |
| Constellation            | constellation design           |
| Pivoine                  | peony design                   |
| Branche de cerisier      | cherry blossom branch design   |
| Dragon japonais          | Japanese dragon design         |
| Carpe koï                | koi fish design                |
| Grande vague de Kanagawa | Great Wave off Kanagawa design |
| Masque Hannya            | Hannya mask design             |
| Fleurs en trait fin      | fine-line floral design        |
| Phrase manuscrite        | handwritten script             |
| Papillon                 | butterfly design               |
| Serpent                  | snake design                   |
| Croissant de lune        | crescent moon design           |
| Motif géométrique        | geometric design               |
| Circuits électroniques   | circuit board design           |
| Plume                    | feather design                 |
| Mandala                  | mandala design                 |
| Hirondelle               | swallow design                 |
| Tribal                   | tribal design                  |

## Style vestimentaire

| Valeur                                 | Prompt image                                   | Adulte |
| -------------------------------------- | ---------------------------------------------- | ------ |
| Décontracté (jean, sweat, baskets)     | casual style, jeans, sweatshirt, sneakers      |        |
| Bohème                                 | bohemian style                                 |        |
| Chic parisien                          | Parisian chic style                            |        |
| Business (tailleur, costume)           | business attire, tailored suit                 |        |
| Sportswear                             | sporty athleisure style                        |        |
| Rock (cuir, noir, bottines)            | rock style, black leather, ankle boots         |        |
| Streetwear                             | streetwear style                               |        |
| Vintage                                | vintage style                                  |        |
| Minimaliste                            | minimalist style                               |        |
| Romantique (robes fleuries)            | romantic style, floral dress                   |        |
| Gothique                               | gothic style                                   |        |
| Preppy                                 | preppy style                                   |        |
| Techwear                               | techwear style                                 |        |
| Cyberpunk                              | cyberpunk outfit with neon accents             |        |
| Glamour                                | glamorous style                                | oui    |
| Androgyne                              | androgynous style                              |        |
| Utilitaire (cargo, rangers)            | utility style, cargo pants, combat boots       |        |
| Uniforme scolaire japonais             | Japanese school uniform                        |        |
| Kimono                                 | traditional kimono                             |        |
| Yukata d'été                           | summer yukata                                  |        |
| Hanbok                                 | traditional Korean hanbok                      |        |
| Qipao                                  | qipao dress                                    | oui    |
| Tenue de samouraï                      | samurai attire with hakama                     |        |
| Tenue de ninja                         | ninja garb                                     |        |
| Armure médiévale                       | medieval plate armor                           |        |
| Armure lourde futuriste                | heavy futuristic power armor                   |        |
| Combinaison de pilote de mecha         | mecha pilot plugsuit                           |        |
| Uniforme militaire                     | military uniform                               |        |
| Tenue de mage                          | fantasy mage robes                             |        |
| Tenue d'aventurier                     | fantasy adventurer outfit with leather armor   |        |
| Tenue de soirée                        | elegant evening wear                           |        |
| Tenue de sport                         | sports outfit                                  |        |
| Pyjama                                 | pajamas                                        |        |

## Haut

| Valeur                  | Prompt image               | Remplace | Adulte |
| ----------------------- | -------------------------- | -------- | ------ |
| T-shirt                 | t-shirt                    |          |        |
| T-shirt oversize        | oversized t-shirt          |          |        |
| Chemise blanche         | white button-up shirt      |          |        |
| Chemisier en soie       | silk blouse                |          |        |
| Pull en maille          | knit sweater               |          |        |
| Sweat à capuche         | hoodie                     |          |        |
| Débardeur               | tank top                   |          |        |
| Crop top                | crop top                   |          | oui    |
| Top dos nu              | backless top               |          | oui    |
| Veste en cuir           | leather jacket             |          |        |
| Blouson bombardier      | bomber jacket              |          |        |
| Trench-coat             | trench coat                |          |        |
| Manteau long            | long coat                  |          |        |
| Robe d'été              | summer dress               | Bas      |        |
| Robe de soirée          | evening gown               | Bas      |        |
| Petite robe noire       | little black dress         | Bas      | oui    |
| Combinaison             | jumpsuit                   | Bas      |        |

## Bas

| Valeur              | Prompt image          | Adulte |
| ------------------- | --------------------- | ------ |
| Jean                | jeans                 |        |
| Jean déchiré        | ripped jeans          |        |
| Pantalon cargo      | cargo pants           |        |
| Pantalon de costume | tailored trousers     |        |
| Jupe plissée        | pleated skirt         |        |
| Jupe longue         | long flowing skirt    |        |
| Minijupe            | miniskirt             | oui    |
| Short               | shorts                |        |
| Legging             | leggings              |        |
| Jogging             | sweatpants            |        |

## Chaussures

| Valeur           | Prompt image          | Adulte |
| ---------------- | --------------------- | ------ |
| Baskets          | sneakers              |        |
| Bottines         | ankle boots           |        |
| Rangers          | combat boots          |        |
| Escarpins        | high heels            | oui    |
| Bottes hautes    | thigh-high boots      | oui    |
| Sandales         | sandals               |        |
| Geta             | wooden geta sandals   |        |
| Mocassins        | loafers               |        |
| Pieds nus        | barefoot              |        |

## Tenue intime

> Entièrement adulte : la sous-liste disparaît pour un personnage mineur.

| Valeur                   | Prompt image                 | Remplace     | Adulte |
| ------------------------ | ---------------------------- | ------------ | ------ |
| Lingerie en dentelle     | lace lingerie                | Haut, Bas    | oui    |
| Nuisette en soie         | silk nightgown               | Haut, Bas    | oui    |
| Peignoir                 | bathrobe                     | Haut, Bas    | oui    |
| Maillot de bain une pièce | one-piece swimsuit          | Haut, Bas    | oui    |
| Bikini                   | bikini                       | Haut, Bas    | oui    |
| Chemise d'homme trop grande | oversized men's shirt as a dress | Haut, Bas | oui |

## Accessoires

| Valeur                  | Prompt image                  |
| ----------------------- | ----------------------------- |
| Lunettes de soleil      | sunglasses                    |
| Casquette               | baseball cap                  |
| Chapeau de paille       | straw hat                     |
| Béret                   | beret                         |
| Écharpe                 | scarf                         |
| Collier fin             | delicate necklace             |
| Boucles d'oreilles      | earrings                      |
| Montre                  | wristwatch                    |
| Sac à dos               | backpack                      |
| Casque audio            | headphones around the neck    |
| Masque de kitsune       | kitsune fox mask              |
| Masque chirurgical      | face mask                     |
| Gants sans doigts       | fingerless gloves             |
| Parapluie               | umbrella                      |
| Katana                  | katana                        |
| Épée                    | sword                         |
| Bâton de mage           | magic staff                   |
| Appareil photo          | camera                        |
| Livre                   | book                          |
| Tasse de café           | cup of coffee                 |

## Maquillage

| Valeur               | Prompt image             | Adulte |
| -------------------- | ------------------------ | ------ |
| Aucun                | no makeup                |        |
| Naturel              | natural makeup           | oui    |
| Effet glass skin     | Korean glass skin makeup | oui    |
| Glossy               | dewy glossy makeup       | oui    |
| Eye-liner graphique  | winged eyeliner          | oui    |
| Smoky eyes           | smoky eye makeup         | oui    |
| Rouge à lèvres rouge | red lipstick             | oui    |
| Lèvres nude          | nude lips                | oui    |
| Gothique             | dark gothic makeup       | oui    |
| Pailleté             | glitter makeup           | oui    |
| Peintures de guerre  | war paint on the face    |        |

## Pose

> Une seule pose à la fois, rangée par catégorie dans le menu. Sexy et Intime sont réservées aux adultes : sensuel, jamais explicite.

| Valeur                                           | Prompt image                                           | Catégorie         | Adulte | Genre |
| ------------------------------------------------ | ------------------------------------------------------ | ----------------- | ------ | ----- |
| Debout                                           | standing                                               | Naturelle         |        |       |
| Debout en appui sur une jambe                    | standing relaxed with weight on one leg                | Naturelle         |        |       |
| Debout, mains jointes devant soi                 | standing with hands clasped in front                   | Naturelle         |        |       |
| Debout, mains derrière le dos                    | standing with hands behind the back                    | Naturelle         |        |       |
| Assis(e)                                         | sitting                                                | Naturelle         |        |       |
| Assis(e) en tailleur                             | sitting cross-legged                                   | Naturelle         |        |       |
| Assis(e) en seiza (à la japonaise)               | kneeling in seiza, sitting on the heels                | Naturelle         |        |       |
| Assis(e) sur un rebord de fenêtre                | sitting on a windowsill                                | Naturelle         |        |       |
| Assis(e) sur des marches                         | sitting on steps                                       | Naturelle         |        |       |
| Assis(e) sur un banc                             | sitting on a bench                                     | Naturelle         |        |       |
| Assis(e) par terre, dos au mur                   | sitting on the floor with the back against a wall      | Naturelle         |        |       |
| Assis(e), genoux ramenés contre soi              | sitting hugging the knees                              | Naturelle         |        |       |
| À califourchon sur une chaise retournée          | straddling a backwards chair                           | Naturelle         |        |       |
| Accroupi(e)                                      | crouching                                              | Naturelle         |        |       |
| Agenouillé(e)                                    | kneeling                                               | Naturelle         |        |       |
| Allongé(e) dans l'herbe                          | lying in the grass                                     | Naturelle         |        |       |
| Allongé(e) sur le dos à regarder le ciel         | lying on the back looking up at the sky                | Naturelle         |        |       |
| Appuyé(e) contre un mur                          | leaning against a wall                                 | Naturelle         |        |       |
| Adossé(e) à un arbre                             | leaning back against a tree                            | Naturelle         |        |       |
| Accoudé(e) à une rambarde                        | leaning on a railing                                   | Naturelle         |        |       |
| Accoudé(e) à un comptoir                         | leaning on a counter                                   | Naturelle         |        |       |
| Dos tourné, regard par-dessus l'épaule           | back turned, looking over the shoulder                 | Naturelle         |        |       |
| De profil, regard au loin                        | in profile, gazing into the distance                   | Naturelle         |        |       |
| Bras croisés                                     | arms crossed                                           | Naturelle         |        |       |
| Mains dans les poches                            | hands in pockets                                       | Naturelle         |        |       |
| Main sur la hanche                               | hand on hip                                            | Naturelle         |        |       |
| Menton dans la main                              | chin resting on one hand                               | Naturelle         |        |       |
| Mains derrière la tête                           | hands behind the head                                  | Naturelle         |        |       |
| Tête penchée sur le côté                         | head tilted to the side                                | Naturelle         |        |       |
| Recroquevillé(e)                                 | curled up                                              | Naturelle         |        |       |
| S'étire au réveil                                | stretching after waking up                             | Naturelle         |        |       |
| Remet une mèche derrière l'oreille               | tucking a strand of hair behind the ear                | Naturelle         |        |       |
| Ajuste ses lunettes                              | adjusting glasses                                      | Naturelle         |        |       |
| Salue de la main                                 | waving hello                                           | Naturelle         |        |       |
| Signe V (peace)                                  | making a peace sign                                    | Naturelle         |        |       |
| S'incline à la japonaise                         | bowing politely                                        | Naturelle         |        |       |
| Tient un parapluie                               | holding an umbrella                                    | Naturelle         |        |       |
| Main tendue vers la caméra                       | reaching a hand toward the camera                      | Naturelle         |        |       |
| Debout sous un réverbère                         | standing under a streetlight                           | Naturelle         |        |       |
| Assis(e) sur le capot d'une voiture              | sitting on the hood of a car                           | Naturelle         |        |       |
| Assis(e) sur un toit, jambes dans le vide        | sitting on a rooftop edge, legs dangling               | Naturelle         |        |       |
| Assis(e) sur une balançoire                      | sitting on a swing                                     | Naturelle         |        |       |
| Assis(e) au bord d'un quai, pieds dans l'eau     | sitting on a dock with feet in the water               | Naturelle         |        |       |
| Allongé(e) sur un tatami                         | lying on a tatami mat                                  | Naturelle         |        |       |
| Dos contre dos avec quelqu'un                    | sitting back to back with someone                      | Naturelle         |        |       |
| Main en visière contre le soleil                 | shading the eyes with one hand                         | Naturelle         |        |       |
| Mains autour d'une tasse chaude                  | holding a warm mug with both hands                     | Naturelle         |        |       |
| Tient sa veste par-dessus l'épaule               | holding a jacket over the shoulder                     | Naturelle         |        |       |
| Écharpe remontée sur le nez                      | scarf pulled up over the nose                          | Naturelle         |        |       |
| Capuche relevée                                  | hood up                                                | Naturelle         |        |       |
| Tient un bouquet de fleurs                       | holding a bouquet of flowers                           | Naturelle         |        |       |
| Les bras grands ouverts                          | arms spread wide open                                  | Naturelle         |        |       |
| Les yeux fermés, visage vers le ciel             | eyes closed, face turned up to the sky                 | Naturelle         |        |       |
| Assis(e) sur une valise                          | sitting on a suitcase                                  | Naturelle         |        |       |
| Assis(e) en travers d'un fauteuil                | sitting sideways in an armchair, legs over the armrest | Naturelle         |        |       |
| Debout dans l'encadrement d'une porte            | standing in a doorway                                  | Naturelle         |        |       |
| Penché(e) au-dessus d'un bureau                  | leaning over a desk                                    | Naturelle         |        |       |
| Assis(e) sur un muret                            | sitting on a low wall                                  | Naturelle         |        |       |
| Allongé(e) dans un hamac                         | lying in a hammock                                     | Naturelle         |        |       |
| Appuyé(e) contre une vitrine                     | leaning against a shop window                          | Naturelle         |        |       |
| Debout sous un arbre en fleurs                   | standing under a blossoming tree                       | Naturelle         |        |       |
| Tient un chat dans ses bras                      | holding a cat in the arms                              | Naturelle         |        |       |
| Une main contre une vitre embuée                 | one hand pressed against a fogged window               | Naturelle         |        |       |
| En marche                                        | walking                                                | Dynamique         |        |       |
| Marche d'un pas décidé                           | striding purposefully                                  | Dynamique         |        |       |
| En pleine course                                 | running                                                | Dynamique         |        |       |
| Sprint                                           | sprinting at full speed                                | Dynamique         |        |       |
| En plein saut                                    | mid-jump                                               | Dynamique         |        |       |
| Saut de parkour                                  | mid-air parkour vault                                  | Dynamique         |        |       |
| Course sur un mur                                | wall-running                                           | Dynamique         |        |       |
| Glissade sous un obstacle                        | sliding under an obstacle                              | Dynamique         |        |       |
| Saut dans le vide                                | leaping into the void                                  | Dynamique         |        |       |
| Escalade                                         | climbing                                               | Dynamique         |        |       |
| Coup de pied sauté                               | flying kick                                            | Dynamique         |        |       |
| Coup de poing                                    | throwing a punch                                       | Dynamique         |        |       |
| Garde de boxe                                    | boxing guard stance                                    | Dynamique         |        |       |
| Esquive                                          | dodging                                                | Dynamique         |        |       |
| Se retourne brusquement                          | turning around abruptly                                | Dynamique         |        |       |
| Tourbillonne sur soi-même                        | spinning around                                        | Dynamique         |        |       |
| Chute en arrière                                 | falling backwards                                      | Dynamique         |        |       |
| En suspension, cheveux au vent                   | suspended in mid-air, hair flowing in the wind         | Dynamique         |        |       |
| Glisse sur une rampe                             | sliding down a handrail                                | Dynamique         |        |       |
| Pose de combat                                   | dynamic fighting stance                                | Dynamique         |        |       |
| Pose héroïque                                    | heroic pose                                            | Dynamique         |        |       |
| Atterrissage de super-héros                      | superhero landing, one knee and one fist on the ground | Dynamique         |        |       |
| Tir en roulade                                   | diving roll while aiming                               | Dynamique         |        |       |
| Coup de sabre en mouvement                       | mid-swing sword slash                                  | Dynamique         |        |       |
| Bondit d'un toit à l'autre                       | leaping between rooftops                               | Dynamique         |        |       |
| Roulade de réception                             | landing in a forward roll                              | Dynamique         |        |       |
| Dérape en courant                                | skidding to a stop                                     | Dynamique         |        |       |
| Saute par-dessus une voiture                     | vaulting over a car                                    | Dynamique         |        |       |
| Grimpe à une échelle de secours                  | climbing a fire escape ladder                          | Dynamique         |        |       |
| Se suspend à une corniche                        | hanging from a ledge                                   | Dynamique         |        |       |
| Plonge dans l'eau                                | diving into water                                      | Dynamique         |        |       |
| Patine sur la glace                              | ice skating                                            | Dynamique         |        |       |
| Cheveux qui fouettent au vent                    | hair whipping in the wind                              | Dynamique         |        |       |
| Fend la foule en courant                         | running through a crowd                                | Dynamique         |        |       |
| Court sur une plage                              | running on a beach                                     | Dynamique         |        |       |
| Monte un escalier quatre à quatre                | rushing up the stairs                                  | Dynamique         |        |       |
| Saute dans une flaque                            | jumping into a puddle                                  | Dynamique         |        |       |
| Rattrape un objet au vol                         | catching an object in mid-air                          | Dynamique         |        |       |
| Lance un ballon                                  | throwing a ball                                        | Dynamique         |        |       |
| Pousse une porte d'un coup d'épaule              | shouldering a door open                                | Dynamique         |        |       |
| Se jette à couvert                               | diving for cover                                       | Dynamique         |        |       |
| Glisse sur un toit en pente                      | sliding down a sloped roof                             | Dynamique         |        |       |
| Pose de couverture de magazine                   | magazine cover pose                                    | Mode et éditorial |        |       |
| Mains sur le col de la veste                     | hands on the jacket collar                             | Mode et éditorial |        |       |
| Une main dans la poche, l'autre au visage        | one hand in a pocket, the other touching the face      | Mode et éditorial |        |       |
| Regard caméra, menton baissé                     | looking into the camera, chin lowered                  | Mode et éditorial |        |       |
| Silhouette à contre-jour                         | backlit silhouette                                     | Mode et éditorial |        |       |
| Assis(e) sur une chaise design à l'envers        | sitting backwards on a designer chair                  | Mode et éditorial |        |       |
| Défilé, pas croisé                               | runway walk, legs crossing                             | Mode et éditorial |        |       |
| Accroupi(e) style streetwear                     | streetwear squat pose                                  | Mode et éditorial |        |       |
| Gros plan, main sur la joue                      | close-up with a hand on the cheek                      | Mode et éditorial |        |       |
| Appuyé(e) contre une moto, l'air cool            | leaning coolly against a motorcycle                    | Mode et éditorial |        |       |
| Assis(e) au sol, jambes allongées                | sitting on the floor with legs stretched out           | Mode et éditorial |        |       |
| Regard par-dessus des lunettes                   | looking over the top of glasses                        | Mode et éditorial |        |       |
| Mains dans les cheveux, coudes levés             | hands in the hair, elbows raised                       | Mode et éditorial |        |       |
| Plan serré sur les mains et une bague            | close-up of hands wearing a ring                       | Mode et éditorial |        |       |
| Pose miroir, reflet visible                      | posing in front of a mirror, reflection visible        | Mode et éditorial |        |       |
| Assis(e) sur un escalier en colimaçon            | sitting on a spiral staircase                          | Mode et éditorial |        |       |
| Marche en manteau long dans le vent              | walking in a long coat blowing in the wind             | Mode et éditorial |        |       |
| Debout au milieu d'une rue vide                  | standing in the middle of an empty street              | Mode et éditorial |        |       |
| Signe V près de l'œil                            | peace sign next to the eye                             | Kawaii et anime   |        |       |
| Mains en cœur                                    | making a heart with the hands                          | Kawaii et anime   |        |       |
| Pose « nyan », poings de chat                    | cat pose with paw-like fists                           | Kawaii et anime   |        |       |
| Clin d'œil, langue tirée                         | winking with the tongue out                            | Kawaii et anime   |        |       |
| Joues gonflées, boudeur(se)                      | puffed cheeks, pouting                                 | Kawaii et anime   |        |       |
| Saut de joie, bras en l'air                      | jumping for joy, arms raised                           | Kawaii et anime   |        |       |
| Mains jointes, yeux brillants                    | hands clasped, sparkling eyes                          | Kawaii et anime   |        |       |
| Tête penchée, doigt sur la joue                  | head tilted, finger on the cheek                       | Kawaii et anime   |        |       |
| En retard, toast à la bouche                     | running late with a slice of toast in the mouth        | Kawaii et anime   |        |       |
| Pose de magical girl                             | magical girl transformation pose                       | Kawaii et anime   |        |       |
| Mains sur les joues, émerveillé(e)               | hands on the cheeks, amazed                            | Kawaii et anime   |        |       |
| Révérence en tenue de maid                       | curtsying in a maid outfit                             | Kawaii et anime   |        |       |
| Tient une peluche contre soi                     | hugging a plush toy                                    | Kawaii et anime   |        |       |
| Pose d'idole, micro à la main                    | idol pose holding a microphone                         | Kawaii et anime   |        |       |
| Bras tendus, demande un câlin                    | arms outstretched asking for a hug                     | Kawaii et anime   |        |       |
| Se cache derrière un éventail                    | hiding behind a folding fan                            | Kawaii et anime   |        |       |
| Petit saut sur un pied                           | hopping on one foot                                    | Kawaii et anime   |        |       |
| Tire sur ses joues (grimace)                     | pulling the cheeks, making a funny face                | Kawaii et anime   |        |       |
| Garde de karaté                                  | karate fighting stance                                 | Martiale          |        |       |
| Posture du cavalier (kung-fu)                    | kung fu horse stance                                   | Martiale          |        |       |
| Iaido, main sur la poignée                       | iaido stance, hand on the sword hilt                   | Martiale          |        |       |
| Salut martial, poing dans la paume               | martial arts salute, fist in palm                      | Martiale          |        |       |
| Kata en mouvement                                | performing a martial arts kata                         | Martiale          |        |       |
| Position du lotus, en méditation                 | meditating in the lotus position                       | Martiale          |        |       |
| Posture de la grue                               | crane stance, balancing on one leg                     | Martiale          |        |       |
| Garde de boxe thaï                               | muay thai guard                                        | Martiale          |        |       |
| Posture d'archer                                 | archer's stance drawing a bow                          | Martiale          |        |       |
| Fente avant, poing tendu                         | lunging forward with a punch                           | Martiale          |        |       |
| Pose de la vipère (wing chun)                    | wing chun stance                                       | Martiale          |        |       |
| Bâton bō en garde                                | holding a bo staff in guard                            | Martiale          |        |       |
| Salut au dojo                                    | bowing in a dojo                                       | Martiale          |        |       |
| Coup de pied circulaire figé                     | frozen roundhouse kick                                 | Martiale          |        |       |
| Pose sensuelle                                   | sensual pose                                           | Sexy              | oui    |       |
| Déhanchée                                        | hip cocked to one side, strong contrapposto            | Sexy              | oui    |       |
| Cambrée                                          | arched back                                            | Sexy              | oui    |       |
| Main dans les cheveux                            | running a hand through the hair                        | Sexy              | oui    |       |
| Les deux mains dans les cheveux                  | both hands in the hair, arms raised                    | Sexy              | oui    |       |
| S'étire langoureusement                          | stretching languidly                                   | Sexy              | oui    |       |
| Assise jambes croisées haut                      | sitting with legs crossed high                         | Sexy              | oui    |       |
| Penchée vers la caméra                           | leaning toward the camera                              | Sexy              | oui    |       |
| Allongée sur le côté, sur un coude               | lying on the side propped on one elbow                 | Sexy              | oui    |       |
| Regard par-dessus l'épaule, dos cambré           | looking over the shoulder with an arched back          | Sexy              | oui    |       |
| Assise sur le bord d'une table                   | sitting on the edge of a table                         | Sexy              | oui    |       |
| Assise sur un tabouret de bar                    | perched on a bar stool                                 | Sexy              | oui    |       |
| Agenouillée sur le lit                           | kneeling on a bed                                      | Sexy              | oui    |       |
| Allongée sur un canapé                           | lounging on a sofa                                     | Sexy              | oui    |       |
| Appuyée à l'encadrement d'une porte              | leaning in a doorframe                                 | Sexy              | oui    |       |
| Adossée au mur, une jambe repliée                | leaning against a wall with one leg bent               | Sexy              | oui    |       |
| Marche féline vers la caméra                     | walking toward the camera with a feline stride         | Sexy              | oui    |       |
| Mordille sa lèvre                                | biting the lower lip                                   | Sexy              | oui    |       |
| Doigt sur les lèvres                             | finger on the lips                                     | Sexy              | oui    |       |
| Joue avec une mèche                              | twirling a strand of hair                              | Sexy              | oui    |       |
| Remonte une bretelle                             | adjusting a shoulder strap                             | Sexy              | oui    | femme |
| Ajuste un bas                                    | adjusting a stocking                                   | Sexy              | oui    | femme |
| Pose de pin-up                                   | classic pin-up pose                                    | Sexy              | oui    | femme |
| Regard en coin derrière des lunettes de soleil   | sideways glance over sunglasses                        | Sexy              | oui    |       |
| Allongée sur le capot d'une voiture              | lying across the hood of a car                         | Sexy              | oui    |       |
| Allongée dans les draps, regard caméra           | lying in the sheets, looking at the camera             | Sexy              | oui    |       |
| Assise sur le rebord d'une baignoire             | sitting on the edge of a bathtub                       | Sexy              | oui    |       |
| Robe qui glisse d'une épaule                     | dress slipping off one shoulder                        | Sexy              | oui    | femme |
| Retire ses gants doigt par doigt                 | pulling off gloves finger by finger                    | Sexy              | oui    |       |
| Ajuste son porte-jarretelles                     | adjusting a garter belt                                | Sexy              | oui    | femme |
| Allongée sur un piano                            | lying on a grand piano                                 | Sexy              | oui    |       |
| Silhouette derrière un rideau translucide        | silhouette behind a sheer curtain                      | Sexy              | oui    |       |
| Sous la pluie, chemise trempée                   | in the rain, shirt soaked                              | Sexy              | oui    |       |
| Appuyée sur un comptoir de bar, cambrée          | leaning on a bar counter, back arched                  | Sexy              | oui    |       |
| Assise à califourchon sur une chaise             | straddling a chair                                     | Sexy              | oui    |       |
| Lunettes de soleil remontées dans les cheveux    | sunglasses pushed up into the hair                     | Sexy              | oui    |       |
| Allongée sur une chaise longue                   | lounging on a deck chair                               | Sexy              | oui    |       |
| Debout dans l'eau jusqu'à la taille              | standing waist-deep in water                           | Sexy              | oui    |       |
| Assise sur une moto, cambrée                     | sitting on a motorcycle, back arched                   | Sexy              | oui    |       |
| Appuyée à un mur de briques, regard intense      | leaning against a brick wall, intense gaze             | Sexy              | oui    |       |
| Glisse les doigts le long de son cou             | trailing fingers along the neck                        | Sexy              | oui    |       |
| Pose devant des stores vénitiens                 | posing by venetian blinds, striped light               | Sexy              | oui    |       |
| Penchée sur une table de billard                 | leaning over a pool table                              | Sexy              | oui    |       |
| Assise dans l'escalier, une jambe tendue         | sitting on stairs with one leg extended                | Sexy              | oui    |       |
| Allongé(e) sur un lit                            | lying on a bed                                         | Intime            | oui    |       |
| Sur le ventre, jambes relevées                   | lying on the stomach, legs kicked up                   | Intime            | oui    |       |
| Allongé(e) sur le dos, bras au-dessus de la tête | lying on the back, arms above the head                 | Intime            | oui    |       |
| Allongé(e) sur des draps de soie                 | lying on silk sheets                                   | Intime            | oui    |       |
| Enveloppé(e) dans un drap                        | wrapped in a bedsheet                                  | Intime            | oui    |       |
| Au réveil, cheveux en bataille                   | just woken up in bed, messy hair                       | Intime            | oui    |       |
| Dans une chemise trop grande au petit matin      | wearing an oversized shirt in the early morning        | Intime            | oui    |       |
| Au bord du lit, enfile ses bas                   | sitting on the edge of the bed, putting on stockings   | Intime            | oui    | femme |
| Assis(e) sur le lit, genoux repliés              | sitting on the bed with knees drawn up                 | Intime            | oui    |       |
| Lit au lit sous la couette                       | reading in bed under the duvet                         | Intime            | oui    |       |
| Dans un bain moussant                            | relaxing in a bubble bath                              | Intime            | oui    |       |
| Dans un onsen                                    | relaxing in a Japanese hot spring                      | Intime            | oui    |       |
| Sous la douche, dos tourné                       | in the shower, back to the camera                      | Intime            | oui    |       |
| En peignoir sur le balcon                        | in a bathrobe on a balcony                             | Intime            | oui    |       |
| Se coiffe devant un miroir                       | brushing hair in front of a mirror                     | Intime            | oui    |       |
| Blotti(e) sous un plaid devant la cheminée       | curled up under a blanket by the fireplace             | Intime            | oui    |       |
| Enlacé(e) tendrement avec quelqu'un              | tenderly embracing someone                             | Intime            | oui    |       |
| Blotti(e) contre quelqu'un                       | snuggled up against someone                            | Intime            | oui    |       |
| Endormi(e) dans les bras de quelqu'un            | asleep in someone's arms                               | Intime            | oui    |       |
| Front contre front                               | forehead to forehead with someone                      | Intime            | oui    |       |
| Main dans la main                                | holding hands with someone                             | Intime            | oui    |       |
| Baiser tendre                                    | sharing a tender kiss                                  | Intime            | oui    |       |
| Petit-déjeuner au lit                            | having breakfast in bed                                | Intime            | oui    |       |
| Tête posée sur une épaule                        | head resting on someone's shoulder                     | Intime            | oui    |       |
| Contre quelqu'un, écoute son cœur                | lying against someone, listening to their heartbeat    | Intime            | oui    |       |
| Danse lente enlacée                              | slow dancing in an embrace                             | Intime            | oui    |       |
| Main posée sur une joue                          | hand gently cupping someone's cheek                    | Intime            | oui    |       |
| Dos nu devant une fenêtre à l'aube               | bare back facing a window at dawn                      | Intime            | oui    |       |
| Bain à deux, vue de dos                          | sharing a bath, seen from behind                       | Intime            | oui    |       |
| Massage des épaules                              | giving a shoulder massage                              | Intime            | oui    |       |
| Cheveux mouillés dans une serviette              | wet hair wrapped in a towel                            | Intime            | oui    |       |
| Regard complice sur l'oreiller                   | knowing look on the pillow                             | Intime            | oui    |       |
| Lit à voix haute à quelqu'un                     | reading aloud to someone                               | Intime            | oui    |       |
| S'endort sur un canapé contre quelqu'un          | falling asleep on a sofa against someone               | Intime            | oui    |       |
| Regarde la pluie à deux, sous un plaid           | watching the rain together under a blanket             | Intime            | oui    |       |
| Se brosse les dents côte à côte                  | brushing teeth side by side                            | Intime            | oui    |       |
| Câlin par-derrière                               | back hug                                               | Intime            | oui    |       |
| Dans le reflet d'un miroir embué                 | reflected in a fogged-up mirror                        | Intime            | oui    |       |
| Joue avec les cheveux de quelqu'un               | playing with someone's hair                            | Intime            | oui    |       |
| Allongé(e) dans l'herbe, main dans la main       | lying in the grass holding hands                       | Intime            | oui    |       |

## Action

> Ce que fait le personnage ; se combine avec une pose. « Remplace : Pose » = l'action impose déjà la posture (conduire, nager, dormir…).

| Valeur                                            | Prompt image                                          | Catégorie             | Remplace | Adulte |
| ------------------------------------------------- | ----------------------------------------------------- | --------------------- | -------- | ------ |
| Lit un livre                                      | reading a book                                        | Quotidien             |          |        |
| Lit un manga                                      | reading a manga                                       | Quotidien             |          |        |
| Boit un café                                      | drinking a coffee                                     | Quotidien             |          |        |
| Boit un thé                                       | sipping tea                                           | Quotidien             |          |        |
| Boit un bubble tea                                | drinking bubble tea                                   | Quotidien             |          |        |
| Mange des ramens                                  | eating ramen                                          | Quotidien             |          |        |
| Mange des sushis avec des baguettes               | eating sushi with chopsticks                          | Quotidien             |          |        |
| Cuisine                                           | cooking                                               | Quotidien             |          |        |
| Écrit dans un carnet                              | writing in a notebook                                 | Quotidien             |          |        |
| Tape sur un ordinateur portable                   | typing on a laptop                                    | Quotidien             |          |        |
| Se maquille                                       | applying makeup                                       | Quotidien             |          |        |
| Se brosse les cheveux                             | brushing hair                                         | Quotidien             |          |        |
| Parle au téléphone                                | talking on the phone                                  | Quotidien             |          |        |
| Fait un selfie                                    | taking a selfie                                       | Quotidien             |          |        |
| Fait ses courses                                  | grocery shopping                                      | Quotidien             |          |        |
| Arrose des plantes                                | watering plants                                       | Quotidien             |          |        |
| Promène son chien                                 | walking a dog                                         | Quotidien             |          |        |
| Caresse un chat                                   | petting a cat                                         | Quotidien             |          |        |
| Ouvre une lettre                                  | opening a letter                                      | Quotidien             |          |        |
| Fait les cent pas                                 | pacing back and forth                                 | Quotidien             |          |        |
| Fait sa valise                                    | packing a suitcase                                    | Quotidien             |          |        |
| Range une bibliothèque                            | arranging books on a shelf                            | Quotidien             |          |        |
| Repasse une chemise                               | ironing a shirt                                       | Quotidien             |          |        |
| Fait du pain                                      | baking bread                                          | Quotidien             |          |        |
| Mange une glace                                   | eating an ice cream                                   | Quotidien             |          |        |
| Partage des dangos                                | sharing dango skewers                                 | Quotidien             |          |        |
| Dresse une table                                  | setting a table                                       | Quotidien             |          |        |
| Lave sa voiture                                   | washing a car                                         | Quotidien             |          |        |
| Fait un smoothie                                  | making a smoothie                                     | Quotidien             |          |        |
| Choisit une tenue devant son armoire              | choosing an outfit in front of a wardrobe             | Quotidien             |          |        |
| Fait une liste de courses                         | writing a shopping list                               | Quotidien             |          |        |
| Se fait un masque de beauté                       | applying a face mask                                  | Quotidien             |          |        |
| Mange un onigiri                                  | eating an onigiri                                     | Quotidien             |          |        |
| Porte des sacs de shopping                        | carrying shopping bags                                | Quotidien             |          |        |
| Allume des bougies                                | lighting candles                                      | Quotidien             |          |        |
| Prend un bain de pieds                            | soaking the feet in warm water                        | Quotidien             |          |        |
| Joue de la guitare                                | playing the guitar                                    | Loisirs               |          |        |
| Joue du piano                                     | playing the piano                                     | Loisirs               |          |        |
| Joue du violon                                    | playing the violin                                    | Loisirs               |          |        |
| Chante au micro                                   | singing into a microphone                             | Loisirs               |          |        |
| Mixe en DJ                                        | DJing at turntables                                   | Loisirs               |          |        |
| Dessine                                           | drawing in a sketchbook                               | Loisirs               |          |        |
| Peint sur une toile                               | painting on a canvas                                  | Loisirs               |          |        |
| Calligraphie japonaise                            | doing Japanese calligraphy                            | Loisirs               |          |        |
| Plie un origami                                   | folding origami                                       | Loisirs               |          |        |
| Prend une photo                                   | taking a photo                                        | Loisirs               |          |        |
| Écoute de la musique au casque                    | listening to music with headphones                    | Loisirs               |          |        |
| Joue aux jeux vidéo                               | playing video games                                   | Loisirs               |          |        |
| Joue sur une borne d'arcade                       | playing an arcade machine                             | Loisirs               |          |        |
| Peint une figurine                                | painting a miniature figurine                         | Loisirs               |          |        |
| Surveille une impression 3D résine                | watching a resin 3D printer at work                   | Loisirs               |          |        |
| Monte un PC                                       | building a gaming PC                                  | Loisirs               |          |        |
| Joue aux échecs                                   | playing chess                                         | Loisirs               |          |        |
| Joue au go                                        | playing go                                            | Loisirs               |          |        |
| Tire les cartes de tarot                          | reading tarot cards                                   | Loisirs               |          |        |
| Jardine                                           | gardening                                             | Loisirs               |          |        |
| Pêche                                             | fishing                                               | Loisirs               |          |        |
| Fait un cosplay                                   | in an elaborate cosplay costume                       | Loisirs               |          |        |
| Feuillette des vinyles                            | browsing vinyl records                                | Loisirs               |          |        |
| Fait de la poterie                                | shaping pottery on a wheel                            | Loisirs               |          |        |
| Tricote                                           | knitting                                              | Loisirs               |          |        |
| Fait voler un cerf-volant                         | flying a kite                                         | Loisirs               |          |        |
| Observe au télescope                              | looking through a telescope                           | Loisirs               |          |        |
| Fait un karaoké                                   | singing karaoke                                       | Loisirs               |          |        |
| Joue au billard                                   | playing pool                                          | Loisirs               |          |        |
| Assemble un Gunpla                                | assembling a Gunpla model kit                         | Loisirs               |          |        |
| Lance des dés de jeu de rôle                      | rolling tabletop RPG dice                             | Loisirs               |          |        |
| Fait du roller-derby                              | playing roller derby                                  | Loisirs               | Pose     |        |
| Joue aux cartes                                   | playing cards                                         | Loisirs               |          |        |
| Fait un puzzle géant                              | doing a giant jigsaw puzzle                           | Loisirs               |          |        |
| Range sa collection de cartes                     | sorting a trading card collection                     | Loisirs               |          |        |
| Fait de la photo argentique                       | shooting with a film camera                           | Loisirs               |          |        |
| Lit des bandes dessinées                          | reading comic books                                   | Loisirs               |          |        |
| Regarde un film avec du popcorn                   | watching a movie with popcorn                         | Loisirs               |          |        |
| Peint un mur au pochoir                           | stenciling a mural                                    | Loisirs               |          |        |
| Conduit une voiture                               | driving a car                                         | Déplacement           | Pose     |        |
| Conduit une voiture de sport la nuit              | driving a sports car at night                         | Déplacement           | Pose     |        |
| Fait du vélo                                      | riding a bicycle                                      | Déplacement           | Pose     |        |
| Fait de la moto                                   | riding a motorcycle                                   | Déplacement           | Pose     |        |
| Fait du skate                                     | skateboarding                                         | Déplacement           | Pose     |        |
| Fait du roller                                    | rollerblading                                         | Déplacement           | Pose     |        |
| Monte à cheval                                    | horseback riding                                      | Déplacement           | Pose     |        |
| Attend sous un abribus                            | waiting at a bus stop                                 | Déplacement           |          |        |
| Debout dans le métro, tient la barre              | standing in the subway holding the handrail           | Déplacement           | Pose     |        |
| Regarde par la fenêtre du shinkansen              | looking out of a shinkansen window                    | Déplacement           | Pose     |        |
| Monte un escalator                                | riding an escalator                                   | Déplacement           |          |        |
| Traverse le carrefour de Shibuya                  | crossing the Shibuya scramble crossing                | Déplacement           |          |        |
| Hèle un taxi                                      | hailing a taxi                                        | Déplacement           |          |        |
| Fait du jet-ski                                   | riding a jet ski                                      | Déplacement           | Pose     |        |
| Navigue en voilier                                | sailing a sailboat                                    | Déplacement           |          |        |
| Pagaie en kayak                                   | paddling a kayak                                      | Déplacement           | Pose     |        |
| Fait de la trottinette                            | riding a kick scooter                                 | Déplacement           | Pose     |        |
| Tire une valise dans un aéroport                  | pulling a suitcase through an airport                 | Déplacement           |          |        |
| Prend un téléphérique                             | riding a cable car                                    | Déplacement           |          |        |
| Conduit un tuk-tuk                                | driving a tuk-tuk                                     | Déplacement           | Pose     |        |
| Court sous la pluie                               | running in the rain                                   | Sport                 |          |        |
| Danse                                             | dancing                                               | Sport                 |          |        |
| Danse classique                                   | ballet dancing                                        | Sport                 |          |        |
| Danse hip-hop                                     | hip-hop dancing                                       | Sport                 |          |        |
| Fait du yoga                                      | doing yoga                                            | Sport                 |          |        |
| S'étire                                           | stretching                                            | Sport                 |          |        |
| Boxe                                              | boxing                                                | Sport                 |          |        |
| Frappe dans un sac                                | hitting a punching bag                                | Sport                 |          |        |
| Nage                                              | swimming                                              | Sport                 | Pose     |        |
| Surfe                                             | surfing a wave                                        | Sport                 | Pose     |        |
| Grimpe                                            | rock climbing                                         | Sport                 |          |        |
| Tire à l'arc (kyudo)                              | practicing kyudo archery                              | Sport                 |          |        |
| S'entraîne au sabre (kendo)                       | practicing kendo                                      | Sport                 |          |        |
| Soulève des haltères                              | lifting dumbbells                                     | Sport                 |          |        |
| Fait des pompes                                   | doing push-ups                                        | Sport                 | Pose     |        |
| Fait du snowboard                                 | snowboarding                                          | Sport                 | Pose     |        |
| Fait du parkour                                   | doing parkour                                         | Sport                 |          |        |
| Joue au tennis                                    | playing tennis                                        | Sport                 |          |        |
| Joue au basket                                    | playing basketball                                    | Sport                 |          |        |
| Fait de la gymnastique                            | doing gymnastics                                      | Sport                 |          |        |
| Fait de l'escrime                                 | fencing                                               | Sport                 |          |        |
| Patine                                            | ice skating                                           | Sport                 | Pose     |        |
| Fait du kitesurf                                  | kitesurfing                                           | Sport                 | Pose     |        |
| Saute à la corde                                  | jumping rope                                          | Sport                 |          |        |
| Rit avec quelqu'un                                | laughing with someone                                 | Moments               |          |        |
| Regarde l'horizon                                 | gazing at the horizon                                 | Moments               |          |        |
| Contemple la pluie à la fenêtre                   | watching the rain through a window                    | Moments               |          |        |
| Regarde les étoiles                               | stargazing                                            | Moments               |          |        |
| Regarde un feu d'artifice                         | watching fireworks                                    | Moments               |          |        |
| Lâche une lanterne céleste                        | releasing a sky lantern                               | Moments               |          |        |
| Admire les cerisiers en fleurs                    | admiring cherry blossoms                              | Moments               |          |        |
| Attrape des flocons de neige                      | catching snowflakes                                   | Moments               |          |        |
| Souffle sur un pissenlit                          | blowing on a dandelion                                | Moments               |          |        |
| Souffle les bougies d'un gâteau                   | blowing out birthday candles                          | Moments               |          |        |
| Accroche un vœu (Tanabata)                        | hanging a Tanabata wish                               | Moments               |          |        |
| Fait un câlin à quelqu'un                         | hugging someone                                       | Moments               |          |        |
| Rêvasse                                           | daydreaming                                           | Moments               |          |        |
| Fait une sieste                                   | taking a nap                                          | Moments               | Pose     |        |
| Dort                                              | sleeping                                              | Moments               | Pose     |        |
| Pleure de joie                                    | crying tears of joy                                   | Moments               |          |        |
| Lit une lettre d'amour                            | reading a love letter                                 | Moments               |          |        |
| Regarde une vieille photo                         | looking at an old photograph                          | Moments               |          |        |
| Danse seul(e) dans sa cuisine                     | dancing alone in the kitchen                          | Moments               |          |        |
| Chante sous la douche                             | singing in the shower                                 | Moments               |          | oui    |
| Fête une victoire                                 | celebrating a victory                                 | Moments               |          |        |
| Attend quelqu'un sur un quai de gare              | waiting for someone on a train platform               | Moments               |          |        |
| Dit adieu par la fenêtre d'un train               | waving goodbye from a train window                    | Moments               |          |        |
| Regarde le lever du soleil                        | watching the sunrise                                  | Moments               |          |        |
| Fait un vœu devant une étoile filante             | making a wish on a shooting star                      | Moments               |          |        |
| Lâche un ballon                                   | letting go of a balloon                               | Moments               |          |        |
| Rentre trempé(e) d'une averse                     | coming home soaked from a downpour                    | Moments               |          |        |
| Ferme les yeux sous la neige                      | eyes closed in the falling snow                       | Moments               |          |        |
| Lit sous une lampe la nuit                        | reading under a lamp at night                         | Moments               |          |        |
| Rit aux larmes                                    | laughing to tears                                     | Moments               |          |        |
| Contemple un aquarium                             | gazing into an aquarium                               | Moments               |          |        |
| Dégaine un katana                                 | drawing a katana                                      | Combat et SF          |          |        |
| Combat à l'épée                                   | fighting with a sword                                 | Combat et SF          |          |        |
| Combat à deux sabres                              | dual-wielding swords                                  | Combat et SF          |          |        |
| Pare un coup                                      | parrying a blow                                       | Combat et SF          |          |        |
| Bande un arc                                      | drawing a bow                                         | Combat et SF          |          |        |
| Manie une lance                                   | wielding a spear                                      | Combat et SF          |          |        |
| Médite avant le combat                            | meditating before battle                              | Combat et SF          |          |        |
| Lance un sort                                     | casting a glowing spell                               | Combat et SF          |          |        |
| Invoque une créature                              | summoning a creature                                  | Combat et SF          |          |        |
| Tire au pistolet                                  | aiming a pistol                                       | Combat et SF          |          |        |
| Tire au fusil de précision                        | aiming a sniper rifle                                 | Combat et SF          |          |        |
| Recharge son arme                                 | reloading a weapon                                    | Combat et SF          |          |        |
| Tient un bouclier énergétique                     | holding an energy shield                              | Combat et SF          |          |        |
| Pilote un mecha                                   | piloting a mecha in the cockpit                       | Combat et SF          | Pose     |        |
| Pilote un vaisseau                                | piloting a spaceship                                  | Combat et SF          | Pose     |        |
| Pirate un terminal holographique                  | hacking a holographic terminal                        | Combat et SF          |          |        |
| Consulte une carte holographique                  | studying a holographic map                            | Combat et SF          |          |        |
| Active un implant cybernétique                    | activating a cybernetic implant                       | Combat et SF          |          |        |
| Répare une machine                                | repairing a machine                                   | Combat et SF          |          |        |
| Soude une pièce d'armure                          | welding a piece of armor                              | Combat et SF          |          |        |
| Dévie un tir au sabre laser                       | deflecting a blast with a laser sword                 | Combat et SF          |          |        |
| Combat en apesanteur                              | fighting in zero gravity                              | Combat et SF          |          |        |
| Pilote un speeder                                 | riding a hover speeder                                | Combat et SF          | Pose     |        |
| Se téléporte                                      | teleporting in a burst of light                       | Combat et SF          |          |        |
| Contrôle un essaim de drones                      | controlling a swarm of drones                         | Combat et SF          |          |        |
| Combat un kaiju                                   | facing a giant kaiju                                  | Combat et SF          |          |        |
| Fait feu depuis une tourelle                      | firing from a gun turret                              | Combat et SF          | Pose     |        |
| Active une armure énergétique                     | powering up an energy armor                           | Combat et SF          |          |        |
| Câble une baie de serveurs                        | neatly cabling a server rack                          | Travail               |          |        |
| Tape au clavier en salle serveur                  | typing on a keyboard in a server room                 | Travail               |          |        |
| Bricole sous une voiture au garage                | working under a car in a garage                       | Travail               | Pose     |        |
| Pilote un drone                                   | piloting a drone with a remote controller             | Travail               |          |        |
| Travaille en laboratoire                          | working in a laboratory                               | Travail               |          |        |
| Enseigne au tableau                               | teaching at a blackboard                              | Travail               |          |        |
| Sert au comptoir d'un café                        | serving at a café counter                             | Travail               |          |        |
| Fait une présentation sur grand écran             | giving a presentation on a big screen                 | Travail               |          |        |
| Code dans une salle obscure                       | coding in a dark room lit by screens                  | Travail               |          |        |
| Répare un réseau en haut d'un poteau              | repairing network cables at the top of a utility pole | Travail               | Pose     |        |
| Analyse des logs sur plusieurs écrans             | analyzing logs on multiple monitors                   | Travail               |          |        |
| Fait de la soudure                                | welding                                               | Travail               |          |        |
| Imprime une pièce en 3D au labo                   | 3D printing a part in a lab                           | Travail               |          |        |
| Prépare des cafés en barista                      | making coffee as a barista                            | Travail               |          |        |
| Accorde un piano                                  | tuning a piano                                        | Travail               |          |        |
| Cérémonie du thé                                  | performing a tea ceremony                             | Japon traditionnel    |          |        |
| Tire un omikuji au temple                         | drawing an omikuji fortune at a shrine                | Japon traditionnel    |          |        |
| Fait sonner la cloche d'un sanctuaire             | ringing a shrine bell                                 | Japon traditionnel    |          |        |
| Danse en yukata au matsuri                        | dancing in a yukata at a summer festival              | Japon traditionnel    |          |        |
| Pêche des poissons rouges au festival             | scooping goldfish at a festival stall                 | Japon traditionnel    |          |        |
| Joue du shamisen                                  | playing the shamisen                                  | Japon traditionnel    |          |        |
| Joue du taiko                                     | playing taiko drums                                   | Japon traditionnel    |          |        |
| Arrange un ikebana                                | arranging ikebana flowers                             | Japon traditionnel    |          |        |
| Prie devant un torii                              | praying in front of a torii gate                      | Japon traditionnel    |          |        |
| Marche sous les lanternes de Kyoto                | walking under lanterns in Kyoto                       | Japon traditionnel    |          |        |
| Peint une estampe                                 | painting a ukiyo-e print                              | Japon traditionnel    |          |        |
| Fait des mochis                                   | making mochi                                          | Japon traditionnel    |          |        |
| Allume une lanterne en papier                     | lighting a paper lantern                              | Japon traditionnel    |          |        |
| Marche sur un pont rouge                          | crossing a red arched bridge                          | Japon traditionnel    |          |        |
| Joue au shogi                                     | playing shogi                                         | Japon traditionnel    |          |        |
| Écrit un haïku au pinceau                         | writing a haiku with a brush                          | Japon traditionnel    |          |        |
| Danse du Bon (Obon)                               | dancing the Bon Odori                                 | Japon traditionnel    |          |        |
| Se branche au réseau par la nuque                 | jacking into the network with a cable at the neck     | Cyberpunk et nuit     |          |        |
| Boit un verre dans un bar néon                    | having a drink in a neon bar                          | Cyberpunk et nuit     |          |        |
| Mange des nouilles dans une échoppe sous la pluie | eating noodles at a street stall in the rain          | Cyberpunk et nuit     |          |        |
| Scanne un code au poignet                         | scanning a code on a wrist display                    | Cyberpunk et nuit     |          |        |
| Fuit des drones de sécurité                       | fleeing from security drones                          | Cyberpunk et nuit     |          |        |
| Négocie dans une ruelle                           | negotiating in a back alley                           | Cyberpunk et nuit     |          |        |
| Répare son bras cybernétique                      | repairing a cybernetic arm                            | Cyberpunk et nuit     |          |        |
| Pilote une moto sous les néons                    | riding a motorcycle under neon lights                 | Cyberpunk et nuit     | Pose     |        |
| Contemple la ville depuis un toit                 | overlooking the city from a rooftop                   | Cyberpunk et nuit     |          |        |
| Tague un mur                                      | spray-painting graffiti                               | Cyberpunk et nuit     |          |        |
| Achète au marché noir                             | buying at a black market                              | Cyberpunk et nuit     |          |        |
| Plonge dans le cyberespace                        | diving into cyberspace with a neural headset          | Cyberpunk et nuit     |          |        |
| Fuit sur des toits mouillés                       | escaping across wet rooftops                          | Cyberpunk et nuit     |          |        |
| Reçoit un appel holographique                     | taking a holographic call                             | Cyberpunk et nuit     |          |        |
| Débarque d'un aéro-taxi                           | stepping out of a flying taxi                         | Cyberpunk et nuit     |          |        |
| Joue dans un club clandestin                      | performing in an underground club                     | Cyberpunk et nuit     |          |        |
| Efface ses traces numériques                      | erasing digital traces on a terminal                  | Cyberpunk et nuit     |          |        |
| En position de tir couché                         | in a prone shooting position                          | Militaire et tactique | Pose     |        |
| Vérifie son équipement                            | checking tactical gear                                | Militaire et tactique |          |        |
| Communique à la radio                             | talking on a military radio                           | Militaire et tactique |          |        |
| Observe aux jumelles                              | looking through binoculars                            | Militaire et tactique |          |        |
| Lit une carte topographique                       | reading a topographic map                             | Militaire et tactique |          |        |
| Progresse en colonne                              | advancing in a tactical column                        | Militaire et tactique |          |        |
| Monte la garde                                    | standing guard                                        | Militaire et tactique |          |        |
| Saute en parachute                                | skydiving with a parachute                            | Militaire et tactique | Pose     |        |
| Pilote un hélicoptère                             | piloting a helicopter                                 | Militaire et tactique | Pose     |        |
| Démine avec précaution                            | carefully defusing a mine                             | Militaire et tactique |          |        |
| Débriefe autour d'une table                       | debriefing around a map table                         | Militaire et tactique |          |        |
| Se camoufle le visage                             | applying face camouflage paint                        | Militaire et tactique |          |        |
| Nettoie son arme                                  | cleaning a rifle                                      | Militaire et tactique |          |        |
| Entre en formation dans un bâtiment               | breaching a building in formation                     | Militaire et tactique |          |        |
| Signale à la main                                 | giving tactical hand signals                          | Militaire et tactique |          |        |
| Monte dans un blindé                              | climbing into an armored vehicle                      | Militaire et tactique |          |        |
| Établit une liaison satellite                     | setting up a satellite uplink                         | Militaire et tactique |          |        |
| Allume un feu de camp                             | lighting a campfire                                   | Survie et aventure    |          |        |
| Monte une tente                                   | pitching a tent                                       | Survie et aventure    |          |        |
| Filtre de l'eau                                   | filtering water                                       | Survie et aventure    |          |        |
| Fend du bois à la hache                           | chopping wood with an axe                             | Survie et aventure    |          |        |
| Lit une boussole                                  | reading a compass                                     | Survie et aventure    |          |        |
| Traverse une rivière à gué                        | wading across a river                                 | Survie et aventure    |          |        |
| Escalade une falaise                              | climbing a cliff face                                 | Survie et aventure    |          |        |
| Explore une ruine envahie par la jungle           | exploring jungle-covered ruins                        | Survie et aventure    |          |        |
| Avance à la lampe torche                          | advancing with a flashlight                           | Survie et aventure    |          |        |
| Installe une antenne radio                        | setting up a radio antenna                            | Survie et aventure    |          |        |
| Pose un piège                                     | setting a snare trap                                  | Survie et aventure    |          |        |
| Fait sécher ses vêtements près du feu             | drying clothes by the fire                            | Survie et aventure    |          |        |
| Cuit un poisson sur le feu                        | grilling fish over the fire                           | Survie et aventure    |          |        |
| Construit un abri                                 | building a shelter                                    | Survie et aventure    |          |        |
| Grimpe à un arbre pour observer                   | climbing a tree to scout                              | Survie et aventure    |          |        |
| Envoie un signal au miroir                        | signaling with a mirror                               | Survie et aventure    |          |        |
| Lit des traces au sol                             | reading tracks on the ground                          | Survie et aventure    |          |        |
| Lit un grimoire ancien                            | reading an ancient grimoire                           | Fantasy et grimdark   |          |        |
| Prépare une potion                                | brewing a potion                                      | Fantasy et grimdark   |          |        |
| Chevauche un dragon                               | riding a dragon                                       | Fantasy et grimdark   | Pose     |        |
| Bénit une machine par l'encens                    | blessing a machine with incense smoke                 | Fantasy et grimdark   |          |        |
| Prêche devant une foule                           | preaching to a crowd                                  | Fantasy et grimdark   |          |        |
| Brandit une bannière                              | raising a battle banner                               | Fantasy et grimdark   |          |        |
| Forge une épée                                    | forging a sword                                       | Fantasy et grimdark   |          |        |
| Veille sur des remparts                           | standing watch on the ramparts                        | Fantasy et grimdark   |          |        |
| Joue du luth à la taverne                         | playing the lute in a tavern                          | Fantasy et grimdark   |          |        |
| Déchiffre des runes lumineuses                    | deciphering glowing runes                             | Fantasy et grimdark   |          |        |
| Défend une brèche                                 | defending a breach in the wall                        | Fantasy et grimdark   |          |        |
| Charge à cheval                                   | charging on horseback                                 | Fantasy et grimdark   | Pose     |        |
| Purifie une relique                               | purifying a holy relic                                | Fantasy et grimdark   |          |        |
| Négocie avec un démon                             | bargaining with a demon                               | Fantasy et grimdark   |          |        |
| Lit l'avenir dans une boule de cristal            | reading the future in a crystal ball                  | Fantasy et grimdark   |          |        |
| Brandit un marteau-tonnerre                       | wielding a thunder hammer                             | Fantasy et grimdark   |          |        |
| Récite une litanie de maintenance                 | reciting a litany of maintenance over a machine       | Fantasy et grimdark   |          |        |

---

## Incompatibilités

- Texture des cheveux : Crépus ↔ Lisses et soyeux
- Style vestimentaire : Pyjama ↔ Chaussures : Escarpins
- Pose : En pleine course ↔ Action : Lit un livre
- Pose : Sprint ↔ Action : Lit un livre
- Pose : En plein saut ↔ Action : Lit un livre
- Pose : En pleine course ↔ Action : Boit un café
- Pose : En pleine course ↔ Action : Mange des ramens
- Pose : Recroquevillé(e) ↔ Action : Danse
- Pose : Saut de parkour ↔ Action : Boit un thé
