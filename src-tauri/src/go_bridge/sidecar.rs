//! Sidecar manager — Rust corre Go y mantiene el túnel IPC
//!
//! Arquitectura:
//! ```text
//! Solid (JS) --invoke("fs_list_dir")--> Rust (Tauri cmd) --stdin JSON--> Go Sidecar
//!                                              ^                                |
//!                                              |____________stdout JSON__________/
//! ```
//!
//! - Rust spawnea el binario Go como proceso hijo en `setup`.
//! - Mantiene `stdin` y `stdout` abiertos como pipes.
//! - Cada comando Tauri hace `bridge.call("method", params).await`.
//! - `call` es secuencial (Mutex) porque Go procesa línea a línea.
//! - stderr de Go se redirige a logs de Rust (no interfiere con el túnel).
//!
//! Ejemplo de uso interno:
//! ```rust
//! let bridge = app.state::<GoBridge>();
//! let dir: Directory = bridge.call("list_dir", json!({"path": "/src"})).await?;
//! ```
//!
//! Si el binario Go no existe (ej: `cargo build` sin `go build`), el bridge
//! queda en modo `unavailable` y los comandos Tauri retornan error amigable
//! para que el frontend haga fallback a `cache/file.ts` mock.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;
use serde_json::Value;

use super::protocol::{GoRequest, GoResponse};

/// Estado global del bridge, guardado en `tauri::Manager` via `.manage()`
pub struct GoBridge {
    /// `None` si no se pudo spawnear Go (modo fallback)
    inner: Option<GoBridgeInner>,
    /// Path del binario Go (para debugging)
    binary_path: PathBuf,
}

struct GoBridgeInner {
    #[allow(dead_code)]
    child: Mutex<Child>,
    stdin: Mutex<ChildStdin>,
    stdout: Mutex<BufReader<ChildStdout>>,
    /// Lock global para secuenciar requests (Go es single-threaded line-delimited)
    call_lock: Mutex<()>,
}

impl GoBridge {
    /// Intenta spawnear el sidecar Go. Si falla, deja `inner = None`.
    ///
    /// `app` se usa para resolver `resource_dir()` (donde Tauri instala
    /// `bundle.resources` = `binaries/killer-go` cuando la app está
    /// instalada vía .deb/AppImage). En dev, cae a los paths relativos.
    pub async fn new(app: &tauri::AppHandle) -> Self {
        let bin = Self::resolve_binary_path(app);
        eprintln!("[rust] resolving go sidecar at: {}", bin.display());

        // Intentar spawnear
        match Self::spawn_child(&bin).await {
            Ok((child, stdin, stdout)) => {
                eprintln!("[rust] go sidecar spawned PID={:?}", child.id());
                let inner = GoBridgeInner {
                    child: Mutex::new(child),
                    stdin: Mutex::new(stdin),
                    stdout: Mutex::new(BufReader::new(stdout)),
                    call_lock: Mutex::new(()),
                };
                // Opcional: spawned stderr logger task
                Self {
                    inner: Some(inner),
                    binary_path: bin,
                }
            }
            Err(e) => {
                eprintln!("[rust] failed to spawn go sidecar at {}: {} — running in mock mode", bin.display(), e);
                Self {
                    inner: None,
                    binary_path: bin,
                }
            }
        }
    }

