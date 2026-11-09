---
type: liste
titre: Plan vidéo - Aria Studio
tags: [video, camera, replique, son, musique, prompt, liste]
version: "1.0"
---

# Plan (vidéo)

> Proposé seulement quand le modèle cible est un modèle vidéo (MiniMax H3). Toutes les sous-listes sont facultatives.
> Bonnes pratiques H3 (fiche du workflow) : un court brief de production, UN seul mouvement de caméra nommé avec sa vitesse,
> des répliques courtes avec la langue et le ton, des sons concrets liés à l'action, la musique à part.
> L'action et la réplique sont des champs libres de l'interface : l'action est traduite par Llama, la réplique jamais.

---

## Caméra

| Valeur                           | Prompt vidéo                                               | Catégorie                 |
| -------------------------------- | ---------------------------------------------------------- | ------------------------- |
| Plan fixe                        | the camera stays still, locked-off shot                    | Mouvements fixes et lents |
| Avancée lente                    | the camera slowly pushes in                                | Mouvements fixes et lents |
| Recul lent                       | the camera slowly pulls back                               | Mouvements fixes et lents |
| Panoramique vers la droite       | the camera slowly pans from left to right                  | Mouvements fixes et lents |
| Panoramique vers la gauche       | the camera slowly pans from right to left                  | Mouvements fixes et lents |
| Bascule vers le haut             | the camera slowly tilts up                                 | Mouvements fixes et lents |
| Bascule vers le bas              | the camera slowly tilts down                               | Mouvements fixes et lents |
| Travelling latéral               | the camera tracks sideways alongside the subject           | Travellings               |
| Suivi de dos                     | the camera follows the subject from behind                 | Travellings               |
| Orbite                           | the camera slowly orbits around the subject                | Travellings               |
| Caméra à l'épaule                | handheld camera with a slight natural shake                | Dynamiques                |
| Grue montante                    | the camera rises on a crane, revealing the scene           | Aérien et grue            |
| Vue de drone                     | an aerial drone shot gliding forward                       | Aérien et grue            |
| Zoom lent                        | a slow zoom in                                             | Mouvements fixes et lents |
| Dolly zoom (vertigo)             | a dolly zoom, the background stretching behind the subject | Dynamiques                |
| Légère avancée vers le visage    | the camera slowly pushes in toward the face                | Mouvements fixes et lents |
| Plan fixe avec respiration       | locked-off shot with a subtle breathing sway               | Mouvements fixes et lents |
| Panoramique révélateur           | a slow pan that reveals the scene                          | Mouvements fixes et lents |
| Rack focus                       | rack focus shifting from foreground to subject             | Mouvements fixes et lents |
| Travelling avant                 | a smooth forward tracking shot                             | Travellings               |
| Travelling arrière               | a smooth backward tracking shot                            | Travellings               |
| Travelling circulaire            | a circular tracking shot around the subject                | Travellings               |
| Suivi de face                    | the camera walks backward in front of the subject          | Travellings               |
| Steadicam                        | smooth steadicam following the subject                     | Travellings               |
| Plongée aérienne descendante     | an aerial shot descending toward the subject               | Aérien et grue            |
| Survol d'un paysage              | a sweeping aerial flyover of the landscape                 | Aérien et grue            |
| Grue descendante                 | the camera lowers on a crane                               | Aérien et grue            |
| Caméra qui s'élève verticalement | the camera rises straight up                               | Aérien et grue            |
| Whip pan                         | a fast whip pan                                            | Dynamiques                |
| Caméra portée nerveuse           | nervous handheld camera, documentary feel                  | Dynamiques                |
| Zoom rapide                      | a quick crash zoom                                         | Dynamiques                |
| Pov du personnage                | first-person point of view shot                            | Dynamiques                |
| Plan subjectif en marchant       | first-person walking shot, slight bob                      | Dynamiques                |
| Ralenti                          | slow motion                                                | Dynamiques                |

## Langue

| Valeur              | Prompt vidéo     |
| ------------------- | ---------------- |
| Français            | French           |
| Anglais             | English          |
| Japonais            | Japanese         |
| Coréen              | Korean           |
| Mandarin            | Mandarin Chinese |
| Espagnol            | Spanish          |
| Italien             | Italian          |
| Allemand            | German           |

## Ton

| Valeur          | Prompt vidéo                        |
| --------------- | ----------------------------------- |
| Murmuré         | in a soft whisper                   |
| Doux            | in a gentle voice                   |
| Complice        | in a warm, knowing tone             |
| Joueur          | in a playful, teasing tone          |
| Enjoué          | in a cheerful voice                 |
| Ferme           | in a firm, steady voice             |
| Froid           | in a cold, flat voice               |
| Ému             | in a voice trembling with emotion   |
| Inquiet         | in a worried voice                  |
| Crié            | shouting                            |

## Ambiance sonore

