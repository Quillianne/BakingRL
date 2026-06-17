# BakingRL - Plan de refonte produit et validation

## But

Ce plan part du code existant de `BakingRL`, `BakingRLSDK` et `BakingRLPlugins`. Il cadre une grosse refonte pour clarifier les frontieres entre host, SDK et plugins.

La refonte de `BakingRL` doit recentrer l'application sur ses responsabilites de plateforme:

- recuperer et exposer les donnees Rocket League;
- fournir un runtime de plugins robuste;
- permettre a `BakingRLPlugins` et aux plugins externes de creer un vrai ecosysteme autour du host;
- rester assez generique pour changer les choix techniques pendant l'implementation.

Ce plan decrit les comportements attendus, les parcours utilisateur/developpeur et les tests qui prouveront que le systeme fonctionne. Il ne fige pas encore le contrat final JSON, les endpoints exacts, ni les protocoles internes.

## Repos et roles

Le travail reste separe entre les trois repositories actuels:

- `BakingRL`: application Tauri/Rust/Svelte. C'est le host runtime, l'application minimale, la collecte Rocket League, l'installation et l'execution des plugins.
- `BakingRLSDK`: SDK des plugins. Il definit les types developpeur, helpers, validateurs, templates et outils CLI necessaires pour creer des plugins.
- `BakingRLPlugins`: plugins first-party et packages de validation. Il sert a valider le host et le SDK avec de vrais plugins simples, sans copier leur source dans le repository host.

`BakingRL` ne doit pas copier le code des plugins de test dans son repository. Les POC suivis dans ce plan doivent rester dans `BakingRLPlugins` et servir de validation fonctionnelle.

## Points immuables

### BakingRL minimal

`BakingRL` garde uniquement les responsabilites de plateforme:

- connexion Rocket League;
- donnees Rocket League normalisees et si possible typees;
- installation, validation, activation et desactivation de plugins;
- runtime plugin;
- settings et secrets;
- etat partage, evenements, diagnostics;
- API locale ou commandes host pour les plugins et l'admin;
- interface admin tres simple.

`BakingRL` ne doit pas devenir proprietaire de domaines metier comme:

- overlay editor;
- rendu de visuels;
- OBS;
- packs de logos;
- scoreboards;
- marketplace avancee.

Ces domaines doivent pouvoir exister comme plugins.

### Donnees Rocket League

`BakingRL` doit exposer les donnees Rocket League comme une source fiable:

- statut de connexion;
- snapshots;
- evenements;
- modele stable et documente;
- typage suffisant pour que les plugins puissent coder sans deviner les champs.

`BakingRL` peut changer la technique interne de collecte, mais les plugins doivent recevoir une surface claire.

### Plugins comme extensions de plateforme

Un plugin doit pouvoir etre simple ou devenir une petite plateforme pour d'autres plugins.

Un plugin simple peut:

- lire des donnees Rocket League autorisees;
- avoir des settings;
- ouvrir une webview de configuration ou d'outil;
- lancer du Node si necessaire;
- lancer un sidecar si necessaire;
- exposer un service;
- publier des ressources ou contenus consommables.

Un plugin plateforme peut en plus:

- declarer une interface que d'autres plugins peuvent etendre;
- documenter ce qu'il accepte;
- decouvrir les plugins qui contribuent a son interface;
- orchestrer une experience plus large sans que `BakingRL` connaisse ce domaine.

### Chaines de plugins

Le systeme doit permettre des chaines du type:

```text
BakingRL
  -> fournit donnees RL + runtime

Overlay Studio
  -> plugin plateforme
  -> cree des overlays ingame ou OBS
  -> expose une interface pour recevoir des visuels

Plugin A
  -> depend d'Overlay Studio
  -> ajoute un type de visuel ou de systeme de contenu
  -> expose lui-meme une interface pour recevoir des images ou ressources

Plugin B
  -> depend de Plugin A
  -> fournit des images, logos ou contenus compatibles avec Plugin A
```

`BakingRL` ne doit pas coder specialement ce cas. Il doit fournir les primitives qui le rendent possible.

### Securite et mediation host

Les plugins ne sont pas consideres comme totalement fiables.

`BakingRL` doit:

- valider ce qu'un plugin declare;
- refuser les chemins dangereux;
- eviter les acces implicites a tout le systeme;
- garder les secrets host-owned;
- mediatiser les appels entre plugins;
- rendre les erreurs visibles dans les diagnostics;
- permettre de desactiver un plugin sans casser `BakingRL`.

### Solutions techniques adaptables

Le plan ne verrouille pas encore:

- le nom exact des champs du manifest;
- le format exact des endpoints;
- le protocole de message entre webviews et host;
- le modele exact d'extension points;
- le format final des ressources partagees.

Pendant l'implementation, ces choix pourront evoluer si les tests de parcours restent satisfaits.

## Parcours utilisateur cibles

