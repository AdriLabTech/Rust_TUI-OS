# TUI-OS

**Un sistema operativo escrito 100 % en Rust, con un escritorio de texto (TUI)
dibujado directamente sobre el buffer VGA 80×25.**

```
 _____ _   _ ___       ___  ____
|_   _| | | |_ _|     / _ \/ ___|   TUI-OS v0.1.0  ░  a 100% Rust OS
  | | | | | || |_____| | | \___ \   ratatui 0.30 -> VGA text 80x25
  | | | |_| || |_____| |_| |___) |   amd64 · kernel + bootloader + desktop
  |_|  \___/|___|     \___/|____/
```

## Estado del proyecto

El escritorio arranca, dibuja su dock, despacha aplicaciones y siembra el disco
solo. **La app Archivos es un andamiaje.** Esto es lo que hay y lo que no:

| Tarea | Estado |
| --- | --- |
| 1. `make test` compila y ejecuta las pruebas del kernel en QEMU | Hecha |
| 2. Framework de escritorio: barras, fondo, despacho de apps, enrutado de teclas | Hecha |
| 3. Dock dibujado en la parte inferior | Hecha |
| 4. El disco se formatea y se siembra en el primer arranque | Hecha |
| 5. Terminal con el conjunto completo de comandos | Hecha |
| 6. App Archivos de tres paneles | Pendiente. Solo responde a `q` |
| 7. Sparkline de CPU y verificación integral | Pendiente |

Las 103 pruebas del kernel y las 21 de la herramienta de volcado pasan en
release y en debug, sin warnings. El plan de
trabajo completo está en
[`os-tui/docs/superpowers/plans/2026-09-24-tuios-desktop.md`](os-tui/docs/superpowers/plans/2026-09-24-tuios-desktop.md).

Al encender aterrizas en la terminal, a pantalla completa entre la barra de
título y la de estado. Las teclas `F1` a `F5` abren una aplicación desde
cualquier sitio, y `Esc` o `F5` cierran la que tengas abierta.

El dock es la fila 23: `▸ Archivos  Terminal  Sistema  Ayuda  ░  Apagar`, con
`◀` y `▶` para moverse, `Intro` para abrir y `Esc` para volver. El `▸` marca la
entrada resaltada.

## La terminal

Es la pieza central del sistema: un shell completo que vive dentro del kernel,
con la misma disposición de teclado, historial y autocompletado que un shell de
Unix, hablando con el sistema de ficheros real en lugar de con uno de mentira.

![Terminal del sistema con comandos ejecutados](docs/comandos.png)

Los nombres siguen las convenciones habituales de Unix, con alias en español.

| Categoría | Comandos |
| --- | --- |
| Ficheros | `pwd` `cd` `ls` `cat` `touch` `mkdir` `rm` `mv` `cp` |
| Aplicaciones | `help` `ayuda` `apps` `abrir` `sysinfo` |
| Sistema | `mem` `uptime` `date` `version` `echo` `clear` `random` `pci` `halt` `apagar` `reboot` `reiniciar` |

`abrir` acepta el nombre del dock (`abrir Sistema`) y salta a esa aplicación.

En la línea de comandos tienes historial con `↑` y `↓`, edición con `←`, `→`,
`Inicio` y `Fin`, `Supr` y `Retroceso`, y `Tab` completa los nombres de comando
que empiezan por lo que ya has escrito (mientras no hayas puesto un espacio).

Los comandos de ficheros no son de juguete: `cat` lee con el recorrido real de
MFS, `cp` copia en trozos a través de `FileIO` para no corromper binarios, y
`mv` mueve dentro del mismo disco. `cd` mueve el directorio de trabajo del
proceso, `rm` borra ficheros, y `pci` lista el hardware leyendo los campos de
`DeviceConfig`.

Un fallo nunca entra en pánico: cada comando que no puede hacer lo que le piden
escribe un mensaje en español en el terminal.

### Lo que el shell todavía no hace

