<div align="center">

# SolarFocus OS

**Pomodoro de escritorio con coach de IA que vive en tu equipo.**
Privacidad por diseño — sin nube, sin telemetría, sin cuenta.

[![estado](https://img.shields.io/badge/estado-v2.0.0--dev-brightgreen)]()
[![rust](https://img.shields.io/badge/rust-1.78%2B-orange)]()
[![iced](https://img.shields.io/badge/iced-0.13-blueviolet)]()
[![plataforma](https://img.shields.io/badge/macOS%20%2B%20Windows-supported-black)]()
[![licencia](https://img.shields.io/badge/licencia-Apache--2.0%20%2F%20MIT-blue)]()

</div>

---

## ¿Qué es SolarFocus OS?

SolarFocus OS es una aplicación de escritorio para trabajar con la
técnica Pomodoro, pensada para personas que quieren un coach que las
acompañe sin enviar nada a la nube. Todo —el cronómetro, la detección
de distracciones, el modelo de lenguaje y la base de datos— corre en
tu máquina.

Está construida en Rust con [iced 0.13](https://iced.rs/) y optimizada
para Apple Silicon, con aceleración Metal opcional para el modelo de
lenguaje.

## Novedades

**v2.0.0** — Windows port (DRAFT en branch). macOS sigue siendo el
target primario. Live Calendar queda macOS-only hasta v2.1.

**v1.13.0** — Calibración guiada con wizard data-driven (10
frames/etapa + análisis estadístico) y cooldown del coach IA tras
feedback negativo.

**v1.12.x** — Help + Acerca redesign, dashboard analytics
(distribución horaria, origen de semillas, logros), responsive
layout, fix de URL para descarga de YOLOv8n.

**v1.9.0–v1.11.0** — Sistema de semillas 🌱 con jardín solarpunk,
modo estudio profundo encadenado, detector de celular YOLOv8n.

**v1.6.0–v1.8.0** — Refactor modular (`main.rs` 5,783→622 líneas),
export JSON/CSV, validez de sesión por umbral.

Histórico completo en `solarfocus/SolarFocus OS — Implementation
Journal.md`.

## Características

- **Cronómetro Pomodoro configurable** — duraciones de foco, pausa
  corta y pausa larga ajustables (chips de 1 / 5 / 15 / 25 / 50 min,
  etc.). Pausa larga automática cada 4 sesiones.
- **Coach de IA local** — banco curado de ~50 mensajes en español e
  inglés, complementado con SmolLM2-1.7B (GGUF Q4_K_M, ~1 GB)
  ejecutado vía `llama-cpp-2` con aceleración Metal opcional. Un
  validador `looks_coherent` rechaza salidas confusas y cae al
  banco curado, garantizando que el usuario nunca vea texto roto.
- **Detección de distracciones transparente** — cada 10 segundos lee
  la ventana activa del sistema operativo (proceso + título, vía
  `NSWorkspace`), compara contra una lista local de palabras clave
  (TikTok, instagram.com, youtube.com/watch, …) y aplica una
  compuerta de 2 muestras × ≥70 % de confianza antes de marcar
  drift. **No** toma capturas, **no** lee contenido de páginas,
  **no** hace red, **no** persiste títulos.
- **Clasificador opcional DistilBERT** — modelo INT8 vía
  `ort 2.0.0-rc.12` para clasificación más fina cuando las reglas
  por palabras clave no bastan.
- **Resumen diario** — al cambiar de fecha local, el `Summarizer`
  recoge todas las sesiones del día anterior y produce un resumen
  *grounded* en datos reales (no inventa cifras).
- **Persistencia local en SQLite** — sesiones, resúmenes y feedback
  del coach se guardan en
  `~/Library/Application Support/SolarFocus OS/solarfocus.db`.
- **Asistente de primera ejecución** — wizard guiado para elegir
  idioma, duraciones, modo de RAM y descargar el modelo.
- **Atajos de teclado** — `Esc` termina la sesión sin guardar fila
  completa.
- **Iconografía dibujada** — todos los íconos se renderizan con
  `iced::widget::canvas` (formas geométricas), no con emoji. Esto
  evita problemas de fuentes con cosmic-text.

## Privacidad

SolarFocus OS está diseñado para no enviar datos personales fuera de
tu equipo:

| Recurso | Origen | Destino |
|---|---|---|
| Modelos GGUF / ONNX | Hugging Face (descarga única bajo demanda) | Disco local |
| Sesiones, feedback, resúmenes | Generados localmente | SQLite local |
| Reglas de distracción | `rules.toml` empaquetado + overrides locales | Disco local |
| Telemetría / analytics | — | **Ninguno** |

**Permisos por OS:**

- **macOS** — Screen Recording opcional para leer título de ventana
  activa (no toma captura). Cámara opcional para presencia. Calendar
  opcional para Live Calendar.
- **Windows** — sin permiso especial para EnumWindows; cámara opcional
  para presencia (UAC standard).

**Detalle de qué se guarda en SQLite local** (`solarfocus.db`):

| Tabla | Campos | Privacidad |
|---|---|---|
| `sessions` | id, start_time, duration, state, category, is_valid | Solo metadata, sin contenido de pantalla |
| `distraction_events` | id, at, process_name, rule, confidence | `process_name` (ej. "Safari") sin título de URL |
| `summaries` | date, text, model_id | Texto generado localmente por LLM |
| `coaching_feedback` | trigger, message, rating, model_id | 👍/👎 + mensaje del coach |
| `seeds` | id, earned_at, kind, amount, session_id | Eventos de cosecha solarpunk |

**Privacy → Exportar tus datos** dumpea todo a JSON o CSV. **Privacy
→ Borrar todos los datos** elimina DB + modelos + ajustes en una
sola acción.

## Plataformas soportadas

| Característica | macOS | Windows | Linux |
|---|:---:|:---:|:---:|
| UI iced + render Metal/D3D12 | ✅ | ✅ | ⚠ smoke-only |
| Pomodoro custom + categorías | ✅ | ✅ | ✅ |
| Window watcher (active-win-pos-rs) | ✅ NSWorkspace | ✅ EnumWindows | ✅ X11/Wayland |
| Coach IA local (llama-cpp-2) | ✅ Metal | ✅ CPU/CUDA | ✅ CPU |
| DistilBERT classifier (ort) | ✅ | ✅ | ✅ |
| Notificaciones nativas (notify-rust) | ✅ Banner | ✅ Toast | ✅ libnotify |
| Reveal en explorador | ✅ Finder | ✅ Explorer | ✅ xdg-open |
| Detección de presencia (nokhwa) | ✅ AVFoundation | ✅ MediaFoundation | ⚠ V4L2 |
| Detector de celular YOLOv8n | ✅ | ✅ | ⚠ untested |
| Live Calendar EventKit (iCloud/Google/Exchange unificado) | ✅ | ❌ | ❌ |
| Live Calendar ICS (cross-platform v2.1+) | ✅ | ✅ | ✅ |
| Manual deadline | ✅ | ✅ | ✅ |
| Plugins TOML | ✅ | ✅ | ✅ |
| Sistema de semillas + jardín | ✅ | ✅ | ✅ |
| Stats dashboard + export | ✅ | ✅ | ✅ |
| Calibración guiada (v1.13) | ✅ | ✅ | ⚠ untested |

**v2.0** = macOS primario + Windows soportado. Linux es smoke-only
en CI; no hay garantía de que cada feature funcione end-to-end.

## Modelos de IA disponibles

| Modelo | Rol | Tamaño | Estado |
|---|---|---:|---|
| **Rules** (clasificador) | Compara ventana activa contra deny-list de keywords + plugins. Default. | 0 MB | ✅ siempre disponible |
| **DistilBERT INT8** (clasificador) | Clasificación semántica de texto de ventana. Capta contexto, no solo keywords. | ~63 MB | Opcional, descarga |
| **SmolLM2 1.7B** (LLM coach) | Coach IA que genera mensajes personalizados. Recomendado Apple Silicon. | ~1 GB | Default LLM |
| **Llama 3.2 1B** (LLM coach) | Alternativa de coach con tono distinto. | ~700 MB | Opcional |
| **Qwen 2.5 1.5B** (LLM coach) | Mejor multilingüe. | ~1 GB | Opcional |
| **YuNet 2023mar** (presencia) | Detecta cara en cámara para auto-pausar si te alejas. | ~337 KB | Opcional |
| **YOLOv8n COCO** (presencia) | Detecta celular en cámara (clase 67) para auto-pausar. | ~12 MB | Opcional |

Los LLMs se descargan vía `Setup → IA → Modelo IA`. Los modelos de
presencia se descargan vía `Setup → IA → Detección de presencia`.
Todos los downloads son opcionales y bajo demanda.

## Requisitos

### macOS

- macOS 12+ (Monterey o superior). Apple Silicon recomendado para
  aceleración Metal en LLM; Intel funciona en CPU.
- Rust 1.78+.
- ~1 GB para SmolLM2 (opcional). ~70 MB para DistilBERT INT8.

### Windows

- Windows 10 (build 1809+) o Windows 11.
- Rust 1.78+ con MSVC toolchain (`rustup default stable-x86_64-pc-windows-msvc`).
- Visual Studio Build Tools 2022 con "Desktop development with C++"
  para compilar `llama-cpp-2`.
- ~1 GB para SmolLM2 (opcional, sin GPU acceleration por ahora).

## Instalación y ejecución

### macOS (Apple Silicon o Intel)

```bash
git clone https://github.com/Citric88/solarfocus.git
cd solarfocus

# Compilación mínima (sin LLM ni clasificador avanzado).
cargo run

# Compilación completa con coach IA + clasificador + Metal.
cargo run --features llm,classifier,gpu-metal
```

### Windows 10/11 (v2.0+)

```powershell
git clone https://github.com/Citric88/solarfocus.git
cd solarfocus

# Compilación mínima.
cargo run --release

# Con detección de presencia (cámara + YuNet/YOLO):
cargo run --release --features presence

# Coach IA local (sin aceleración GPU; gpu-metal es macOS-only):
cargo run --release --features llm,classifier,presence
```

**Notas Windows:**
- Primera ejecución: SmartScreen puede mostrar un aviso porque el
  binario no está firmado todavía. Click *More info → Run anyway*.
  El Authenticode signing llega en v2.1.
- El detector de presencia usa Media Foundation (Win10+).
- "Calendario en vivo" es macOS-only por ahora; en Windows usa el
  campo manual de deadline. Outlook/WinRT integration en v2.1+.
- Notificaciones nativas via Toast (ya integradas).

### Cargo features

| Feature | Activa |
|---|---|
| (por defecto) | UI, cronómetro, persistencia, coach con banco curado |
| `llm` | Coach con SmolLM2 vía `llama-cpp-2` |
| `classifier` | Clasificador DistilBERT INT8 vía `ort` |
| `gpu-metal` | Aceleración Metal en Apple Silicon |
| `gpu-cuda` | Aceleración CUDA (Linux/Windows con NVIDIA) |
| `presence` | Detección de presencia con cámara (v1.3) |
| `calendar` | Conciencia de calendario / próximos deadlines (v1.3) |

## Arquitectura

```
solarfocus/
├── apps/desktop/         ← Aplicación iced (UI, infraestructura,
│   ├── src/main.rs          cableado de servicios).
│   ├── src/ui/           ← Sidebar, paleta, helpers de canvas.
│   └── src/infra/        ← Persistencia, settings, descargas,
│                            window_watch, llm_coach, onnx_classifier.
├── crates/core-domain/   ← Lógica pura del Pomodoro (sin I/O).
└── crates/intelligence/  ← Traits Coach / Summarizer / Classifier,
                            banco curado de prompts, clasificador
                            por reglas. Sin dependencias de UI.
```

Arquitectura hexagonal: los crates `core-domain` e `intelligence` no
conocen `iced` ni el sistema de archivos. La aplicación de escritorio
inyecta implementaciones concretas (`MockCoach`, `LlmCoach`,
`RulesClassifier`, `OnnxClassifier`) detrás de los traits.

## Testing

```bash
cargo test                                  # 33 tests por defecto
cargo test --features llm,classifier        # 45 tests con todas las features
```

La matriz de compilación se mantiene en cero warnings:

```bash
cargo build
cargo build --features llm
cargo build --features classifier
cargo build --features llm,classifier,gpu-metal
```

## Estructura de versiones

- `main` — línea estable, actualmente en **v1.12.2** (release pública
  con dashboard analytics + redesigned Help/About).
- `v1.13.0-dev` — calibración guiada con wizard data-driven.
  Branch sin merge pendiente review.
- `v1.13.1-dev` — pulido v1.13 (botón falso positivo en toast,
  retry selectivo, atajo `C`). Encadenado a v1.13.0-dev.
- `v2.0.0-dev` — Windows port + CI matrix multi-OS. **DRAFT
  PR #16**, sin merge.

Todos los tags v1.x permanecen inmutables. El historial completo
de cada release candidate vive en el journal de implementación
(`solarfocus/SolarFocus OS — Implementation Journal.md`).

## Troubleshooting

### macOS

- **"Sin permiso — no se puede leer la ventana activa"** → Setup
  → Privacidad → Abrir Ajustes del sistema → marca SolarFocus en
  Screen Recording. Vuelve a la app y cambia de tab; el badge
  pasa a verde automáticamente desde v1.12.2.
- **El coach IA dice "esperando descarga"** → Setup → IA →
  Modelo IA → Descargar. SmolLM2 son ~1 GB; primer arranque
  toma 1–3 min en banda ancha.
- **YuNet/YOLOv8n download falla** → verifica conexión; el URL
  apunta a HuggingFace que requiere salir a internet.

### Windows

- **SmartScreen muestra "Windows protected your PC"** → Click
  *More info → Run anyway*. Es esperado hasta que firmemos el
  binario en v2.1.
- **`cargo build` con `llm` falla con "MSVC linker not found"**
  → instala Visual Studio Build Tools 2022 con "Desktop
  development with C++" workload.
- **"Live Calendar no soportado en esta plataforma"** → en v2.0
  (sin ICS path) era esperado. v2.1+ resuelve esto: en Setup →
  General introduce la ruta absoluta a un archivo `.ics` exportado
  desde tu calendario:
  - **Outlook**: File → Save Calendar → `.ics`
  - **Google Calendar**: Settings → Export → archivo `.zip` con
    un `.ics` por calendario; descomprime y apunta SolarFocus al
    `.ics` que te interese
  - **Apple Calendar**: File → Export → `.ics`
  - **Exchange/Office 365**: subscribe URL → `.ics`
- **Cámara no detecta nada (presencia)** → Windows Privacy
  Settings → Camera → SolarFocus debe estar permitido. La
  primera ejecución dispara el prompt UAC.

### Calibración

- **Wizard dice "separación insuficiente"** → tu cámara/ángulo
  o iluminación no permite distinguir presente vs ausente.
  Reposiciona la laptop (cámara frontal a tu cara) y mejora la
  luz ambiental; reintenta el paso problemático con
  **Reintentar este paso** (v1.13.1+).
- **YOLO sugiere threshold 0.00** → significa que el modelo no
  detectó tu celular en la captura "PhoneWith". Asegúrate de
  que el celular ocupe parte significativa del cuadro y esté
  iluminado. Si persiste, el guardrail v1.13.0 rechazará el
  threshold automáticamente.

### Performance

- **App tarda 30+ s en arrancar** → el LLM se carga en
  background con `tokio::spawn_blocking`. PERF-1 (v1.2.x) ya
  garantiza primer paint <1 s; el coaching falla al banco
  curado mientras el LLM termina de cargar.
- **CPU al 100% durante una sesión** → desactiva la detección
  de presencia (es lo único con inferencia continua a 5 fps).
  Window watcher es 1 syscall cada 10 s, despreciable.

## Roadmap

| Versión | Tema | Estado |
|---|---|---|
| **v2.0.0** | Windows port + CI matrix | DRAFT PR #16 |
| **v2.0.1** | Hotfixes Windows (post live test) | TBD |
| **v2.1.0** | ICS file calendar (cross-platform) + code signing CI | branch (pre-merge) |
| **v2.2** | Auto-update (Sparkle macOS / WinSparkle Windows) | planned |
| **v2.3** | MSI installer Windows + DMG firmada macOS | planned |
| **v3.0** | Linux como plataforma soportada + bundle id migration | exploration |

Sin compromiso de fechas. Cada release sale cuando está estable;
revisión de Jesús + smoke test + CI verde son blockers
permanentes.

## Licencia

Apache-2.0 OR MIT, a elección del usuario.

---

<div align="center">
<sub>Construido con cuidado por Gabriel Ordoñez · 2026</sub>
</div>
