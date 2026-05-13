<!-- body { color:black background-color:white } a:link{ color:#0070FF } a:visited{ color:#0070FF } --> RealmCrafter: Community Edition Documentation

# Getting Started

This page is the contributor-oriented quick start for the current RCCE source tree. It assumes you already have some familiarity with the Blitz language family and want to build, test, or inspect the engine source itself.

## Current architecture

RCCE is no longer built as a split Blitz3D/BlitzPlus project. The repository now builds through the vendored [BlitzForge](../compiler/BlitzForge) toolchain:

*   `src/Client.bb` builds the game client
*   `src/Server.bb` builds the game server
*   `src/GUE.bb` builds the Game Unified Editor
*   `src/Project Manager.bb` builds the launcher used to open and run projects
*   `src/Modules/` contains the shared include files used by those entry points

The default sample game content lives under [`data/`](../data), and the compiled outputs are written to [`bin/`](../bin) plus the root-level `Project Manager` executable.

## Clone the repository

```bash
git clone --recurse-submodules https://github.com/RydeTec/rcce2.git
cd rcce2
```

If you already cloned without submodules:

```bash
git submodule update --init --recursive
```

## Build from source

### Windows

Use the repository entrypoint scripts from the repo root:

```bat
compile.bat            :: build engine + tools
compile.bat -b         :: also rebuild BlitzForge first
publish.bat            :: produce a redistributable release
```

`compile.bat` invokes BlitzForge to build `Server.exe`, `Client.exe`, `GUE.exe`, `Project Manager.exe`, and every tool source under `src\Tools`.

### macOS (Apple Silicon) / Linux

Use the shell-script equivalents from the repo root:

```bash
./scripts/bootstrap_macos.sh    # one-time setup on macOS
./compile.sh                    # build engine + tools
./compile.sh -b                 # also rebuild BlitzForge first
./publish.sh                    # produce a redistributable release
```

macOS support is currently alpha because the underlying BlitzForge runtime is still incomplete there. Treat the macOS path as a development and feedback surface, not a production release target.

## Run tests

The tracked tests live under [`src/Tests`](../src/Tests).

*   Windows: `test.bat`
*   macOS / Linux: `./test.sh`

Both runners compile each `*.bb` file in `src/Tests` with BlitzForge test mode. They are the expected pre-commit validation surface for focused source changes.

## Launch the tools

After a successful build:

*   Open `Project Manager.exe` on Windows or `Project Manager` on macOS to launch the sample project, run the server, or run the client.
*   The main editor and support tools are emitted under [`bin/`](../bin) and [`bin/tools/`](../bin/tools).

## Useful orientation notes

*   Running the client in debug mode uses a windowed display, which makes multi-client local testing easier.
*   Server-side gameplay code often refers to abilities as spells and zones as areas.
*   If you are inspecting gameplay or content behavior, look in `data/Server Data/Scripts` alongside the engine sources in `src/`.