- **No hay redirección.** `>` no existe, así que no hay forma de meter texto en
  un fichero desde la terminal. `touch` crea el fichero vacío y `cp` copia, pero
  no hay comando que escriba contenido. Es lo primero que debería venir.
- **`..` no funciona.** MFS resuelve una ruta recorriendo entradas de
  directorio, y no tiene entradas `.` ni `..`, así que `cd ..` y
  `cat ../notas.txt` fallan. Las rutas relativas sí: dentro de `/home`,
  `mkdir relativo` crea `/home/relativo`.
- **`rm` rechaza los directorios.** No hay `rm -r`. El mensaje apunta al
  explorador de archivos.
- **No hay canalización.** Ni `|`, ni `>`, ni `&&`.

## Capturas

### Monitor del sistema

![Aplicación Sistema con medidor de memoria y actividad de CPU](docs/gestion_memoria.png)

## ¿Qué es TUI-OS?

TUI-OS es un sistema operativo autocontenido en el que **todo vive dentro del
kernel**: no hay espacio de usuario.

Al arrancar levanta el teclado PS/2, el reloj RTC, un puerto serie y un
escritorio TUI que dibuja con ratatui 0.30 directamente en el buffer de texto
VGA 80×25 (`0xB8000`) por medio de un `VgaBackend` propio. No hay ventanas: hay
un dock y aplicaciones a pantalla completa.

### Características

- **100 % Rust.** Kernel, bootloader y escritorio en un solo crate.
- **Sin userspace.** Todo el sistema corre en el kernel.
- **Interfaz en español.** Toda la UI del sistema está en español.
- **Arranca en hardware real.** x86-64 con BIOS/CSM (2005-2020).
- **Pruebas dentro del kernel.** `make test` arranca la imagen real en QEMU y
  ejecuta 103 pruebas contra el frame allocator, el sistema de ficheros y el
  terminal de verdad.

## El escritorio

| App | Qué hace | Estado |
| --- | --- | --- |
| **Terminal** | Shell dentro del kernel con 26 comandos | Funciona |
| **Sistema** | CPU y RAM en vivo, con medidor de memoria | Funciona. La sparkline de actividad es la Tarea 7 |
| **Ayuda** | Lista de comandos, atajos y aplicaciones del dock | Funciona |
| **Archivos** | Administrador de tres paneles (padre / actual / info y vista previa) con crear, renombrar, borrar, copiar y mover | Andamiaje. Falta la Tarea 6 |
| **Apagar** | Apagado por ACPI | Funciona. También `halt`, `apagar`, `reboot` y `reiniciar` desde la terminal |

## El sistema de ficheros

MFS (`src/sys/fs/`) es un sistema de ficheros jerárquico con rutas absolutas y
relativas, directorio de trabajo por proceso, subdirectorios anidados, ficheros
y dispositivos. Vive en el superbloque MFS del disco ATA, en el offset 4 MB.

**En el primer arranque** el kernel busca un superbloque en los discos ATA. Si
no lo encuentra, monta el primero, lo formatea y siembra un árbol de bienvenida:

```
/bienvenida.txt   /manual.txt   /usr/README.txt
/usr   /home   /tmp   /etc
```

La siembra es idempotente: un árbol que ya tiene `bienvenida.txt` se deja como
está, así que un segundo arranque deja la imagen byte a byte idéntica.

## Requisitos

Necesitas `git`, `gcc`, `make`, `curl`, `qemu-img`, `qemu-system-x86_64`,
`python3` y un toolchain reciente de Rust **nightly** (ver
`rust-toolchain.toml`) con el subcomando `bootimage` de cargo:

    $ curl https://sh.rustup.rs -sSf | sh -s -- -y --default-toolchain none
    $ rustup show
    $ cargo install bootimage

## Compilar y ejecutar en QEMU

Todos los comandos siguientes se ejecutan desde la carpeta `os-tui/`.

Construye la imagen de disco arrancable (32 MB, se crea si no existe):

    $ make image

