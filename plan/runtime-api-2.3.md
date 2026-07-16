# BakingRL Runtime API 2.3 - Contrat d'integration

Ce document fige les contrats inter-repo de la livraison BakingRL 0.10. Les
implementations peuvent ajouter des helpers internes, mais elles ne doivent pas
changer les formes ou les garanties ci-dessous sans mettre a jour les quatre
depots.

## Version supportee

- Le host expose Runtime API `2.3.0` et accepte uniquement les packages
  `2.3.x`.
- Le schema de manifest reste `bakingrl.plugin/4`.
- Les packages plus anciens sont incompatibles et doivent etre reconstruits.
- Aucune couche de retrocompatibilite, aucun adaptateur d'URI et aucune
  migration automatique de stockage legacy ne sont maintenus.
- Tous les packages utilisent uniquement des chemins de stockage relatifs.

## Permissions

Le manifest 2.3 peut declarer:

```json
{
  "permissions": {
    "bus": { "read": [], "publish": [] },
    "registry": { "read": [], "write": [] },
    "network": { "http": [], "websocket": [], "listen": [] },
    "storage": { "read": [], "write": [] }
  }
}
```

Les motifs de bus, registre et stockage sont soit une valeur exacte, soit une
valeur contenant un unique `*` terminal. La comparaison est sensible a la
casse. `*` seul autorise toute valeur. Le host normalise les chemins de stockage
avant comparaison.

Les permissions reseau sont structurees et n'utilisent pas de glob URL:

```ts
type NetworkEndpoint = {
  scheme: "http" | "https" | "ws" | "wss";
  host: string;
  ports: "*" | number[];
  pathPrefixes?: string[];
};

type ListenEndpoint = {
  transport: "http" | "https" | "ws" | "wss" | "tcp";
  host: string;
  ports: "*" | number[];
};
```

Le schema limite `http` aux schemes HTTP et `websocket` aux schemes WS. Scheme
et host sont mis en minuscules, le host est normalise en IDNA, les ports par
defaut sont 80/443 et les chemins sont normalises avant comparaison.

Le host applique bus, registre et stockage pour ses API mediatisees. Les
permissions reseau et les acces directs d'un runtime Node ou d'un sidecar sont
des declarations de consentement, pas une sandbox. Les interfaces et la
documentation ne doivent jamais presenter ces permissions comme une garantie de
securite.

Les capacites natives affichees par la marketplace sont derivees de maniere
normative depuis `runtime.node`, `runtime.sidecars`, leurs plateformes et les
webviews `surface`. Elles ne sont pas ressaisies dans le manifest.

## Surfaces

Une surface est une webview declaree ainsi:

```ts
type SurfaceDeclaration = {
  id: string;
  entry: string;
  title?: string;
  kind: "surface";
  defaultSize: [number, number];
  surface: {
    defaultPosition?: [number, number];
    defaultScreen?: "primary" | string;
    transparent?: boolean;
    alwaysOnTop?: boolean;
    clickThrough?: boolean;
    resizable?: boolean;
  };
};
```

Valeurs par defaut: position `[0, 0]`, ecran `primary`, booleens `false`, sauf
`resizable` a `true`. Transparence, always-on-top, click-through et resizable
sont des capacites immuables du manifest.

`context.webviews.open(id, options)` accepte seulement `position`, `size` et
`screen`. Une surface est singleton par `packageId/webviewId`. Un nouvel appel
reconfigure la surface existante et retourne:

```ts
type SurfaceState = {
  instanceId: string;
  screen: string;
  bounds: { x: number; y: number; width: number; height: number };
  scaleFactor: number;
  visible: boolean;
};
```

Les coordonnees sont des pixels logiques relatifs a la work area. Le host garde
au moins 64 pixels visibles. Un ecran absent utilise l'ecran primaire et produit
un diagnostic. Le host ferme les surfaces sur disable, crash, reload, safe
mode, uninstall, fermeture du host et sortie de l'application.

## Stockage prive

Le stockage 2.3 est host-owned et vit hors du repertoire d'installation du
package:

```ts
type PluginStorage = {
  readText(path: string): Promise<string>;
  writeText(path: string, contents: string): Promise<void>;
  readJson<T extends JsonValue = JsonValue>(path: string): Promise<T>;
  writeJson(path: string, value: JsonValue): Promise<void>;
  list(prefix?: string): Promise<string[]>;
  delete(path: string): Promise<boolean>;
  usage(): Promise<{ usedBytes: number; quotaBytes: number }>;
};
```

Les chemins utilisent `/`, sont relatifs et ne peuvent contenir chemin absolu,
`..`, antislash ou symlink. `list` est recursif, trie et ne retourne que les
fichiers. Les ecritures utilisent fichier temporaire, flush et rename sous
verrou. Le quota des API mediatisees est de 256 Mio par plugin.

Un uninstall conserve les donnees par defaut. Leur suppression demande une
action explicite. Les anciens stockages situes dans les packages et les donnees
d'un package renomme ne sont pas migres automatiquement.