### Parcours 1 - Demarrer BakingRL

L'utilisateur lance BakingRL.

Il doit pouvoir:

- voir si `BakingRL` fonctionne;
- voir si Rocket League est connecte ou non;
- voir les derniers diagnostics;
- voir la liste des plugins installes;
- activer ou desactiver un plugin.

Reussite:

- l'app demarre sans plugin;
- l'admin minimal donne un etat clair;
- les erreurs sont comprehensibles.

### Parcours 2 - Installer un plugin simple

L'utilisateur installe un plugin proof of concept simple.

Ce plugin:

- declare des settings;
- lit le flux ou snapshot Rocket League;
- expose un petit service;
- peut afficher une webview minimale.

Reussite:

- le plugin est valide;
- ses settings sont visibles/modifiables;
- il recoit les donnees RL;
- son service repond;
- sa webview s'ouvre si elle est declaree.

### Parcours 3 - Installer un plugin avec sidecar

L'utilisateur installe un plugin proof of concept avec sidecar.

Reussite:

- le sidecar demarre;
- `BakingRL` sait s'il est healthy;
- un crash est detecte;
- les logs/diagnostics sont visibles;
- le plugin peut etre arrete proprement.

### Parcours 4 - Installer Overlay Studio comme plugin plateforme

L'utilisateur installe un plugin proof of concept `Overlay Studio`.

Ce plugin:

- consomme les donnees RL de `BakingRL`;
- expose une interface que d'autres plugins pourront enrichir;
- peut ouvrir une UI d'edition;
- peut produire une sortie visible ou servable pour un overlay de test.

Reussite:

- `BakingRL` ne contient pas de logique overlay;
- Overlay Studio decouvre ses contributions;
- Overlay Studio recoit les donnees RL;
- un overlay de test peut changer quand les donnees RL changent.

### Parcours 5 - Installer un plugin qui enrichit Overlay Studio

L'utilisateur installe un plugin A qui depend d'Overlay Studio.

Ce plugin:

- apporte un visuel, un widget ou une categorie de contenu;
- declare ce qu'il apporte a Overlay Studio;
- peut lui-meme exposer une interface pour etre enrichi par d'autres plugins.

Reussite:

- `BakingRL` detecte la relation entre Overlay Studio et le plugin A;
- Overlay Studio voit la contribution du plugin A;
- le plugin A ne fonctionne pas silencieusement si sa dependance manque;
- la contribution disparait proprement si le plugin A est desactive.

### Parcours 6 - Installer un plugin qui enrichit un autre plugin

L'utilisateur installe un plugin B qui depend du plugin A.

Ce plugin:

- fournit des images, logos, presets, donnees ou ressources;
- ne depend pas directement d'une logique overlay dans `BakingRL`;
- enrichit ce que Plugin A sait utiliser.

Reussite:

- `BakingRL` detecte la chaine BakingRL -> Overlay Studio -> Plugin A -> Plugin B;
- Plugin A voit ce que Plugin B apporte;
- Overlay Studio peut en beneficier via Plugin A;
- la desactivation de Plugin B ne casse ni Plugin A ni Overlay Studio.

## Parcours developpeur cibles

### Developpeur d'un plugin simple

Le developpeur doit pouvoir creer un plugin minimal qui:

- declare son identite;
- declare ses settings;
- lit des donnees Rocket League;
- expose un service simple;
- affiche une webview optionnelle.

Il doit avoir des erreurs de validation claires quand le package est invalide.

Le SDK doit rendre ce parcours simple:

- types partages avec `BakingRL`;
- helpers pour lire les donnees RL autorisees;
- helpers pour services, settings, webviews et diagnostics;
- validator local;
- templates de plugins POC.

### Developpeur d'un plugin plateforme

Le developpeur doit pouvoir creer un plugin qui:

- expose une interface publique pour d'autres plugins;
- documente ce que les autres plugins peuvent lui fournir;
- decouvre les contributions compatibles;
- appelle ou consomme ces contributions via `BakingRL`;
- reste fonctionnel quand aucune contribution externe n'est installee.

### Developpeur d'un plugin contributeur

Le developpeur doit pouvoir creer un plugin qui:

- depend d'un plugin plateforme;
- fournit une contribution compatible;
- annonce clairement ce qu'il apporte;
- echoue proprement si la dependance n'est pas presente ou incompatible.

### Developpeur d'un plugin de contenu

Le developpeur doit pouvoir creer un plugin qui:

- fournit uniquement des ressources ou donnees;
- n'a pas besoin de Node ou sidecar si inutile;
- peut etre consomme par un autre plugin;
- ne donne pas a `BakingRL` de connaissance metier supplementaire.

## Proofs of concept attendus

Pour valider la refonte, il faudra maintenir des plugins de validation:

Ces plugins doivent vivre dans `BakingRLPlugins`.

1. `poc-simple-node`
   - lit un snapshot RL mock;
   - expose un service `ping`;
   - ecrit un etat de debug.

