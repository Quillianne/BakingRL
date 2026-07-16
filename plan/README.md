# BakingRL 0.10 - Plan de livraison avec Marketplace

## Objectif

Ce plan poursuit la refonte du code existant de `BakingRL`, `BakingRLSDK`,
`BakingRLPlugins` et `BakingRLMarketplace`. La livraison cible une application
Windows fonctionnelle, une plateforme permettant de construire des plugins
puissants et une marketplace proposant les plugins officiels des le premier
demarrage.

- `BakingRL` reste le host: Rocket League, runtime, fenetres, stockage,
  installation, marketplace et administration.
- `BakingRLSDK` fournit les contrats types, validateurs, templates et outils de
  publication.
- `BakingRLPlugins` contient les plugins officiels et leurs fiches marketplace.
- `BakingRLMarketplace` contient l'index de confiance, les editeurs, les
  versions approuvees, les sections editees et la chaine de publication du
  catalogue signe.
- Les statistiques, layouts, visuels et fonctions OBS restent dans les plugins.

La version cible Windows, Rocket League en mode fenetre ou sans bordure et OBS
Browser Source. Le plein ecran exclusif, le cloud et le support complet de
macOS/Linux restent hors de cette livraison.

## Modele de confiance

Le modele suit celui des extensions VS Code:

- un plugin peut executer du code Node ou un sidecar natif avec les droits de
  l'utilisateur;
- BakingRL applique des controles, permissions declaratives et mediations host
  lorsque cela est possible, mais ne garantit pas qu'un plugin soit sur;
- une signature prouve l'identite cryptographique de l'editeur et l'integrite
  du bundle, pas l'innocuite du code;
- la validation marketplace controle le format, la compatibilite, l'integrite
  et la qualite minimale de la fiche, sans constituer un audit de securite;
- avant la premiere installation d'un editeur, l'utilisateur doit confirmer
  qu'il lui fait confiance;
- cette confiance est liee a la cle de l'editeur et un changement de cle impose
  une nouvelle confirmation;
- les permissions sensibles, Node, les sidecars et l'exposition reseau sont
  presentes avant installation;
- les plugins officiels portent un badge distinct `BakingRL officiel`; un
  editeur tiers peut etre verifie sans etre presente comme garanti;
- BakingRL conserve ses protections contre les chemins dangereux, secrets
  exposes, acces inter-plugin non declares et installations partielles.

## Coordination

1. Garder les quatre depots propres avec des commits separes par depot et
   responsabilite.
2. Avant toute parallelisation, chaque agent lit ce plan et inspecte sa future
   zone de code.
3. Chaque agent remet son analyse des contrats, dependances, migrations et
   risques sans commencer l'implementation.
4. Preparer une proposition finale de repartition puis demander aux agents leur
   avis sur cette proposition.
5. Ne demarrer les travaux paralleles qu'apres accord sur les contrats communs
   et l'ordre d'integration.
6. Valider chaque tranche inter-repo avec le SDK package et installe localement.

## Marketplace

### Catalogue et publication

- Reutiliser dans `BakingRLMarketplace` le catalogue statique versionne
  `bakingrl.marketplace/1` et ajouter son client dans `BakingRL`.
- Publier un catalogue statique signe via GitHub Pages ou CDN et heberger les
  bundles `.brlp` dans des GitHub Releases.
- Inclure identite, version, compatibilite Runtime API, description, medias,
  dependances, permissions, capacites natives, URL, SHA-256, cle editeur et
  canal.
- Signer l'index avec une cle marketplace dont la cle publique est integree a
  BakingRL.
- Verifier separement signature du catalogue, SHA-256 du bundle et signature
  Ed25519 de l'editeur.
- Gerer la revocation d'une version, d'un plugin ou d'une cle compromise.
- Conserver les installations manuelles `.brlp`, URL, Git et deep link pour les
  developpeurs.

### Publications tierces

- Accepter les plugins tiers par proposition de fiche dans
  `BakingRLMarketplace`, sans importer leur code source.
- Exiger bundle signe, release immuable, hash, documentation, support, medias
  et declaration des permissions.
- Ajouter au SDK la validation des fiches et la preparation d'une soumission.
- La CI controle archive, signature, hash, manifeste, compatibilite,
  dependances et installation temporaire.
- La revue humaine controle la fiche et l'identite annoncee sans certifier le
  comportement du plugin.
- Une mise a jour repasse par les validations techniques et une rotation de cle
  demande une nouvelle approbation.

### Interface et premier demarrage

- Ajouter les vues Marketplace, Installes et Mises a jour.
- Fournir recherche, categories, filtres, details, captures, dependances,
  permissions, installation et mise a jour.
- Presenter avant installation l'editeur, sa cle, son niveau de confiance et
  les capacites sensibles.
- Installer les dependances dans l'ordre et transactionnellement; restaurer les
  versions precedentes en cas d'echec.
- Signaler les mises a jour sans les appliquer automatiquement.
- Mettre en cache le dernier catalogue signe valide pour la consultation hors
  ligne.
