import type { GameEventFrame, RlTelemetryEventName } from "$lib/rlTelemetry";

export type VisualExportDescriptor = {
  name: string;
  entry: string;
  default_width: number;
  default_height: number;
  settings: string | null;
};

export type NamedExportDescriptor = {
  name: string;
};

export type ServiceExportDescriptor = {
  name: string;
  methods: string[];
};

export type PageExportDescriptor = {
  name: string;
  path: string;
  title: string | null;
  description: string | null;
};

export type LayoutTemplateExportDescriptor = {
  name: string;
  path: string;
  title: string | null;
  description: string | null;
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
  id: string;
  name: string;
  version: string;
  author: string | null;
  enabled: boolean;
  status: "installed" | "deleting";
  path: string;
  exports: {
    visuals: VisualExportDescriptor[];
    components: NamedExportDescriptor[];
    services: ServiceExportDescriptor[];
    connectors: NamedExportDescriptor[];
    assets: NamedExportDescriptor[];
    schemas: NamedExportDescriptor[];
    pages: PageExportDescriptor[];
    layouts: LayoutTemplateExportDescriptor[];
  };
  imports: {
    components: string[];
    services: string[];
  };
  effective_permissions: PermissionShape;
  settings: string | null;
  error: string | null;
};

export type ManifestExports = {
  visuals?: Record<string, unknown>;
  components?: Record<string, unknown>;
  services?: Record<string, unknown>;
  connectors?: Record<string, unknown>;
  assets?: Record<string, unknown>;
  schemas?: Record<string, unknown>;
  pages?: Record<string, unknown>;
  layouts?: Record<string, unknown>;
};

export type BundleInspection = {
  manifest: {
    id: string;
    name: string;
    version: string;
    author: string | null;
    exports: ManifestExports;
    imports?: PackageDescriptor["imports"];
    permissions?: PermissionShape;
  };
  hashes_present: boolean;
  signature_present: boolean;
  signature_verified: boolean;
  signature_public_key: string | null;
  file_count: number;
  uncompressed_size: number;
  sha256: string;
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
    plugin_runtime_isolation: "export" | "package";
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
    update_rate_fps: number;
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

export type DeveloperTelemetrySort = "recent" | "alpha";
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