2. `poc-webview-settings`
   - declare des settings;
   - ouvre une webview;
   - lit/modifie ses settings.

3. `poc-sidecar`
   - lance un sidecar;
   - repond a un health check;
   - permet de tester un crash.

4. `poc-overlay-studio`
   - plugin plateforme;
   - consomme les donnees RL;
   - expose une interface pour recevoir des contributions;
   - affiche un overlay de test tres simple.

5. `poc-visual-pack`
   - depend de `poc-overlay-studio`;
   - ajoute un visuel ou widget de test;
   - montre que le plugin plateforme peut decouvrir et utiliser la contribution.

6. `poc-content-pack`
   - depend de `poc-visual-pack`;
   - fournit des images ou donnees de test;
   - montre une chaine de plugins a trois niveaux.

Ces POC ne sont pas des produits finaux. Ils servent a prouver que le modele de plateforme fonctionne.

Le SDK dans `BakingRLSDK` est considere assez utile quand ces POC peuvent etre ecrits sans recopier manuellement les contrats internes de `BakingRL`.

## Tests unitaires attendus

### Telemetry

- transforme un evenement Rocket League brut ou mock en modele `BakingRL` type;
- produit un snapshot coherent;
- signale une deconnexion;
- garde une surface stable pour les plugins.

### Validation plugin

- accepte un plugin minimal valide;
- refuse un plugin sans identite stable;
- refuse une version de runtime API incompatible;
- refuse des chemins absolus ou sortant du package;
- refuse une declaration incoherente;
- produit des erreurs lisibles.

### Settings et secrets

- valide les schemas de settings;
- stocke les valeurs non secretes;
- ne stocke pas les secrets en clair;
- expose seulement les settings declarees par le plugin.

### Runtime

- modelise les etats stopped, starting, running, crashed;
- capture stdout/stderr;
- remonte un diagnostic de crash;
- arrete proprement un plugin.

### Dependances et contributions

- resout une dependance presente;
- detecte une dependance manquante;
- detecte une incompatibilite;
- indexe une interface exposee par un plugin;
- indexe une contribution vers une interface;
- retire les contributions quand un plugin est desactive.

### Ressources et contenus

- indexe des ressources publiques declarees;
- garde les ressources privees non accessibles aux autres plugins;
- refuse les chemins dangereux;
- permet de retrouver une ressource par plugin, type ou metadata generique;
- ne donne pas de signification metier au contenu.

## Tests d'integration attendus

### BakingRL sans plugin

- l'app demarre;
- l'admin minimal s'affiche;
- aucun plugin installe ne provoque d'erreur;
- les diagnostics de boot sont lisibles.

### Flux Rocket League mock

- `BakingRL` lance une source RL mock;
- un snapshot est visible dans l'admin;
- un plugin de test recoit les changements.

### Plugin simple

- installation locale;
- validation;
- activation;
- lecture donnees RL;
- appel service;
- desactivation propre.

### Plugin avec webview et settings

- settings visibles;
- modification persistee;
- webview ouverte;
- la webview n'a pas d'acces direct non declare au host.

### Plugin avec sidecar

- sidecar lance;
- health visible;
- crash simule detecte;
- logs visibles;
- arret propre.

### Chaine de plugins ecosysteme

Installer dans l'ordre:

1. `poc-overlay-studio`
2. `poc-visual-pack`
3. `poc-content-pack`

Le test doit prouver que:

- Overlay Studio fonctionne seul;
- Visual Pack est detecte par Overlay Studio;
- Content Pack est detecte par Visual Pack;
- la chaine complete produit un resultat visible dans l'overlay de test;
- desactiver Content Pack retire seulement son contenu;
- desactiver Visual Pack retire sa contribution d'Overlay Studio;
- desactiver Overlay Studio rend les contributions dependantes inactives ou clairement non utilisables.

### Robustesse erreurs

- plugin invalide refuse sans etat partiel;
- dependance manquante expliquee;
- plugin crashe sans faire tomber `BakingRL`;
- settings invalides rejetes;
- ressource interdite non servie.

## Definition de succes

La refonte est prete pour continuer si:

- tous les tests unitaires passent;
- tous les tests d'integration POC passent;
- l'admin minimal permet de comprendre l'etat du runtime;
- aucun code overlay/editor/OBS specifique n'est dans `BakingRL`;
- un plugin plateforme peut etre cree;
- un plugin peut contribuer a un autre plugin;
- une chaine de plugins a trois niveaux fonctionne;
- les donnees Rocket League alimentent au moins un POC visuel via un plugin, pas via `BakingRL`.

## Commandes de validation cible

Les commandes exactes pourront evoluer avec le scaffold, mais l'objectif est:

```sh
npm run check
npm run build
cargo test
cargo check
```

Les tests d'integration devront avoir une commande dediee une fois le harness en place.
