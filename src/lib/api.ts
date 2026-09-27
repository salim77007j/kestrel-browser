// Typed access to the Tauri IPC. Uses the global bridge injected by
// `withGlobalTauri` (no bundler-side Rust glue needed).

declare global {
  interface Window {
    __TAURI__: {
      core: {
        invoke: (cmd: string, args?: Record<string, unknown>) => Promise<any>;
      };
      event: {
        listen: (event: string, cb: (e: { payload: any }) => void) => Promise<() => void>;
        emit: (event: string, payload?: any) => Promise<void>;
      };
      window: {
        Window: {
          getAll: () => any[];
        };
      };
    };
  }
}

export function invoke<T = any>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  return window.__TAURI__.core.invoke(cmd, args);
}

export function listen(event: string, cb: (payload: any) => void): Promise<() => void> {
  return window.__TAURI__.event.listen(event, (e) => cb(e.payload));
}

export function emit(event: string, payload?: any): Promise<void> {
  return window.__TAURI__.event.emit(event, payload);
}

export async function windowMinimize() {
  const w = window.__TAURI__.window.Window.getAll().find((x) => x.label === "main");
  if (w) await w.minimize();
}
export async function windowToggleMaximize() {
  const w = window.__TAURI__.window.Window.getAll().find((x) => x.label === "main");
  if (w) await w.toggleMaximize();
}
export async function windowClose() {
  const w = window.__TAURI__.window.Window.getAll().find((x) => x.label === "main");
  if (w) await w.close();
}
