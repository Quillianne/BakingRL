import type { GameEventFrame, RlTelemetryEventName } from "$lib/rlTelemetry";

export type VisualContributionDescriptor = {
  name: string;
  entry: string;
  default_width: number;
  default_height: number;
  settings: string | null;
};

export type NamedContributionDescriptor = {
  name: string;
};

export type ServiceContributionDescriptor = {
  name: string;
  methods: string[];
};

export type PageContributionDescriptor = {
  name: string;
  path: string;
  title: string | null;
  description: string | null;
};

export type OverlayContributionDescriptor = {
  name: string;
  path: string;
  title: string | null;
  description: string | null;
};

export type WebviewContributionDescriptor = {
  name: string;
  entry: string | null;
  path: string | null;
  title: string | null;
  description: string | null;
  icon: string | null;
  configuration: string | null;
  route: string | null;
};

export type ConfigurationContributionDescriptor = {
  title: string | null;
  description: string | null;
  path: string;
  visuals: VisualContributionDescriptor[];
};

export type PackageCompatibilityStatus =
  | "compatible"
  | "incompatible"
  | "requires_newer_host"
  | "unknown_runtime_api";

export type PackageCompatibilityDescriptor = {
  status: PackageCompatibilityStatus;
  runtimeApi: string | null;
  sdk: string | null;
  hostRuntimeApi: string;
  supportedRuntimeApi: string;
  message: string | null;
};

export type PermissionShape = {
  bus?: {
    read?: string[];
    publish?: string[];
  };
  registry?: {
    read?: string[];
    write?: string[];
  };
  network?: {
    http?: string[];
    websocket?: string[];
  };
  storage?:
    | string[]
    | {
        read?: string[];
        write?: string[];
      };
};

export type PermissionSection = {
  title: string;
  rows: {
    label: string;
    values: string[];
    emptyLabel: string;
  }[];
};

export type PackageDescriptor = {
  manifestSchema: string;
  id: string;
  name: string;
  version: string;
  author: string | null;
  kind: string | null;
  activation: Record<string, unknown> | null;
  runtime: Record<string, unknown> | null;
  contributes: Record<string, unknown> | null;
  capabilities: Record<string, unknown> | null;
  diagnostics: Record<string, unknown> | null;
  safeMode: Record<string, unknown> | null;
  enabled: boolean;
  status: "installed" | "deleting";
  path: string;
  contributions: {
    commands: NamedContributionDescriptor[];
    visuals: VisualContributionDescriptor[];
    services: ServiceContributionDescriptor[];
    views: WebviewContributionDescriptor[];
    assets: NamedContributionDescriptor[];
    schemas: NamedContributionDescriptor[];
    pages: PageContributionDescriptor[];
    overlays: OverlayContributionDescriptor[];
    webviews: WebviewContributionDescriptor[];
    configuration: ConfigurationContributionDescriptor | null;
  };
  effective_permissions: PermissionShape;
  compatibility: PackageCompatibilityDescriptor;
  settings: string | null;
  has_public_settings: boolean;
  has_secrets: boolean;
  error: string | null;
};

export type JsonSchema = {
  title?: string;
  description?: string;
  type?: string | string[];
  format?: string;
  default?: unknown;
  enum?: unknown[];
  oneOf?: JsonSchemaOption[];
  anyOf?: JsonSchemaOption[];
  items?: JsonSchema;
  properties?: Record<string, JsonSchema>;
  required?: string[];
  minimum?: number;
  maximum?: number;
  minLength?: number;
  maxLength?: number;
  "x-bakingrl-secret"?: boolean;
};

export type JsonSchemaOption = {
  const?: unknown;
  title?: string;
  description?: string;
};

export type PackageSecretDescriptor = {
  key: string;
  label: string;
  description: string | null;
  required: boolean;
  configured: boolean;
};

