# Hangar

<p align="center">
  <img src="assets/Hangar_banner.png" alt="Hangar — Rust package manager for C/C++" width="100%">
</p>

<p align="center">
  <a href="https://github.com/Lipon18/Hangar/actions/workflows/ci.yml"><img src="https://github.com/Lipon18/Hangar/actions/workflows/ci.yml/badge.svg" alt="Windows build"></a>
  <img src="https://img.shields.io/badge/platform-Windows-blue" alt="Platform: Windows">
  <img src="https://img.shields.io/badge/license-MIT-green" alt="License: MIT">
</p>

A small, registry-free package manager for C/C++ projects, written in Rust.
Point it at a Git repository and a tag, and Hangar fetches, builds and installs
the library so CMake's `find_package()` can use it.

Built for game engines as a lightweight alternative to Conan and vcpkg: no
registry, no recipes, no global state. Everything lives in a folder you choose.

> **Platform support:** Hangar currently supports **Windows** (Visual Studio /
> MSVC). Linux support is planned.

## Features

- **Git-based**: dependencies are plain Git URLs plus a tag. No registry.
- **TOML configuration**: one manifest per project, one profile per platform.
- **Debug and Release together**: build several build types into one install
  folder, so CMake picks the right library for each configuration.
- **Central cache**: sources, builds and installs live in a folder you choose
  (for example `C:/hangar`), shared by all your projects.
- **Skips finished work**: sources are cloned once, and a library that is
  already built is never rebuilt.
- **Compiled and header-only**: CMake libraries are built automatically;
  header-only libraries are just copied.
- **Overlays**: for libraries without a usable CMake build (Lua, SOIL, ...),
  provide your own small `CMakeLists.txt`.
- **CMake integration**: Hangar writes a small file that makes `find_package()`
  and imported targets work in your project.

## Requirements

