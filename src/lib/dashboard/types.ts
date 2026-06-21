import type { GameEventFrame, RlTelemetryEventName } from "$lib/rlTelemetry";

export type NamedContributionDescriptor = {
  name: string;
};

export type ServiceContributionDescriptor = {
  name: string;
  methods: string[];
};

export type ExtensionPointContributionDescriptor = {
  name: string;
  version: string | null;
  title: string | null;
  description: string | null;
  schema: string | null;
  service: string | null;
  reference: string;
};

export type PluginContributionDescriptor = {
  name: string;
  target: string;
  kind: string | null;
  title: string | null;
  description: string | null;
  data_schema: string | null;
  service: string | null;
  resources: string[];
  metadata: unknown | null;
};

export type ResourceContributionDescriptor = {
  name: string;
  paths: string[];
  resource_type: string | null;
  visibility: string;
  public: boolean;
  metadata: unknown | null;
  reference: string;
};

export type WebviewContributionDescriptor = {
  name: string;
  entry: string | null;
  path: string | null;
  kind: string | null;
  title: string | null;
  description: string | null;
  icon: string | null;
  configuration: string | null;
  route: string | null;
  default_width: number;
  default_height: number;
};

export type PackageCompatibilityStatus =
  | "compatible"
  | "incompatible"
  | "requires_newer_host"
  | "unknown_runtime_api";

export type PackageCompatibilityDescriptor = {
  status: PackageCompatibilityStatus;
  bakingrlApi: string | null;
  sdk: string | null;
  hostRuntimeApi: string;
  supportedRuntimeApi: string;
  message: string | null;
};

export type PluginRuntimeNodeDescriptor = {
  entry: string;
};

export type PluginRuntimeSidecarDescriptor = {
  id: string;
  bin: string;
  args: string[];
  env: Record<string, string>;
  platforms: string[];
  protocol: string;
  activation: string;
  healthCheck?: {
    method: string;
    intervalMs?: number | null;
    timeoutMs?: number | null;
  } | null;
};

export type SidecarRuntimeStatus = {
  running: boolean;
  lastExitCode?: number | null;
  restartCount: number;
  crashCount: number;
  healthy?: boolean | null;
  lastHealthError?: string | null;
  lastHealthCheckMs?: number | null;
};

export type ExtensionHostRuntimeState = "stopped" | "starting" | "running" | "stopping" | "crashed";

export type ExtensionHostRuntimeStatus = {
  state: ExtensionHostRuntimeState;
  running: boolean;
  lastExitCode?: number | null;
  restartCount: number;
  crashCount: number;
  lastError?: string | null;
  updatedAtMs?: number | null;
};

export type PluginRuntimeDescriptor = {
  node: PluginRuntimeNodeDescriptor | null;
  sidecars: PluginRuntimeSidecarDescriptor[];
};

export type PackageDependencyStatus =
  | "pending"
  | "satisfied"
  | "optional_missing"
  | "missing"
  | "disabled"
  | "incompatible"
  | "version_mismatch";

export type PackageDependencyDescriptor = {
  package_id: string;
  version: string | null;
  optional: boolean;
  status: PackageDependencyStatus;
  message: string | null;
};

export type PackageDescriptor = {
  manifestSchema: string;
  id: string;
  name: string;
  version: string;
  author: string | null;
  runtime: PluginRuntimeDescriptor | null;
  extensionHostStatus?: ExtensionHostRuntimeStatus | null;
  sidecarStatuses: Record<string, SidecarRuntimeStatus>;
  contributes: Record<string, unknown> | null;
  dependencies: PackageDependencyDescriptor[];
  enabled: boolean;
  status: "installed" | "deleting";
  path: string;
  contributions: {
    commands: NamedContributionDescriptor[];
    services: ServiceContributionDescriptor[];
    extension_points: ExtensionPointContributionDescriptor[];
    contributions: PluginContributionDescriptor[];
    resources: ResourceContributionDescriptor[];
    webviews: WebviewContributionDescriptor[];
  };
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
  "x-bakingrl-restart-required"?: boolean;
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
  hasSettingsWebview: boolean;
  schema: JsonSchema | null;
  values: Record<string, unknown>;
  secrets: PackageSecretDescriptor[];
  secretStoreAvailable: boolean;
  secretStoreError: string | null;
};

export type ManifestContributes = {
  commands?: unknown[];
  services?: unknown[];
  extensionPoints?: unknown[];
  contributions?: unknown[];
  resources?: unknown[];
  webviews?: unknown[];
  settings?: Record<string, unknown>;
};

export type BundleInspection = {
  manifest: {
    schemaVersion: string;
    id: string;
    name: string;
    version: string;
    author: string | null;
    bakingrlApi: string;
    runtime?: Record<string, unknown> | null;
    contributes?: ManifestContributes | null;
  };
  hashes_present: boolean;
  signature_present: boolean;
  signature_verified: boolean;
  signature_public_key: string | null;
  file_count: number;
  uncompressed_size: number;
  sha256: string;
};

export type RuntimeInfo = {
  appVersion: string;
  runtimeApiVersion: string;
  supportedRuntimeApi: string;
};

export type PreparedPackageInstall = {
  path: string;
  source: string;
  inspection: BundleInspection;
};

export type PendingInstall = PreparedPackageInstall & {
  kind: "file" | "url" | "git";
  label: string;
};

export type ConfirmRequest = {
  title: string;
  message: string;
  confirmLabel: string;
  danger?: boolean;
  run: () => void | Promise<void>;
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
  telemetry: {
    rocket_league_host: string;
    rocket_league_port: number;
    update_state_throttle_fps: number;
  };
};

export type TelemetryConnectionStatus = {
  state: "connecting" | "connected" | "disconnected" | string;
  message: string | null;
  host: string;
  port: number;
  updated_at_ms: number;
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

export type RuntimeLogEvent = {
  kind?: string;
  source?: string;
  stream?: string;
  line?: string;
};

export type SidecarRuntimeStatusEvent = {
  packageId: string;
  sidecarId: string;
  sidecarRef: string;
  status: SidecarRuntimeStatus;
};

export type ExtensionHostRuntimeStatusEvent = {
  packageId: string;
  status: ExtensionHostRuntimeStatus;
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