export type PackageConfigurationState = {
  packageId: string;
  title: string;
  hasCustomPage: boolean;
  schema: JsonSchema | null;
  values: Record<string, unknown>;
  secrets: PackageSecretDescriptor[];
  secretStoreAvailable: boolean;
  secretStoreError: string | null;
};

export type ManifestContributes = {
  commands?: Record<string, unknown>;
  visuals?: Record<string, unknown>;
  services?: Record<string, unknown>;
  views?: Record<string, unknown>;
  assets?: Record<string, unknown>;
  schemas?: Record<string, unknown>;
  pages?: Record<string, unknown>;
  overlays?: Record<string, unknown>;
  webviews?: Record<string, unknown>;
  layouts?: Record<string, unknown>;
  configuration?: Record<string, unknown>;
};

export type BundleInspection = {
  manifest: {
    schema: string;
    id: string;
    name: string;
    version: string;
    author: string | null;
    kind?: string | null;
    activation?: Record<string, unknown> | null;
    runtime?: Record<string, unknown> | null;
    contributes?: ManifestContributes | null;
    capabilities?: Record<string, unknown> | null;
    diagnostics?: Record<string, unknown> | null;
    safeMode?: Record<string, unknown> | null;
    compatibility?: {
      runtimeApi?: string | null;
      sdk?: string | null;
    } | null;
    permissions?: PermissionShape;
  };
  hashes_present: boolean;
  signature_present: boolean;
  signature_verified: boolean;
  signature_public_key: string | null;
  verified_developer: {
    id: string;
    name: string;
  } | null;
  file_count: number;
  uncompressed_size: number;
  sha256: string;
};

export type RuntimeInfo = {
  runtimeApiVersion: string;
  supportedRuntimeApi: string;
};

export type PreparedPackageInstall = {
  path: string;
  source: string;
  inspection: BundleInspection;
};

export type PendingInstall = PreparedPackageInstall & {
  kind: "file" | "url" | "git" | "marketplace";
  label: string;
};

export type MarketplaceListing = {
  schema: string;
  packageId: string;
  displayName: string;
  shortDescription: string;
  longDescription: string;
  tags: string[];
  repo: string;
  iconUrl: string | null;
  bannerUrl: string | null;
  screenshots: {
    url: string;
    alt: string | null;
    caption: string | null;
  }[];
  links: {
    docs?: string | null;
    support?: string | null;
    homepage?: string | null;
  };
};

export type MarketplaceApprovedVersion = {
  version: string;
  bundleUrl: string;
  bundleSha256: string;
  signaturePublicKey: string;
  runtimeApi: string | null;
  revoked?: boolean;
  review: {
    status: string;
    reviewedAt: string;
    permissions: PermissionShape;
  };
};

export type MarketplaceCatalogPackage = {
  id: string;
  developerId: string;
  developerName: string | null;
  developerVerified: boolean;
  repo: string;
  listingUrl: string;
  listing: MarketplaceListing | null;
  listingError: string | null;
  approvedVersions: MarketplaceApprovedVersion[];
};

export type MarketplaceCatalog = {
  generatedAt: string;
  sections: {
    recommended: string[];
    new: string[];
  };
  packages: MarketplaceCatalogPackage[];
};

export type ConfirmRequest = {
  title: string;
  message: string;
  confirmLabel: string;
  danger?: boolean;
  run: () => void | Promise<void>;
};

export type OverlayItem = {
  id: string;
  package_id: string;
  export_name: string;
  name: string;
  x: number;
  y: number;
  width: number;
  height: number;
  z_index: number;
  visible: boolean;
  locked: boolean;
  opacity: number;
  settings: Record<string, unknown>;
};

export type OverlayLayer = {
  id: string;
  name: string;
  kind: "normal" | "event";
  visible: boolean;
  locked: boolean;
  order: number;
  items: OverlayItem[];
};

export type OverlayLayout = {
  id: string;
  name: string;
  width: number;
  height: number;
  layers: OverlayLayer[];
  items?: OverlayItem[];
  created_at_ms: number;
  updated_at_ms: number;
  template_source?: string | null;
  thumbnail?: string | null;
};

