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

## Novedades v1.3 — *Aware Pomodoro*

- **Modo personalizado de duración** — chips de 1 / 5 / 15 / 25 / 50
  min más un campo numérico para cualquier valor entre 1 y 180.
- **Categorías de sesión nombradas** — Deep work / Coding / Reading /
  Writing / Other o etiqueta libre. Se persisten en SQLite y el
  coach ajusta el mensaje según la categoría.
- **Detección de presencia por cámara** (opt-in, feature `presence`)
  — pausa automáticamente la sesión cuando te alejas del escritorio,
  usando un detector por luminosidad (sin grabar, sin reconocimiento
  facial). v1.3.1 añadirá YuNet ONNX para detección facial.
- **Conciencia de calendario** (feature `calendar`) — entrada manual
  de "próxima reunión" + badge en el cronómetro mostrando "X en Yh
  Zm". v1.3.1 añadirá lectura automática de Calendar (iCloud /
  Google / Exchange / Local) vía EventKit.

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

Permisos de macOS: opcional **Screen Recording** únicamente para
poder leer el título de la ventana activa (no se toma ninguna
captura). Si se rechaza, la detección sigue funcionando con el
nombre del proceso solamente.

## Requisitos

- macOS (Apple Silicon recomendado para aceleración Metal).
- Rust 1.78 o superior.
- ~1 GB de espacio para SmolLM2-1.7B (opcional, para el coach IA).
- ~70 MB adicionales para DistilBERT INT8 (opcional, para
  clasificación avanzada).

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

- `main` — línea estable.
- `v1.1.x` — serie 1.1 mantenida para retrocompatibilidad.
- `v1.2.0-dev` — línea activa de v1.2 (RC1 → RC16; este es el
  estado actual antes del merge a `main`).

El historial completo de cada release candidate vive en el journal
de implementación que acompaña al proyecto.

## Licencia

Apache-2.0 OR MIT, a elección del usuario.

---

<div align="center">
<sub>Construido con cuidado por Gabriel Ordoñez · 2026</sub>
</div>