## Contributions et delegation

L'extension point Layout Studio est declare avec l'id
`layout-studio.visual`, la version `1.0.0` et la cible complete
`bakingrl.layout-studio/layout-studio.visual`.

Une contribution peut deleguer au package cible un service et des evenements de
donnees appartenant uniquement au package contributeur. Le host verifie
l'appartenance et accorde, pendant l'activation de la contribution, seulement:

- l'appel des methodes de service declarees;
- l'abonnement aux evenements declares.

La delegation n'est jamais transitive. Broadcast Visuals expose donc son propre
service `visualData`; il consomme Extended Stats en interne et Layout Studio
n'appelle jamais Extended Stats en son nom.

## RenderBundle

Le service `bakingrl.layout-studio/layoutStudio` expose
`renderBundle({ layoutId })`. Il retourne:

```ts
type RenderBundleV1 = {
  schema: "bakingrl.render-bundle/1";
  layout: {
    id: string;
    revision: number;
    width: number;
    height: number;
    items: Array<{
      id: string;
      contribution: string;
      bounds: { x: number; y: number; width: number; height: number };
      zIndex: number;
      opacity: number;
      visible: boolean;
      settings: Record<string, JsonValue>;
    }>;
  };
  resources: Array<{
    ref: string;
    mediaType: string;
    encoding: "utf8" | "base64";
    contents: string;
    sha256: string;
  }>;
  dataSources: {
    events: string[];
    snapshots: Array<{
      key: string;
      service: string;
      method: string;
      input?: JsonValue;
    }>;
  };
  initialData: Record<string, JsonValue>;
};
```

Le bundle ne contient ni chemin disque ni secret. Layout Studio agrege les
evenements autorises sur `plugin.bakingrl.layout-studio.render.<layoutId>`. Le
meme renderer virtuel consomme `initialData` et ce flux dans la surface en jeu
et dans le navigateur OBS.

## Extended Stats

Le package `bakingrl.extended-stats` expose:

- `series`: `snapshot`, `configure`, `reset`;
- `sequence`: `snapshot`, `advance`, `reset`;
- `stats`: `snapshot`, `listMatches`, `getMatch`, `compare`, `purge`;
- `cageStats`: `snapshot`, `getMatch`.

Evenements publics:

- `plugin.bakingrl.extended-stats.series.changed`;
- `plugin.bakingrl.extended-stats.sequence.changed`;
- `plugin.bakingrl.extended-stats.stats.live`;
- `plugin.bakingrl.extended-stats.stats.match-finalized`;
- `plugin.bakingrl.extended-stats.cage-stats.changed`.

La retention vaut 500 matchs par defaut et 5 000 au maximum logique, sous
reserve du quota. Le plugin consulte `usage()` et purge preventivement les plus
anciens fichiers avant de depasser le quota.

Broadcast Visuals depend de Layout Studio et Extended Stats. OBS Gateway depend
uniquement de Layout Studio.

## Marketplace 2

Le nouveau catalogue utilise `bakingrl.marketplace/2` et contient:

- `sequence` entier monotone, `generatedAt` et `expiresAt`;
- sections `recommended`, `new` et `firstRun`;
- editeurs avec `kind`, verification et cles identifiees;
- fiches et medias hashes inclus dans l'index signe;
- packages, versions, dependances, permissions, capacites derivees et artefacts
  par plateforme;
- statuts `active`, `yanked` et `revoked` pour packages, versions et cles.

`revoked` exige une raison et une date. Une entree yanked n'est plus installable
mais peut continuer a fonctionner. Une entree revoked est bloquee et une
installation existante est desactivee, sans supprimer ses donnees.

La signature couvre les octets exacts de `marketplace.json`. L'enveloppe donne
un `keyId`, jamais une cle a approuver. Le host embarque plusieurs racines pour
permettre leur rotation, persiste le plus grand `sequence` accepte et rejette
les sequences inferieures. La tolerance d'horloge est de 24 heures. Un cache
expire reste consultable mais ne permet ni installation ni mise a jour.

La confiance locale porte sur `(developerId, empreinte de cle)`. Elle reste
distincte des labels `verified` et `official`, et est demandee au premier usage
meme pour l'editeur officiel.

## Transaction d'installation

Une transaction marketplace couvre tout le graphe de dependances:

1. verrou et journal de recuperation;
2. telechargement et staging de tous les bundles;
3. verification catalogue, revocations, compatibilite, graphe, SHA-256,
   signature interne, cle editeur et concordance du manifest;
4. confirmation utilisateur;
5. sauvegarde des repertoires installes, etats enabled et provenance;
6. arret des runtimes concernes et swaps;
7. restauration complete si une operation filesystem echoue.

Le stockage prive n'est jamais rollbacke. Les runtimes sont actives seulement
apres commit. Un crash d'activation desactive le plugin et produit un
diagnostic; il ne restaure pas un ancien bundle.
