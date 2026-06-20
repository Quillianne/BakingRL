import { invoke as tauriInvoke } from '@tauri-apps/api/core';
import { listen as tauriListen } from '@tauri-apps/api/event';
import type { UnlistenFn } from '@tauri-apps/api/event';

export type EventCallback<T = any> = (payload: T) => void;

interface AdapterOptions {
  wsUrl?: string;
  apiUrl?: string;
}

/**
 * The BakingRL Adapter abstracts package resource URLs and host calls.
 * It detects whether it runs inside a Tauri runtime or in a browser embedding.
 */
export class BakingRLAdapter {
  public readonly isTauri: boolean;
  private ws: WebSocket | null = null;
  private wsUrl: string;
  private apiUrl: string;

  // For WS listeners
  private listeners: Map<string, Set<EventCallback>> = new Map();
  private wsConnected: boolean = false;

  constructor(options?: AdapterOptions) {
    this.isTauri = typeof window !== 'undefined' &&
      (window.hasOwnProperty('__TAURI_INTERNALS__') || window.hasOwnProperty('__TAURI__'));

    const host = typeof window !== 'undefined' ? window.location.hostname || 'localhost' : 'localhost';
    const port = typeof window !== 'undefined' && window.location.port ? window.location.port : '8080';
    const protocol = typeof window !== 'undefined' && window.location.protocol === 'https:' ? 'https' : 'http';
    const wsProtocol = protocol === 'https' ? 'wss' : 'ws';

    this.wsUrl = options?.wsUrl || `${wsProtocol}://${host}:${port}/ws`;
    this.apiUrl = options?.apiUrl || `${protocol}://${host}:${port}/api`;

    if (!this.isTauri && typeof window !== 'undefined') {
      this.initWebSocket();
    } else if (this.isTauri) {
      console.log('[BakingRLAdapter] Running in Tauri IPC mode.');
    }
  }

  public packageFileUrl(packageId: string, path: string): string {
    const { encodedPackageId, encodedPath } = this.encodePackageFilePath(packageId, path);
    return `${this.apiUrl}/packages/${encodedPackageId}/files/${encodedPath}`;
  }

  public packageModuleUrl(packageId: string, path: string, version: string | number): string {
    const { encodedPackageId, encodedPath } = this.encodePackageFilePath(packageId, path);
    return this.withQueryParam(`${this.apiUrl}/packages/${encodedPackageId}/files/${encodedPath}`, 'v', `v${version}`);
  }

  private encodePackageFilePath(packageId: string, path: string): { encodedPackageId: string; encodedPath: string } {
    return {
      encodedPackageId: encodeURIComponent(packageId),
      encodedPath: path
        .split('/')
        .filter(Boolean)
        .map((segment) => encodeURIComponent(segment))
        .join('/')
    };
  }

  private withQueryParam(url: string, key: string, value: string | number): string {
    const separator = url.includes('?') ? '&' : '?';
    return `${url}${separator}${encodeURIComponent(key)}=${encodeURIComponent(String(value))}`;
  }

  private initWebSocket() {
    console.log(`[BakingRLAdapter] Initializing WebSocket to ${this.wsUrl}`);
    this.ws = new WebSocket(this.wsUrl);

    this.ws.onopen = () => {
      console.log('[BakingRLAdapter] WebSocket connected');
      this.wsConnected = true;
    };

    this.ws.onmessage = (event) => {
      try {
        const data = JSON.parse(event.data);
        const eventName = data.Event || data.event;
        const payload = data.Data || data.data;

        if (eventName && this.listeners.has(eventName)) {
          this.listeners.get(eventName)?.forEach(cb => cb(payload));
        }

        if (this.listeners.has('bakingrl-telemetry')) {
          this.listeners.get('bakingrl-telemetry')?.forEach(cb => cb(data));
        }
      } catch (err) {
        console.error('[BakingRLAdapter] Failed to parse WS message:', err);
      }
    };

    this.ws.onclose = () => {
      console.log('[BakingRLAdapter] WebSocket disconnected, reconnecting in 5s...');
      this.wsConnected = false;
      setTimeout(() => this.initWebSocket(), 5000);
    };

    this.ws.onerror = (err) => {
      console.error('[BakingRLAdapter] WebSocket error:', err);
    };
  }

  /**
   * Subscribe to an event from the BakingRL Event Bus.
   */
  public async listen<T>(eventName: string, handler: EventCallback<T>): Promise<UnlistenFn> {
    if (this.isTauri) {
      const unlisten = await tauriListen<T>(eventName, (event) => {
        handler(event.payload);
      });
      return unlisten;
    } else {
      if (!this.listeners.has(eventName)) {
        this.listeners.set(eventName, new Set());
      }
      this.listeners.get(eventName)!.add(handler);

      const unlisten = () => {
        const eventListeners = this.listeners.get(eventName);
        if (eventListeners) {
          eventListeners.delete(handler);
          if (eventListeners.size === 0) {
            this.listeners.delete(eventName);
          }
        }
      };

      return unlisten;
    }
  }

  /**
   * Invoke a backend command.
   */
  public async invoke<T>(cmd: string, args?: Record<string, any>): Promise<T> {
    if (this.isTauri) {
      return tauriInvoke<T>(cmd, args);
    }
    return this.invokeGateway<T>(cmd, args);
  }

  public async invokeGateway<T>(cmd: string, args?: Record<string, any>): Promise<T> {
    let url = this.apiUrl;
    let method = 'GET';
    let body: any = undefined;

    switch (cmd) {
      case 'list_packages':
        url += '/plugins';
        break;
      case 'call_service_export':
        url += `/packages/${encodeURIComponent(args?.callerPackageId ?? '')}/services/call`;
        method = 'POST';
        body = {
          serviceRef: args?.serviceRef,
          method: args?.method,
          input: args?.input ?? null
        };
        break;
      case 'get_package_settings':
        url += `/packages/${encodeURIComponent(args?.packageId ?? '')}/settings`;
        break;
      case 'plugin_registry_get':
        url += `/packages/${encodeURIComponent(args?.packageId ?? '')}/registry/${encodeURIComponent(args?.key ?? '')}`;
        break;
      case 'registry_get':
        url += `/registry/${encodeURIComponent(args?.key ?? '')}`;
        break;
      default:
        throw new Error(`[BakingRLAdapter] Command '${cmd}' is not supported in HTTP mode.`);
    }

    const response = await fetch(url, {
      method,
      headers: body ? { 'Content-Type': 'application/json' } : undefined,
      body: body ? JSON.stringify(body) : undefined,
    });

    if (!response.ok) {
      throw new Error(`HTTP Error ${response.status}: ${await response.text()}`);
    }

    const contentType = response.headers.get('content-type');
    if (contentType && contentType.includes('application/json')) {
      return response.json() as Promise<T>;
    } else {
      return response.text() as unknown as Promise<T>;
    }
  }
}

// Export a singleton instance for ease of use across the application
export const adapter = new BakingRLAdapter();
