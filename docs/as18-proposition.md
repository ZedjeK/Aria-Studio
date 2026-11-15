# AS-18 : des listes que tu pilotes depuis Réglages

Proposition du 2026-11-11, réalisée le même jour sur la branche `feat/as18-listes`. Choix faits sur les points
laissés ouverts : tes ajouts et tes phrases témoins vont dans la section `securite` de `reglages.json` (le fichier
unique des réglages depuis la 2.1.0 : dossier utilisateur, ou dossier du programme en mode portable), qui garde une
sauvegarde avant chaque changement ; les âges reconnus en plus sont « of 14 », « de 14 », « 12 yrs »,
« 16 years of age » et « age of 15 ».

Remis à plus tard, à valider avant d'entrer : « jeune fille » comme mot de mineur, les mots à double sens de la liste
sensible, la défense contre les graphies déguisées, et la liste des derniers blocages. Ces quatre points sont décrits
plus bas mais ne sont pas dans cette réalisation.

## Objectif

Garder le filtre AS-18 tel qu'il fonctionne (un personnage mineur associé à un terme sensible est bloqué), et te
donner la main sur ses listes : les voir, les compléter, vérifier l'effet, sans toucher au code.

Périmètre : usage personnel, en local. GitHub, les releases et les signalements sont hors sujet pour l'instant.

## Ce qui ne change pas

- La règle : mineur seul = autorisé ; mineur + terme sensible = bloqué ; terme toujours bloqué = bloqué partout.
- Le seuil de 18 ans et la reconnaissance des âges (« 12 ans », « 15yo », « aged 14 ») restent dans le code.
- La fonction `securite.verifier()` et tous ses points d'appel (Plateau, Scénario, Séquences, Voix, listes).
- Le filtre ne se désactive pas.

## Deux étages

| Étage | Où | Depuis Réglages | À la main |
|---|---|---|---|
| **Socle** | `securite/socle.yaml`, dans le dossier d'Aria | lecture, et ajout par promotion | modifiable dans un éditeur de texte |
| **Mes ajouts** | `reglages.json`, section `securite` | ajout et retrait | modifiable aussi |

Chaque étage a les trois mêmes listes : `mineurs`, `sensibles`, `toujours_bloques`. Le filtre applique les deux
étages ensemble, sans distinction.

Le socle ne peut que grandir depuis l'appli : aucun bouton ne retire ni ne modifie un de ses termes. Corriger un terme
du socle se fait en ouvrant le fichier. Ce détour est voulu : affaiblir le socle reste un geste délibéré.

## La carte « Sécurité · Protection des mineurs » (Réglages)

Pour chacune des trois listes :

- **Socle** : replié par défaut, lisible terme par terme, en lecture seule.
- **Mes ajouts** : chaque terme avec un bouton de retrait et un bouton **Promouvoir** ; un champ pour en ajouter.

Et pour l'ensemble :

- **Essayer une phrase** : tu saisis un texte, Aria indique s'il est bloqué, par quels termes, et de quel étage ils
  viennent.
- **Phrases témoins** : une liste de phrases avec le résultat attendu (« doit bloquer » ou « doit passer »). Aria les
  revérifie à chaque changement de liste et affiche un point vert ou rouge à côté de chacune.
- **Derniers blocages** : les dernières générations refusées, avec le terme en cause et son étage.
- **Chemin du fichier du socle**, affiché pour le retrouver.

## Le cycle d'un terme

1. **Ajouter.** Un terme passe alors qu'il devrait bloquer : tu l'ajoutes à « Mes ajouts ». Il compte dès la
   génération suivante, sans redémarrer.
2. **Vérifier.** Avec « Essayer une phrase » et les phrases témoins : il bloque ce qui est visé, et rien de ce qui
   doit passer.
3. **Promouvoir.** Le terme passe de « Mes ajouts » au socle, après confirmation. Il ne se retire plus depuis l'appli.

## Corriger un terme du socle

Cas type : un terme trop large bloque une scène légitime.

1. Ajouter la phrase bloquée à tort aux phrases témoins, en « doit passer » : son point est rouge.
2. Ouvrir `securite/socle.yaml` et resserrer le terme (le remplacer par des formes plus précises plutôt que le
   supprimer).
3. Enregistrer. Aria relit ses listes à chaque vérification : le point passe au vert, et les autres témoins montrent
   qu'aucun trou ne s'est rouvert.

## Garde-fous

- **Socle absent, vide ou illisible** : Aria refuse de générer et affiche l'erreur, au lieu de filtrer avec des listes
  vides.
- **Saisie d'un ajout** : un terme trop court ou déjà présent est refusé ; un terme qui ferait passer au rouge une
  phrase témoin « doit passer » est signalé avant validation.
- **Promotion** : confirmation demandée, puisqu'elle ne se défait pas depuis l'appli.
- **Phrases témoins de départ** : livrées avec Aria, reprises des cas déjà couverts par les tests actuels.

## Reprise de l'existant

- Les listes actuellement figées dans `app/securite.py` deviennent le contenu initial de `securite/socle.yaml`, avec
  leurs commentaires (pourquoi « strip », « string » et « embrasser » en sont absents).
- `securite/termes.yaml` (ajouts actuels, vide aujourd'hui) est repris dans « Mes ajouts » puis retiré.
- Les tests existants du filtre sont conservés et adaptés au fichier.

## Compléter le socle

Un essai du filtre actuel sur 13 phrases (2026-11-11) a montré que 8 phrases qui devraient être bloquées passent.
Trois familles de manques :

- la liste sensible couvre la nudité et les sous-vêtements, pas les actes ni l'anatomie explicites ;
- la liste des mineurs ne reconnaît ni « jeune fille » ni l'âge écrit « a girl of 14 » ;
- aucune défense contre les graphies déguisées (un chiffre à la place d'une lettre).

Les deux premières se règlent dans les listes du socle. La troisième demande une normalisation dans le code.

## Ordre de réalisation

1. Sortir le socle dans `securite/socle.yaml`, avec le refus de générer s'il manque. Comportement identique à
   aujourd'hui.
2. Compléter le socle et ajouter la normalisation des graphies déguisées.
3. Déplacer « Mes ajouts » dans le dossier utilisateur.
4. Construire la carte de Réglages : listes, ajout et retrait, essai d'une phrase.
5. Ajouter la promotion, les phrases témoins et les derniers blocages.

## Points à confirmer

- L'emplacement exact de « Mes ajouts » et des phrases témoins dans le dossier utilisateur, et leur place en mode
  portable.
- Le nombre de derniers blocages gardés, et s'ils gardent la phrase entière ou seulement le terme en cause.
- Les formes à reconnaître pour les âges, au-delà de « of 14 ».