| Valeur                    | Prompt vidéo                               | Catégorie             |
| ------------------------- | ------------------------------------------ | --------------------- |
| Pluie                     | rain pattering on the pavement             | Nature et météo       |
| Orage                     | rolling thunder and heavy rain             | Nature et météo       |
| Vent                      | wind howling                               | Nature et météo       |
| Ville, la nuit            | distant city traffic and sirens            | Ville et lieux        |
| Néons qui grésillent      | buzzing neon signs                         | Technologie et action |
| Foule                     | the murmur of a crowd                      | Ville et lieux        |
| Café                      | clinking cups and soft chatter             | Ville et lieux        |
| Forêt                     | birdsong and rustling leaves               | Nature et météo       |
| Océan                     | waves crashing on the shore                | Nature et météo       |
| Feu de camp               | a crackling campfire                       | Nature et météo       |
| Pas qui résonnent         | footsteps echoing                          | Ville et lieux        |
| Machines                  | the low hum of machinery                   | Technologie et action |
| Vaisseau spatial          | the deep hum of a spaceship engine         | Technologie et action |
| Bataille au loin          | distant explosions and gunfire             | Technologie et action |
| Silence pesant            | near silence, only soft breathing          | Intérieur et intime   |
| Pluie sur les vitres      | rain tapping on windows                    | Nature et météo       |
| Tonnerre lointain         | distant rolling thunder                    | Nature et météo       |
| Ruisseau                  | a babbling brook                           | Nature et météo       |
| Cascade                   | a rushing waterfall                        | Nature et météo       |
| Chants de grillons        | chirping crickets at night                 | Nature et météo       |
| Neige qui crisse          | snow crunching underfoot                   | Nature et météo       |
| Vagues douces             | gentle waves lapping                       | Nature et météo       |
| Cigales                   | cicadas buzzing in the summer heat         | Nature et météo       |
| Tempête                   | a howling storm                            | Nature et météo       |
| Marché animé              | a busy market with vendors calling         | Ville et lieux        |
| Métro                     | a subway train passing, announcements      | Ville et lieux        |
| Gare                      | a train station with distant announcements | Ville et lieux        |
| Klaxons                   | distant car horns                          | Ville et lieux        |
| Sirène au loin            | a distant siren                            | Ville et lieux        |
| Cloches                   | church bells ringing                       | Ville et lieux        |
| Salle de concert          | muffled concert crowd and bass             | Ville et lieux        |
| Arcade                    | arcade machines beeping                    | Ville et lieux        |
| Bar à cocktails           | ice clinking in glasses, low conversation  | Ville et lieux        |
| Ordinateur qui ronronne   | the hum of computer fans                   | Technologie et action |
| Clavier                   | typing on a mechanical keyboard            | Technologie et action |
| Moteur de voiture         | a car engine idling                        | Technologie et action |
| Alarme                    | a blaring alarm                            | Technologie et action |
| Coups de feu              | gunshots echoing                           | Technologie et action |
| Épées qui s'entrechoquent | swords clashing                            | Technologie et action |
| Respiration calme         | soft calm breathing                        | Intérieur et intime   |
| Tic-tac d'horloge         | a ticking clock                            | Intérieur et intime   |
| Draps froissés            | rustling bedsheets                         | Intérieur et intime   |
| Murmures                  | soft whispers                              | Intérieur et intime   |
| Battements de coeur       | a steady heartbeat                         | Intérieur et intime   |
| Douche qui coule          | running shower water                       | Intérieur et intime   |

## Musique

| Valeur                    | Prompt vidéo                             | Catégorie        |
| ------------------------- | ---------------------------------------- | ---------------- |
| Aucune                    | no music                                 | Calme            |
| Synthwave                 | slow retro synthwave                     | Énergie          |
| Orchestrale épique        | an epic orchestral score                 | Sombre et épique |
| Piano doux                | soft solo piano                          | Calme            |
| Lo-fi                     | mellow lo-fi beats                       | Calme            |
| Ambient sombre            | a dark ambient drone                     | Sombre et épique |
| Tambours taiko            | Japanese taiko drums                     | Sombre et épique |
| Rock                      | driving rock guitars                     | Énergie          |
| Jazz de nuit              | smooth late-night jazz                   | Calme            |
| Électro                   | a pulsing electronic beat                | Énergie          |
| Fugue de Bach             | a Bach harpsichord fugue                 | Calme            |
| Guitare acoustique        | gentle acoustic guitar                   | Calme            |
| Harpe                     | delicate harp arpeggios                  | Calme            |
| Boîte à musique           | a music box melody                       | Calme            |
| Violoncelle mélancolique  | a melancholic solo cello                 | Calme            |
| Ambient planant           | airy ambient pads                        | Calme            |
| Koto et shakuhachi        | traditional Japanese koto and shakuhachi | Du monde         |
| Musique celtique          | a lilting Celtic folk tune               | Du monde         |
| Flamenco                  | flamenco guitar                          | Du monde         |
| Bossa nova                | a bossa nova groove                      | Du monde         |
| Gamelan                   | Indonesian gamelan                       | Du monde         |
| K-pop                     | upbeat K-pop beat                        | Énergie          |
| Hip-hop                   | a boom bap hip-hop beat                  | Énergie          |
| Drum and bass             | fast drum and bass                       | Énergie          |
| Funk                      | a funky bassline                         | Énergie          |
| Disco                     | a disco groove                           | Énergie          |
| Metal                     | heavy metal riffs                        | Énergie          |
| Techno                    | driving techno                           | Énergie          |
| Chiptune                  | 8-bit chiptune                           | Énergie          |
| Chœur épique              | an epic choir                            | Sombre et épique |
| Percussions de guerre     | booming war drums                        | Sombre et épique |
| Cordes tendues (thriller) | tense staccato strings                   | Sombre et épique |
| Drone industriel          | an industrial drone                      | Sombre et épique |
| Orgue                     | a dark church organ                      | Sombre et épique |
| Blues                     | a slow blues guitar                      | Calme            |
| Swing                     | a swing big band                         | Énergie          |