Ejecútala en QEMU con ventana (añade `monitor=true` para consola telnet):

    $ make qemu

En headless, con un monitor por socket unix:

    $ qemu-system-x86_64 \
        -name "TUI-OS" -m 32 -smp 2 -cpu core2duo \
        -drive file=disk.img,format=raw \
        -monitor unix:/tmp/tuios-mon.sock,server,nowait \
        -display none -serial file:/tmp/tuios-serial.log

### Leer la pantalla y escribir en el teclado

`os-tui/tools/vgatext.py` arranca QEMU en headless, espera al escritorio, envía
teclas y vuelca el buffer de texto VGA 80×25 en caracteres legibles:

    $ cd os-tui
    $ make dump KEYS="l s ret"

Sin argumentos solo vuelca la pantalla. `make dump` deja el volcado en stdout.

**`sendkey` de QEMU acepta una sola tecla por comando**, así que hay que
mandarlas de una en una:

    (monitor) sendkey a
    (monitor) sendkey r
    (monitor) sendkey c
    (monitor) sendkey h
    (monitor) sendkey i
    (monitor) sendkey ret

Nombres de tecla que funcionan: letras y dígitos sueltos, `spc` (el espacio),
`ret`, `tab`, `backspace`, `esc`, `slash`, `dot`, `minus`, `f1`-`f12`,
`up`/`down`/`left`/`right`.

Dos nombres que **no** existen y devuelven `invalid parameter`: `space`
(es `spc`) y `escape` (es `esc`).

**El espacio se llama `spc`, no `space`.** `sendkey space` responde `invalid
parameter` porque ese no es el nombre de QEMU, pero `spc` teclea un espacio
perfectamente. `make dump KEYS="..."` separa las teclas por espacios, así que
`spc` es justo el token que hay que escribir:

    $ make dump KEYS="e c h o spc h o l a ret"
    ...
    > echo hola
    hola

El kernel vacía todo el buffer de salida del 8042 dentro de una sola IRQ1 (máx.
32 bytes, esperando entre lecturas), lo que mantiene funcionando las ráfagas
rápidas de teclado tanto en QEMU como en hardware real.

### Cambiar la distribución del teclado

La distribución se compila dentro del kernel, así que hay que recompilar:

    $ make image keyboard=azerty
    $ make image keyboard=dvorak
    $ make image keyboard=qwerty   # por defecto

Un nombre que no sea uno de esos tres hace que `make` pare con un error, en
lugar de construir una imagen cuyo teclado no responde.

## Ejecutar en hardware real

Arranca en máquinas x86-64 de ~2005-2020 con **BIOS/CSM** habilitado (UEFI no
está soportado). Escribe la imagen en un USB y arranca desde él:

    $ sudo dd if=target/x86_64-tuios/release/bootimage-tuios.bin of=/dev/sdX bs=4M conv=fsync

`sdX` es tu dispositivo USB. **Comprueba bien el nombre, `dd` lo va a
sobrescribir.** En la máquina, activa el arranque Legacy/CSM y selecciona el
USB. El teclado es PS/2. La disposición se compila dentro de la imagen, así
que si no es `qwerty` hay que reconstruirla con `make image keyboard=azerty`.

## Desarrollo

    $ make test              # release, 103 pruebas
    $ make test mode=debug   # debug: activa los debug_assert, que cazan más fallos

Y las pruebas de la herramienta que lee la pantalla de vuelta:

    $ cd tools && python3 -m unittest test_vgatext

Las dos formas compilan y arrancan la imagen real en QEMU. Ejecuta la de debug
también: varios bugs de este repo solo se ven con los `debug_assert` activos,
porque en release se compilan fuera y el kernel degrada en silencio.

## Licencia

MIT. El copyright y la licencia originales se conservan en `LICENSE`.

## Documentación técnica

El README técnico del proyecto (en inglés) está en `os-tui/README.md`, junto
con `CONTRIBUTING.md` y `LICENSE`.
