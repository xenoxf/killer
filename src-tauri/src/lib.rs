// Killer — entry point Tauri + Go sidecar
//
// Arquitectura del túnel IPC:
//
// ```text
// ┌─────────────┐  invoke("fs_*")   ┌──────────────┐  stdin JSON   ┌─────────────┐
// │  SolidJS    │ ─────────────────> │ Rust (Tauri) │ ────────────> │  Go Sidecar │
// │  Frontend   │ <───────────────── │  go_bridge   │ <──────────── │   (fs.SVC)  │
// └─────────────┘   JSON Response    └──────────────┘  stdout JSON  └─────────────┘
//              \__________________ TÚNEL IPC (stdio) __________________/
// ```
//
// - Rust spawnea Go al iniciar la app (`setup`).
// - Cada `#[tauri::command]` hace `bridge.call(method, params).await`.
// - Go responde por el mismo túnel; Rust lo retorna a JS.
// - Si Go no está disponible, los comandos retornan error y el frontend
//   hace fallback a `src/cache/file.ts` (mock) para seguir funcionando en web.

mod go_bridge;
use go_bridge::GoBridge;
use tauri::Manager;

// Comando de ejemplo original (se mantiene para test)
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        // Setup: spawnear Go sidecar antes de que la ventana cargue
        .setup(|app| {
            // `block_on` porque `setup` es síncrono pero GoBridge::new es async.
            // Se pasa el AppHandle para resolver `resource_dir()` (app instalada).
            let bridge = tauri::async_runtime::block_on(GoBridge::new(app.handle()));
            eprintln!(
                "[rust] GoBridge available: {} (binary: {})",
                bridge.is_available(),
                if bridge.is_available() { "ok" } else { "fallback/mock" }
            );
            app.manage(bridge);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            greet,
            go_bridge::fs_ping,
            go_bridge::fs_list_dir,
            go_bridge::fs_read_file,
            go_bridge::fs_stat,
            go_bridge::fs_tree,
            go_bridge::fs_write_file,
            go_bridge::fs_delete,
            go_bridge::fs_exists,
            go_bridge::fs_ensure_dir
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