export type OverlayLayoutCatalog = {
  active_layout_id: string;
  stream_layout_id: string;
  layouts: OverlayLayout[];
};

export type RecentActivityEntry =
  | {
      kind: "page";
      id: string;
      updatedAtMs: number;
      page: PageLayout;
    }
  | {
      kind: "layout";
      id: string;
      updatedAtMs: number;
      layout: OverlayLayout;
    };

export type PageItem = {
  id: string;
  kind: "visual" | "text" | "image" | "shape";
  package_id?: string | null;
  export_name?: string | null;
  name: string;
  x: number;
  y: number;
  width: number;
  height: number;
  z_index: number;
  visible: boolean;
  locked: boolean;
  opacity: number;
  settings: Record<string, unknown>;
};

export type PageLayer = {
  id: string;
  name: string;
  kind: "normal";
  visible: boolean;
  locked: boolean;
  order: number;
  items: PageItem[];
};

export type PageLayout = {
  id: string;
  name: string;
  favorite: boolean;
  width: number;
  height: number;
  background: {
    kind: "color" | "image";
    color: string;
    image?: string | null;
    fit: "cover" | "contain" | "stretch";
  };
  settings: {
    open_target: "app" | "window";
  };
  layers: PageLayer[];
  created_at_ms: number;
  updated_at_ms: number;
  template_source?: string | null;
  thumbnail?: string | null;
};

export type PagesFile = {
  pages: PageLayout[];
};

export type AppSettings = {
  behavior: {
    launch_at_startup: boolean;
    close_will_hide: boolean;
    start_minimized: boolean;
  };
  security: {
    plugins_safe_mode: boolean;
    disable_plugin_activation: boolean;
    require_trusted_remote_packages: boolean;
    trusted_package_public_keys: string[];
  };
  obs: {
    host: string;
    port: number;
    access_token: string;
  };
  overlay: {
    hide_when_game_unfocused: boolean;
    update_state_throttle_fps: number;
    use_monitor_size: boolean;
    monitor_id?: string | null;
    screen_width: number;
    screen_height: number;
  };
  telemetry: {
    rocket_league_host: string;
    rocket_league_port: number;
  };
};

export type TelemetryConnectionStatus = {
  state: "connecting" | "connected" | "disconnected" | string;
  message: string | null;
  host: string;
  port: number;
  updated_at_ms: number;
};

export type ObsGatewayStatus = {
  running: boolean;
  address: string;
  message: string | null;
};

export type PluginDiagnosticEvent = {
  packageId: string | null;
  source: string;
  severity: "info" | "warning" | "error" | "fatal";
  phase: string;
  message: string;
  timestampMs: number;
  crashCount?: number;
};

export type OverlayMonitor = {
  id: string;
  name: string;
  x: number;
  y: number;
  width: number;
  height: number;
  scaleFactor: number;
  primary: boolean;
  current: boolean;
};

export type RegistryEntry = {
  key: string;
  value: unknown;
};

export type DeveloperTelemetryEntry = {
  id: string;
  receivedAt: string;
  receivedAtMs: number;
  eventName: string;
  frame: GameEventFrame;
};

export type DeveloperTelemetryGroup = {
  eventName: string;
  latest: DeveloperTelemetryEntry;
  count: number;
  lastReceivedAt: number;
};

export type DeveloperTelemetrySort = "arrival" | "alpha";
export type DeveloperFrameTemplate = RlTelemetryEventName;

export type RuntimeErrorEvent = {
  kind?: string;
  source?: string;
  message?: string;
  timestamp_ms?: number;
};

export type DeveloperErrorEntry = {
  id: string;
  receivedAt: string;
  receivedAtMs: number;
  kind: string;
  source: string;
  message: string;
};

export type ToastTone = "info" | "success" | "warning" | "error";

export type ToastMessage = {
  id: string;
  tone: ToastTone;
  message: string;
};
