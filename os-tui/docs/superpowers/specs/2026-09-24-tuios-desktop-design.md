# TUI-OS: Escritorio TUI con dock, terminal y gestor de archivos — Diseño

Fecha: 2026-09-24 · Estado: **aprobado por el usuario** (6 secciones revisadas en chat)

## Contexto

TUI-OS es un OS 100% Rust que arranca por BIOS/CSM x86-64 y dibuja un
shell ratatui directamente sobre el buffer de texto VGA 80×25 mediante el backend
`VgaBackend`. Hoy la UI es un shell con pantallas (Home, Help, SysInfo, Widgets)
y un set corto de comandos.

El objetivo aprobado: convertir la pantalla de inicio en un **escritorio TUI
profesional** con un **dock** desde el que lanzar apps a pantalla completa
(**Terminal** con un listado de comandos funcionales, **Archivos** — gestor de
archivos de tres paneles —, **Sistema** y **Ayuda**), con la UI en español.
Además, **renombrar el proyecto a TUI-OS** (eliminar la marca del proyecto
original de la interfaz y los artefactos, conservando el crédito MIT al
upstream).

## Fuera de alcance (decidido)

- **UEFI**: el arranque sigue siendo BIOS/CSM. El código limine a medio hacer
  queda como está; no se trabaja sobre él en esta iteración.
- **Ratón / imágenes**: el kernel no tiene soporte de ratón ni se renderizan
  imágenes (VGA texto). El port del gestor de archivos usa el fallback de
  metadatos.
- **Userspace / red / sonido**: no se reintroduce.

## Arquitectura (Sección 1 — aprobada)

Reorganización de `src/usr/` (Opción A: framework de apps sin trait objects):

```
src/usr/
├── mod.rs           # pub mod desktop; pub mod util; pub mod tui; pub mod apps;
├── desktop.rs       # Desktop: bucle teclado→dispatcher, dock, wallpaper, cambio de app
├── util.rs          # helpers compartidos (format_uptime, label_span, format_size)
├── tui.rs           # VgaBackend · SIN CAMBIOS
└── apps/
    ├── mod.rs       # enum AppKind { Terminal, Files, SysInfo, Help }
    │                # enum AppAction { Keep, Close }
    │                # static APPS: &[(&str name, &str desc)]   (catálogo del dock)
    ├── terminal.rs  # TerminalApp → shell actual (log+input+comandos)
    ├── files.rs     # FilesApp → gestor de archivos de tres paneles, diálogos, visor
    ├── sysinfo.rs   # SysInfoApp → pantalla sysinfo actual (gauge RAM + sparkline)
    └── help.rs      # HelpApp → ayuda de comandos + atajos + apps del dock
```

Contratos:

- Toda app implementa `fn handle_key(&mut self, k: DecodedKey) -> AppAction` y
  `fn render(&mut self, frame: &mut Frame, area: Rect)`.
- `AppAction::Close` = cerrar app y volver al desktop.
- El desktop decide el enum "app abierta"; con ninguna abierta pinta el
  wallpaper; con app abierta, la app ocupa todo menos dock/status.
- `Esc` cierra la app solo si no hay diálogo/visor propios activos (en Files,
  el primer `Esc` cierra diálogo/visor; el segundo, la app).
- F1 Ayuda · F2 Sistema · F3 Terminal · F4 Archivos · F5/Esc Escritorio.
- `shell.rs` desaparece; sus piezas van a las apps; `usr::shell::main` se
  conserva como shim hacia `usr::desktop::run`.

## Escritorio — diseño visual (Sección 2 — revisada: "Ink & Brass")

Paleta: en lugar de los 16 colores IBM, el kernel reprograma la DAC VGA con la
paleta de marca **"Ink & Brass"** (`sys::vga::palette::TUIOS_COLORS`): cálida,
de bajo contraste y aire editorial. Los índices VGA conservan su rol semántico,
así que la UI sigue hablando de `Blue`/`LightCyan`/etc. y el tema se propaga a
todas las apps (incluido el port del gestor de archivos). Todos los canales son
múltiplos de 4 para que la DAC de 6 bits reproduzca el color exacto.

| Rol (índice VGA) | RGB | Uso |
|---|---|---|
| Fondo / ink — Negro (0) | `#0C0C10` | fondo del escritorio, log, celdas |
| Paneles / carbón — Azul (1) | `#181C24` | barra de título, dock, paneles |
| Cuerpo — LightGray (7) | `#BCC4CC` | texto principal |
| Apagado — DarkGray (8) | `#3C4450` | texto secundario, bordes finos |
| Acento — LightCyan (11) | `#C8A05C` | **latón**: marca, selección, highlights |
| Éxito — LightGreen (10) | `#94AC88` | prompt, estados correctos |
| Errores — LightRed (12) | `#C06058` | solo errores y acciones destructivas |

Layout (25 filas):