- Au premier demarrage, proposer depuis la marketplace `Layout Studio`,
  `Extended Stats`, `Broadcast Visuals` et `OBS Gateway`, selectionnes par
  defaut mais desactivables.
- Demander explicitement la confiance envers l'editeur officiel BakingRL.
- Si la marketplace est inaccessible, permettre de reessayer, continuer sans
  plugin ou installer un `.brlp`.

## Plateforme et SDK

Le contrat normatif partage entre les quatre depots est decrit dans
[`runtime-api-2.3.md`](runtime-api-2.3.md).

- Faire evoluer la Runtime API de `2.2` vers `2.3`.
- Accepter uniquement les packages `2.3.x`; les packages plus anciens doivent
  etre reconstruits et aucune couche de retrocompatibilite n'est maintenue.
- Ajouter un type de webview `surface` avec taille, position, ecran,
  transparence, always-on-top, click-through et redimensionnement.
- Le host detruit les surfaces lorsqu'un plugin est desactive ou plante.
- Etendre le stockage prive avec `readJson`, `writeJson` atomique, `list` et
  `delete`, avec chemins relatifs securises et quota par plugin.
- Conserver services, evenements, ressources publiques et extension points
  comme contrats generiques entre plugins.
- Ajouter les types correspondants au SDK et mettre a jour validateur,
  templates, documentation et tests.
- Documenter quelles API sont mediatisees et quelles capacites Node ou sidecar
  sortent de cette protection.
- Maintenir les POC comme tests techniques de la plateforme.

## IHM BakingRL

- Recentrer la navigation sur Vue d'ensemble, Plugins, Diagnostics et Reglages.
- Utiliser une interface desktop sobre: palette graphite neutre, accents
  limites, densite maitrisee et hierarchie claire.
- Afficher connexion Rocket League, etat du runtime, plugins necessitant une
  action et raccourcis vers leurs outils.
- Regrouper Marketplace, Installes et Mises a jour dans la section Plugins.
- Montrer pour chaque plugin version, editeur, confiance, dependances,
  capacites, reglages, outils et diagnostics.
- Distinguer plugin officiel, editeur tiers approuve et editeur non approuve.
- Conserver les outils de telemetrie mock dans un mode developpeur.

## Plugins officiels

### Layout Studio

- Transformer le POC existant en `bakingrl.layout-studio`.
- Gerer plusieurs layouts avec resolution, widgets, position, taille, ordre,
  opacite, visibilite, verrouillage et reglages.
- Fournir bibliotheque, canvas, zoom, deplacement, redimensionnement,
  alignement, calques, duplication et undo/redo.
- Persister les layouts et permettre import/export JSON.
- Exposer `layout-studio.visual@1` et un service produisant un `RenderBundle`.
- Utiliser le meme moteur pour l'editeur, l'overlay en jeu et OBS.

### Extended Stats

- Extraire les services statistiques existants de `cast-package`.
- Calculer metriques joueur/equipe et agregats de match et serie/BO.
- Publier snapshots, evenements et services types.
- Conserver un historique configurable de 500 matchs par defaut, avec maximum
  de 5 000.
- Fournir liste des matchs, detail, comparaison simple et purge.

### Broadcast Visuals

- Refactorer les visuels de `cast-package` en contributions Layout Studio.
- Livrer scoreboard, statistiques, boost, but, victoire et statistiques plein
  ecran.
- Declarer pour chaque widget taille, reglages, ressources et dependances.
- Consommer Extended Stats lorsque necessaire.

### OBS Gateway

- Conserver le sidecar Rust et les routes HTTP existantes.
- Dependre de Layout Studio et consommer son `RenderBundle`.
- Servir chaque layout, ses ressources, son snapshot et les evenements
  SSE/WebSocket.
- Fournir des URL pretes pour OBS Browser Source.
- Rester sur `127.0.0.1` par defaut et avertir avant toute exposition reseau.
- Ne pas ajouter le pilotage des scenes OBS dans cette version.

## Tests et livraison

- Packager `BakingRLSDK` et installer ses archives locales dans
  `BakingRLPlugins` avant les validations.
- Tester catalogue signe ou altere, bundle altere, cle revoquee, rotation de
  cle, confiance editeur, dependances et rollback.
- Verifier qu'aucun badge ou message ne transforme une validation technique en
  garantie de securite.
- Tester le premier demarrage en ligne, hors ligne, annule et repris.
- Tester un plugin tiers valide et un plugin manuel non repertorie.
- Tester telemetrie mock, historique, editeur, overlay Windows et crash plugin.
- Verifier que le rendu OBS correspond a Layout Studio en 1280x720, 1920x1080
  et 2560x1440.
- Produire un installateur Windows signe et tester installation, mise a niveau
  et desinstallation.

La version est livrable lorsqu'un nouvel utilisateur peut installer BakingRL,
approuver un editeur, choisir les plugins proposes par la marketplace, creer un
layout et l'utiliser en jeu et dans OBS.
