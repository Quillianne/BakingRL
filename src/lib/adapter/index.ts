import { invoke as tauriInvoke } from '@tauri-apps/api/core';
import { listen as tauriListen } from '@tauri-apps/api/event';
import type { UnlistenFn } from '@tauri-apps/api/event';

export type EventCallback<T = any> = (payload: T) => void;

interface AdapterOptions {
    wsUrl?: string; // Defaults to the current OBS gateway origin.
    apiUrl?: string; // Defaults to the current OBS gateway origin.
    token?: string;
}

/**
 * The BakingRL Adapter abstracts the data source for visual package contributions.
 * It automatically detects if it is running inside the native Tauri in-game overlay
 * or inside an external browser source (like OBS) and routes events accordingly.
 */
export class BakingRLAdapter {
    public readonly isTauri: boolean;
    private ws: WebSocket | null = null;
    private wsUrl: string;
    private apiUrl: string;
    private gatewayToken: string;
    
    // For WS listeners
    private listeners: Map<string, Set<EventCallback>> = new Map();
    private wsConnected: boolean = false;

    constructor(options?: AdapterOptions) {
        // Tauri v2 typically injects __TAURI_INTERNALS__
        this.isTauri = typeof window !== 'undefined' && 
                       (window.hasOwnProperty('__TAURI_INTERNALS__') || window.hasOwnProperty('__TAURI__'));

        // In the browser, we use the current gateway origin if possible.
        const host = typeof window !== 'undefined' ? window.location.hostname : 'localhost';
        const port = typeof window !== 'undefined' && window.location.port ? window.location.port : '8080';
        const httpProtocol = typeof window !== 'undefined' && window.location.protocol === 'https:' ? 'https' : 'http';
        const wsProtocol = httpProtocol === 'https' ? 'wss' : 'ws';
        const pageToken = typeof window !== 'undefined'
            ? new URLSearchParams(window.location.search).get('token') ?? ''
            : '';
        
        this.wsUrl = options?.wsUrl || `${wsProtocol}://${host}:${port}/ws`;
        this.apiUrl = options?.apiUrl || `${httpProtocol}://${host}:${port}/api`;
        this.gatewayToken = options?.token ?? pageToken;
        this.wsUrl = this.withGatewayToken(this.wsUrl);
        
        if (!this.isTauri && typeof window !== 'undefined') {
            this.initWebSocket();
        } else if (this.isTauri) {
            console.log('[BakingRLAdapter] Running in Tauri IPC mode.');
        }
    }

    public configureGateway(host: string, port: number, token = "") {
        if (!this.isTauri) return;
        this.gatewayToken = token;
        this.wsUrl = `ws://${host}:${port}/ws`;
        this.apiUrl = `http://${host}:${port}/api`;
        this.wsUrl = this.withGatewayToken(this.wsUrl);
    }

    public packageFileUrl(packageId: string, path: string): string {
        const { encodedPackageId, encodedPath } = this.encodePackageFilePath(packageId, path);
        if (this.gatewayToken) {
            return `${this.apiUrl}/package-files/${encodeURIComponent(this.gatewayToken)}/${encodedPackageId}/${encodedPath}`;
        }
        return `${this.apiUrl}/packages/${encodedPackageId}/files/${encodedPath}`;
    }

    public packageHtmlUrl(packageId: string, path: string, version?: string | number): string {
        const url = this.packageFileUrl(packageId, path);
        return version === undefined ? url : this.withQueryParam(url, 'v', version);
    }

    public packageModuleUrl(packageId: string, path: string, version: string | number): string {
        const { encodedPackageId, encodedPath } = this.encodePackageFilePath(packageId, path);
        if (this.gatewayToken) {
            return `${this.apiUrl}/package-modules/${encodeURIComponent(this.gatewayToken)}/${encodeURIComponent(`v${version}`)}/${encodedPackageId}/${encodedPath}`;
        }
        return this.withQueryParam(`${this.apiUrl}/packages/${encodedPackageId}/files/${encodedPath}`, 'v', version);
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

    private withGatewayToken(url: string): string {
        if (!this.gatewayToken) return url;
        return this.withQueryParam(url, 'token', this.gatewayToken);
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
                // We expect the Rust WS server to send: { Event: string, Data: any }
                // Notice the capitalization to match Rust's GameEvent struct
                const eventName = data.Event || data.event;
                const payload = data.Data || data.data;
                
                if (eventName && this.listeners.has(eventName)) {
                    this.listeners.get(eventName)?.forEach(cb => cb(payload));
                }
                
                // Also broadcast a generic telemetry event like Tauri does
                if (this.listeners.has("bakingrl-telemetry")) {
                    this.listeners.get("bakingrl-telemetry")?.forEach(cb => cb(data));
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
     * @param eventName The name of the event (e.g., 'BallHit', 'plugin.com.example.possession.updated')
     * @param handler The callback function to receive the payload
     * @returns A function to unlisten/unsubscribe
     */
    public async listen<T>(eventName: string, handler: EventCallback<T>): Promise<UnlistenFn> {
        if (this.isTauri) {
            // Tauri environment: directly use IPC
            const unlisten = await tauriListen<T>(eventName, (event) => {
                handler(event.payload);
            });
            return unlisten;
        } else {
            // WebSocket environment: use internal event emitter logic
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
     * Maps Tauri commands to REST API endpoints when running in OBS.
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
            case 'get_overlay_layouts':
                url += '/layouts';
                break;
            case 'get_pages':
                url += '/pages';
                break;
            case 'read_visual_export_source':
                url += `/packages/${encodeURIComponent(args?.packageId ?? '')}/visuals/${encodeURIComponent(args?.exportName ?? '')}/source`;
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

        const response = await fetch(this.withGatewayToken(url), {
            method,
            headers: body ? { 'Content-Type': 'application/json' } : undefined,
            body: body ? JSON.stringify(body) : undefined,
        });

        if (!response.ok) {
            throw new Error(`HTTP Error ${response.status}: ${await response.text()}`);
        }

        // For plugin source, it might be plain text
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