    fn resolve_binary_path(app: &tauri::AppHandle) -> PathBuf {
        use tauri::Manager;

        // 0. `bundle.resources` (app instalada .deb/AppImage): Tauri copia
        //    `src-tauri/binaries/killer-go` a `<resource_dir>/killer-go`.
        //    ESTE es el caso que antes fallaba: el binario no estaba empaquetado
        //    y `resolve` solo miraba paths relativos al cwd -> mock siempre.
        if let Ok(resource_dir) = app.path().resource_dir() {
            for name in ["killer-go", "killer-go-x86_64-unknown-linux-gnu"] {
                let p = resource_dir.join(name);
                if p.exists() {
                    return p;
                }
            }
            // Algunos bundles lo ponen en `binaries/` dentro de resources
            for name in ["binaries/killer-go", "binaries/killer-go-x86_64-unknown-linux-gnu"] {
                let p = resource_dir.join(name);
                if p.exists() {
                    return p;
                }
            }
        }

        // 0b. Junto al ejecutable instalado (`/usr/bin/killer` -> `/usr/bin/killer-go`,
        //     o `.../resources/` al lado). Cubre layouts alternos del bundle.
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                for name in ["killer-go", "killer-go-x86_64-unknown-linux-gnu"] {
                    let p = dir.join(name);
                    if p.exists() {
                        return p;
                    }
                }
                // AppImage monta resources al lado del binario
                for sub in ["../resources/killer-go", "../lib/killer/killer-go", "../lib/killer/binaries/killer-go"] {
                    let p = dir.join(sub);
                    if p.exists() {
                        return p;
                    }
                }
            }
        }

        // Orden de búsqueda (dev):
        // 1. `src-tauri/binaries/killer-go-<triple>` (Tauri sidecar convención)
        // 2. `../go/killer-go` (dev local `go build -o /tmp/killer-go`)
        // 3. `go/killer-go` desde cwd de `cargo run`
        // 4. `/tmp/killer-go` (fallback CI)

        let candidates = [
            // Tauri espera binaries con sufijo de target, pero también soporta sin sufijo en dev
            "src-tauri/binaries/killer-go-x86_64-unknown-linux-gnu",
            "src-tauri/binaries/killer-go",
            "../go/killer-go",
            "killer/go/killer-go",
            "go/killer-go",
            "/tmp/killer-go",
            "./go/killer-go",
        ];

        // Si existe variable env KILLER_GO_BIN, úsala
        if let Ok(env_path) = std::env::var("KILLER_GO_BIN") {
            let p = PathBuf::from(env_path);
            if p.exists() {
                return p;
            }
        }

        for c in candidates {
            let p = PathBuf::from(c);
            if p.exists() {
                // Si es relativo, hacerlo absoluto desde `CARGO_MANIFEST_DIR` (src-tauri)
                if p.is_relative() {
                    // Intentar resolver respecto a la carpeta del proyecto
                    if let Ok(manifest) = std::env::var("CARGO_MANIFEST_DIR") {
                        let abs = Path::new(&manifest).join(&p);
                        if abs.exists() {
                            return abs;
                        }
                        // Probar parent de manifest (killer/)
                        let parent = Path::new(&manifest).parent().unwrap_or(Path::new("."));
                        let abs2 = parent.join(&p.strip_prefix("src-tauri/").unwrap_or(&p));
                        if abs2.exists() {
                            return abs2;
                        }
                    }
                }
                return p;
            }
        }

        // Por defecto, devolver el candidato más probable para mensaje de error
        PathBuf::from("src-tauri/binaries/killer-go")
    }

    async fn spawn_child(bin: &Path) -> Result<(Child, ChildStdin, ChildStdout), String> {
        if !bin.exists() {
            return Err(format!("binary not found at {}", bin.display()));
        }

        // Los `bundle.resources` pueden perder el bit +x al instalarse
        // (.deb/AppImage). Asegurarlo antes de spawnear.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(bin, std::fs::Permissions::from_mode(0o755));
        }

        let mut child = Command::new(bin)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("spawn failed: {}", e))?;

        let stdin = child.stdin.take().ok_or("failed to open stdin")?;
        let stdout = child.stdout.take().ok_or("failed to open stdout")?;

        // Spawnear tarea que lee stderr y lo loguea (no bloquea el túnel)
        if let Some(stderr) = child.stderr.take() {
            tokio::spawn(async move {
                let mut reader = BufReader::new(stderr);
                let mut line = String::new();
                loop {
                    line.clear();
                    match reader.read_line(&mut line).await {
                        Ok(0) => break, // EOF
                        Ok(_) => eprint!("[go-stderr] {}", line),
                        Err(e) => {
                            eprintln!("[rust] stderr read error: {}", e);
                            break;
                        }
                    }
                }
            });
        }

        Ok((child, stdin, stdout))
    }

    /// ¿Está disponible el sidecar Go?
    pub fn is_available(&self) -> bool {
        self.inner.is_some()
    }

    /// Llamada genérica al sidecar. Secuencial y con timeout implícito.
    ///
    /// ```rust
    /// let v: Value = bridge.call("list_dir", json!({"path": "/src"})).await?;
    /// ```
    pub async fn call(&self, method: &str, params: Value) -> Result<Value, String> {
        let inner = self.inner.as_ref().ok_or_else(|| {
            format!(
                "go sidecar not available (binary missing at {}) — run `go build -o src-tauri/binaries/killer-go ./go` or `KILLER_GO_BIN=/tmp/killer-go cargo run`",
                self.binary_path.display()
            )
        })?;

        // Lock global para que solo un request viaje a la vez
        let _guard = inner.call_lock.lock().await;

        let req = GoRequest::new(method, params);
        let req_id = req.id.clone();
        let line = serde_json::to_string(&req).map_err(|e| format!("serialize request: {}", e))?;

        // Escribir línea JSON + \n a stdin
        {
            let mut stdin = inner.stdin.lock().await;
            stdin
                .write_all(format!("{}\n", line).as_bytes())
                .await
                .map_err(|e| format!("write to go stdin: {}", e))?;
            stdin.flush().await.map_err(|e| format!("flush stdin: {}", e))?;
        }

        eprintln!("[rust] -> go {} {}", method, req_id);

        // Leer una línea de stdout (una Response)
        let mut stdout = inner.stdout.lock().await;
        let mut resp_line = String::new();
        stdout
            .read_line(&mut resp_line)
            .await
            .map_err(|e| format!("read from go stdout: {}", e))?;

        if resp_line.is_empty() {
            return Err("go sidecar closed stdout (EOF)".into());
        }

        eprintln!("[rust] <- go {}", resp_line.trim());

        let resp: GoResponse =
            serde_json::from_str(&resp_line).map_err(|e| format!("invalid go response JSON: {} | raw: {}", e, resp_line))?;

        if resp.id != req_id {
            eprintln!("[rust] warning: response id mismatch: expected {} got {}", req_id, resp.id);
        }

        if let Some(err) = resp.error {
            return Err(err);
        }

        resp.result.ok_or_else(|| "go returned null result".into())
    }

    /// Helper para binarios que retornan `Directory`
    #[allow(dead_code)]
    pub async fn list_dir(&self, path: &str) -> Result<Value, String> {
        self.call("list_dir", serde_json::json!({"path": path})).await
    }
    #[allow(dead_code)]
    pub async fn read_file(&self, path: &str) -> Result<Value, String> {
        self.call("read_file", serde_json::json!({"path": path})).await
    }
    #[allow(dead_code)]
    pub async fn stat(&self, path: &str) -> Result<Value, String> {
        self.call("stat", serde_json::json!({"path": path})).await
    }
    #[allow(dead_code)]
    pub async fn tree(&self, root: &str, max_depth: i32, show_hidden: bool) -> Result<Value, String> {
        self.call(
            "tree",
            serde_json::json!({"root": root, "maxDepth": max_depth, "showHidden": show_hidden}),
        )
        .await
    }
}