- `row 0`  — barra de título: `‹ Escritorio` · nombre de app ancho restante · reloj a la derecha
- `rows 1..22` — área principal (wallpaper o app a pantalla completa)
- `row 23` — dock (1 fila): `▸ Archivos  ▸ Terminal  ▸ Sistema  ▸ Ayuda  ░  Apagar`
- `row 24` — status: `MEM …` | `UP …` | hora

Wallpaper: logotipo refinado **TUI-OS** (Blanco + latón) centrado, tagline en
gris apagado y un filete `────── · ──────`. **Sin** arte ASCII ni horizonte
`░▒▓█` (retirados por "menos retro"): superficies planas y bordes finos grises,
con el acento latón gastado en un solo lugar (la marca y la selección).

## App Terminal + comandos (Sección 3 — aprobada)

TerminalApp = shell actual (log + prompt, Tab completa, ↑/↓ historial, Ctrl+C
cancela, Ctrl+L limpia). Help de comandos en español; nombres Unix estándar.

Archivos (sobre MFS): `pwd`, `cd <ruta>`, `ls [ruta]`, `cat <archivo>`,
`touch <archivo>`, `mkdir <carpeta>`, `rm <ruta>`, `mv <o> <d>`, `cp <o> <d>`.

Sistema/apps: `help` (alias `ayuda`), `apps`, `abrir <app>`, `sysinfo`,
`mem`, `uptime`, `date`, `version`, `echo`, `clear`, `random [n]`, `pci`,
`halt` (alias `apagar`), `reboot` (alias `reiniciar`).

Se elimina el comando `widgets` (demo); la app Sistema incorpora un sparkline
de RAM en vivo reutilizando el widget del demo.

## App Archivos — gestor de archivos de tres paneles (Sección 4 — aprobada)

Port 1:1 a subcarpeta `src/usr/apps/files/` con los mismos módulos
(`fsops.rs`, `state.rs`, `input.rs`, `preview.rs`, `ui.rs`, `mod.rs`).

Adaptaciones std → kernel:

- `crossterm` → `sys::keyboard::try_pop_decoded_key()` (DecodedKey).
- `CrosstermBackend` → `VgaBackend` (mismo ratatui).
- `std::fs`/`PathBuf` → MFS (rutas `String`, `Dir::open`+`ReadDir`,
  `File::{open,read,write,close}`, `sys::fs::{delete,info}`).
- `Entry` → sin permisos ni symlinks (columnas `—` / `no`).
- `chrono`/filetime → marca de tiempo MFS + reloj del kernel.
- Imágenes (kitty) → fallback de metadatos (ya disponible para
  terminales sin kitty).
- Ratón/wheel → no aplica.
- `q`/`Esc` en modo normal → `AppAction::Close` (vuelta al escritorio).

Se conserva idéntico: tres paneles (padre/actual/info+preview), keys
(j/k, Enter/→, Backspace/←, g/G, Home/End, `.`, h), operaciones
(n/N/r/d/c/m/p con confirmaciones y `.copy1`), diálogos, visor de texto a
pantalla completa.

## Disco formateado al arrancar (Sección 5 — aprobada)

En `sys::fs::init()`, si tras el escaneo ATA no hay superbloque:

1. `format_ata(0, 0)` + `mount_ata(0, 0)`.
2. Sembrar `/bienvenida.txt`, `/manual.txt` y `/usr/README.txt`.
3. Log por serial: `TUI-OS: disco sin MFS, formateado y montado`.

En arranques siguientes el superbloque existe y solo se monta.

## Errores, tests y renombrado (Sección 6 — aprobada)

- Errores de FS → alertas/diálogos en español (nunca crashear). Comandos de
  terminal imprimen error al log. Boot continúa sin FS si el formateo falla.
- Tests: `make test` (QEMU headless, test_runner en-kernel). Se portan los
  tests puros del gestor de archivos (visor, input, diálogos) y se añaden tests
  de fsops con `format_mem`/`mount_mem`. Verificación integración con
  `make qemu` + screendump de cada pantalla antes de declarar terminado.
- **Renombrado a TUI-OS**: paquete y binario → `tuios`,
  imagen → `tuios-x86_64.img`, target → `x86_64-tuios.json`,
  variables `TUIOS_VERSION`/`TUIOS_KEYBOARD`, firma de superbloque
  `b"TUIOS FS"`, versión `0.1.0`, README reescrito (crédito MIT original),
  UI/barra de título/banner/logos con marca TUI-OS. `LICENSE`/`CONTRIBUTING`
  se conservan (atribución).

## Criterios de aceptación

1. `make image` produce la imagen y `make qemu` arranca hasta el escritorio.
2. El escritorio muestra logotipo TUI-OS, dock con 5 items, status y reloj.
3. Desde el dock se abren Terminal, Archivos, Sistema y Ayuda.
4. La Terminal ejecuta el listado funcional (ls/cat/cd/abrir/…).
5. Archivos navega/crea/renombra/copia/mueve/borra con confirmaciones.
6. `make test` pasa; no hay marca del proyecto original visible en la UI ni en
   artefactos (salvo atribución MIT en `LICENSE`).