- Windows 10 or 11
- [Rust](https://rustup.rs) 1.85 or newer (to build Hangar; the project uses the 2024 edition)
- [Git](https://git-scm.com) and [CMake](https://cmake.org) 3.20 or newer, both on your `PATH`
- Visual Studio with the "Desktop development with C++" workload
  (tested with Visual Studio 2022 and 2026)

## Install

### Download a release

Download `hangar-<version>-windows-x64.zip` from the
[Releases](https://github.com/Lipon18/Hangar/releases) page, unzip it, and put
`hangar.exe` in a folder that is on your `PATH`. No Rust is needed for this.
A `.sha256` file is published next to the zip if you want to verify the download.

### Build from source

```
git clone https://github.com/Lipon18/Hangar.git
cd Hangar
cargo install --path .
```

This builds Hangar and copies `hangar.exe` to `%USERPROFILE%\.cargo\bin`, which
the Rust installer already adds to your `PATH`. Check that it works:

```
hangar --version
```

To update later, pull the latest changes and run `cargo install --path . --force`.

## Quick start

You need two small files in your project: a **manifest** that lists the
libraries, and a **profile** that describes the platform.

### 1. Manifest: `hangar.toml`

```toml
[package]
name = "my_game"
version = "0.1.0"

# Header-only: copy this folder into include/
[dependencies.glm]
git = "https://github.com/g-truc/glm.git"
tag = "1.0.3"
headers = ["glm"]

# Compiled with CMake; options are passed as -D flags
[dependencies.SDL3]
git = "https://github.com/libsdl-org/SDL.git"
tag = "release-3.4.18"
options = ["SDL_TESTS=OFF", "SDL_EXAMPLES=OFF"]
```

### 2. Profile: `profiles/windows-msvc.toml`

```toml
[profile]
name = "windows-msvc"
os = "windows"
compiler = "msvc2022"
build_types = ["Debug", "Release"]
generator = "Visual Studio 17 2022"
arch = "x64"
dependencies_dir = "C:/hangar"
```

Use `generator = "Visual Studio 18 2026"` and `compiler = "msvc2026"` for
Visual Studio 2026.

### 3. Use it from CMake

```cmake
cmake_minimum_required(VERSION 3.20)
project(my_game LANGUAGES C CXX)

# Fetch, build and install the dependencies
find_program(HANGAR_EXE NAMES hangar REQUIRED)

execute_process(
  COMMAND "${HANGAR_EXE}" install
          --manifest  "${CMAKE_SOURCE_DIR}/hangar.toml"
          --profile   "${CMAKE_SOURCE_DIR}/profiles/windows-msvc.toml"
          --cmake-out "${CMAKE_BINARY_DIR}/hangar.cmake"
  RESULT_VARIABLE HANGAR_RESULT)
if(NOT HANGAR_RESULT EQUAL 0)
  message(FATAL_ERROR "hangar install failed")
endif()
include("${CMAKE_BINARY_DIR}/hangar.cmake")

# Re-run CMake when the manifest or profile changes
set_property(DIRECTORY APPEND PROPERTY CMAKE_CONFIGURE_DEPENDS
  "${CMAKE_SOURCE_DIR}/hangar.toml"
  "${CMAKE_SOURCE_DIR}/profiles/windows-msvc.toml")

find_package(SDL3 CONFIG REQUIRED)

add_executable(my_game src/main.cpp)
target_link_libraries(my_game PRIVATE hangar::glm SDL3::SDL3)
```

The first configure builds every library (Debug and Release), which can take a
few minutes. Later configures finish in about a second.

If a library is a shared library (like SDL3), copy its DLL next to your program:

```cmake
add_custom_command(TARGET my_game POST_BUILD
  COMMAND ${CMAKE_COMMAND} -E copy_if_different
          $<TARGET_RUNTIME_DLLS:my_game> $<TARGET_FILE_DIR:my_game>
  COMMAND_EXPAND_LISTS)
```

## Command line

```
hangar install --profile <FILE>... [--manifest <FILE>] [--cmake-out <FILE>]
```

| Option | Description |
|---|---|
| `--manifest <FILE>` | Project manifest. Default: `hangar.toml`. |
| `--profile <FILE>` | Platform profile. **Required.** Repeat it to install several profiles in one run. |
| `--cmake-out <FILE>` | Write a CMake file that sets `CMAKE_PREFIX_PATH` and defines `hangar::<name>` targets. |

## Manifest reference

Each dependency is a `[dependencies.<name>]` table.

| Key | Required | Description |
|---|---|---|
| `git` | yes | Git URL of the repository. |
| `tag` | yes | Tag or branch to clone (`v1.2.0`, `release-3.4.18`). Tags differ between projects, so check them with `git ls-remote --tags <url>`. |
| `options` | no | CMake options, written **without** `-D`: `["SDL_TESTS=OFF"]`. Hangar adds the `-D` itself. |
| `headers` | no | Makes the dependency header-only. Paths relative to the repository root. A folder is copied under its own name (`"glm"` gives `include/glm/...`), and a single file is copied into `include/`. |
| `overlay` | no | Folder with your own `CMakeLists.txt` that builds the library instead of the repository's build. Relative to the manifest file. See [Overlays](#overlays). |

`overlay` cannot be combined with `headers`. Unknown keys are rejected, so a
typo like `Overlay` gives an error instead of being ignored.

Dependencies are processed in alphabetical order, with uppercase names before
lowercase ones. A library can find the libraries that come before it, for
example `SDL3` before `SDL3_mixer`.

## Profile reference

A profile is a `[profile]` table.

| Key | Required | Description |
|---|---|---|
| `name` | yes | A label for the profile. |
| `os` | yes | For example `windows`. |
| `compiler` | yes | For example `msvc2022`. Part of the install folder name. |
| `build_types` | yes | List of `Debug`, `Release`, `RelWithDebInfo`, `MinSizeRel`. |
| `dependencies_dir` | yes | Where Hangar keeps sources, builds and installs, for example `C:/hangar`. |
| `generator` | no | CMake generator, for example `Visual Studio 17 2022`. |
| `arch` | no | Platform passed to CMake with `-A`, for example `x64`. |

### Debug and Release

Every build type listed in `build_types` is built and installed into the same
folder for each library. Debug libraries get a `d` suffix (`spdlogd.lib`), so
Debug and Release do not overwrite each other, and CMake picks the right one for
each configuration.

You can also use **one profile file per build type** and pass both:

```
hangar install --profile profiles/windows-msvc-debug.toml ^
               --profile profiles/windows-msvc-release.toml
```

Profiles installed together must have the same `os`, `arch`, `compiler` and
`dependencies_dir`, so that they share one install folder. Hangar stops with an
error if they don't.

### Custom configurations

If your project has extra Visual Studio configurations such as `Development`,
map them to a build type that Hangar installed. Set this **before**
`find_package()`:

```cmake
set(CMAKE_MAP_IMPORTED_CONFIG_DEVELOPMENT Release)
```

## Overlays

Some libraries have no CMake build, or one that doesn't install a package
config. For those, write a small `CMakeLists.txt` and point to it:

```toml
[dependencies.lua]
git = "https://github.com/lua/lua.git"
tag = "v5.3.5"
overlay = "Overlays/lua"
```

Hangar clones the repository as usual, then configures **your** folder instead.
It passes the location of the clone as `HANGAR_SOURCE_DIR`, and the clone itself
is never modified. The overlay must build the library and install it with a
package config:

```cmake
cmake_minimum_required(VERSION 3.20)
project(lua LANGUAGES C)

file(GLOB LUA_SOURCES "${HANGAR_SOURCE_DIR}/*.c")
list(FILTER LUA_SOURCES EXCLUDE REGEX "/(lua|luac)\\.c$")

add_library(lua STATIC ${LUA_SOURCES})
target_include_directories(lua PUBLIC
  "$<BUILD_INTERFACE:${HANGAR_SOURCE_DIR}>" "$<INSTALL_INTERFACE:include>")

install(FILES ${HANGAR_SOURCE_DIR}/lua.h ${HANGAR_SOURCE_DIR}/luaconf.h
              ${HANGAR_SOURCE_DIR}/lualib.h ${HANGAR_SOURCE_DIR}/lauxlib.h
        DESTINATION include)
install(TARGETS lua EXPORT luaTargets
        ARCHIVE DESTINATION lib INCLUDES DESTINATION include)
install(EXPORT luaTargets NAMESPACE lua:: FILE luaTargets.cmake
        DESTINATION lib/cmake/lua)

file(WRITE "${CMAKE_CURRENT_BINARY_DIR}/luaConfig.cmake"
  "include(\"\${CMAKE_CURRENT_LIST_DIR}/luaTargets.cmake\")\n")
install(FILES "${CMAKE_CURRENT_BINARY_DIR}/luaConfig.cmake"
        DESTINATION lib/cmake/lua)
```

Then use it like any other package:

```cmake
find_package(lua CONFIG REQUIRED)
target_link_libraries(my_game PRIVATE lua::lua)
```

The source layout depends on the repository, so check where the files are in
the clone (`C:/hangar/sources/<name>/<tag>`).

## How it works

```
C:/hangar/
├── sources/                           # Git clones, shared by all profiles
│   └── SDL3/release-3.4.18/
├── builds/
│   └── windows-x64-msvc2022/          # one folder per platform
│       └── SDL3/release-3.4.18/
│           ├── Debug/                 # CMake build folder per build type
│           └── Release/
└── exports/
    └── windows-x64-msvc2022/
        ├── glm/1.0.3/include/
        └── SDL3/release-3.4.18/       # Debug and Release installed here
            ├── include/
            ├── lib/                   # SDL3.lib and SDL3d.lib
            └── bin/                   # SDL3.dll and SDL3d.dll
```

1. Reads `hangar.toml` and the profile(s).
2. Clones each dependency at its tag into `sources/`.
3. Header-only dependencies are copied. Others are configured, built and
   installed with CMake, once per build type, into their own folder in
   `exports/`.
4. Finished work is recorded with marker files (`.hangar-<commit>-<BuildType>`),
   so a re-run only builds what is missing.
5. Writes a CMake file that sets `CMAKE_PREFIX_PATH` for compiled libraries and
   defines `hangar::<name>` targets for header-only ones.

## Troubleshooting

| Problem | Cause and fix |
|---|---|
| `git clone failed ... check url and tag` | The tag doesn't exist. Tags often have a `v` prefix. List them with `git ls-remote --tags <url>`. |
| `build_types must list at least one build type` | The profile still has the old `build_type = "Release"` key. Use `build_types = ["Release"]`. |
| `unknown field ...` | A key in the manifest or profile is misspelled. Keys are case-sensitive. |
| `find_package()` can't find a library | Compiled libraries must install a package config. Check that its install option is on (for SDL3, `SDL_INSTALL=ON`), or use an [overlay](#overlays). |
| Debug build fails with `LNK2038` / `RuntimeLibrary` mismatch | Debug libraries are missing and CMake fell back to Release ones. Put `Debug` in `build_types` (or pass a Debug profile) and run Hangar again. |
| Changed `options` or an overlay but nothing rebuilt | Hangar doesn't detect that yet. Delete the library's folder in `exports/<platform>/` and run again. |
| CMake keeps using an old `hangar.exe` | CMake caches the result of `find_program`. Run `where hangar` to check which copy runs first, and delete the `HANGAR_EXE` line from `CMakeCache.txt`. |

## Current limitations

Hangar is early software.

- Windows with MSVC only.
- Compiled libraries must support `cmake --install` and provide a package
  config, or use an overlay.
- No transitive dependencies: list everything your project needs.
- Changing a dependency's `options` or overlay doesn't trigger a rebuild yet.
- No lockfile and no commit pinning. A tag that moves changes what you get, and
  a branch name such as `master` is never updated after the first clone.

## Roadmap

- [x] Debug and Release in one install folder
- [x] Overlays for libraries without CMake support
- [x] Unit tests and CI on Windows
- [ ] Rebuild when `options` or an overlay change
- [ ] Linux profiles and CI
- [ ] Lockfile and commit pinning
- [ ] Transitive dependencies

## Development

```
cargo test
cargo build --release
```

The tests that run CMake use a small fake `cmake.cmd` script, so they only run
on Windows. The tests need Git on your `PATH`.

## License

MIT, see [LICENSE](LICENSE).

Inspired by [Shipp](https://github.com/linkdd/shipp), a registry-free package
manager for C/C++ that uses JSON and build scripts.