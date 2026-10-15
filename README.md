# Hangar

<p align="center">
  <img src="assets/hangar_banner.png" alt="Hangar — Rust package manager for C/C++" width="100%">
</p>

Keywords: cpp · c++ · cmake · package-manager · gamedev · rust

A small, registry-free package manager for C/C++ projects, written in Rust.
Point it at a Git repository and a tag, and Hangar fetches, builds and installs
the library so CMake's `find_package()` can use it.

Built for game engines as a lightweight alternative to Conan and vcpkg: no
registry, no recipes, no global state. Everything lives in a folder you choose.

## Features

- **Git-based**: dependencies are plain Git URLs plus a tag. No registry.
- **TOML configuration**: one manifest per project, one profile per platform.
- **Central cache**: sources, builds and installs live in a folder you choose
  (for example `C:/hangar`), shared by all your projects.
- **Per-library install folders**: each library gets its own
  `include/` and `lib/`, separated by platform and build type.
- **Skips finished work**: sources are cloned once, and a library that is
  already built is never rebuilt.
- **Compiled and header-only**: CMake libraries are built
  automatically; header-only libraries are just copied.
- **CMake integration**: Hangar writes a small file that makes
  `find_package()` and imported targets work in your project.

## Quick start

### Requirements

- Rust (to build Hangar), Git and CMake on your `PATH`
- A C/C++ toolchain (currently tested with Visual Studio 2022 on Windows)

### Install

```
cargo install --path .
```

### Project manifest: `hangar.toml`

```toml
[package]
name = "my_engine"
version = "0.1.0"

# Header-only: copy these folders into include/
[dependencies.glm]
git = "https://github.com/g-truc/glm.git"
tag = "1.0.1"
headers = ["glm"]

# Compiled with CMake; options are passed as -D flags
[dependencies.sdl3]
git = "https://github.com/libsdl-org/SDL.git"
tag = "release-3.4.12"
options = ["SDL_SHARED=OFF", "SDL_STATIC=ON", "SDL_TESTS=OFF"]
```

### Platform profile: `profiles/windows-msvc.toml`

```toml
[profile]
name = "windows-msvc"
os = "windows"
compiler = "msvc2022"
build_type = "Release"
generator = "Visual Studio 17 2022"
arch = "x64"
dependencies_dir = "C:/hangar"
```

### Use it from CMake

```cmake
find_program(HANGAR_EXE NAMES hangar REQUIRED)

execute_process(
  COMMAND ${HANGAR_EXE} install
          --manifest  ${CMAKE_SOURCE_DIR}/hangar.toml
          --profile   ${CMAKE_SOURCE_DIR}/profiles/windows-msvc.toml
          --cmake-out ${CMAKE_BINARY_DIR}/hangar.cmake
  RESULT_VARIABLE HANGAR_RESULT)
if(NOT HANGAR_RESULT EQUAL 0)
  message(FATAL_ERROR "hangar install failed")
endif()
include(${CMAKE_BINARY_DIR}/hangar.cmake)

find_package(SDL3 CONFIG REQUIRED)

add_executable(my_engine src/main.cpp)
target_link_libraries(my_engine PRIVATE hangar::glm SDL3::SDL3)
```

The first configure builds the libraries. Later ones finish in about a second.
The `examples/sample_project` folder contains a complete working project.

## How it works

```
C:/hangar/
├── sources/                          # Git clones, shared by all profiles
│   └── sdl3/release-3.4.12/
├── builds/
│   └── windows-x64-msvc2022-release/ # one folder per platform + build type
│       └── sdl3/release-3.4.12/
└── exports/
    └── windows-x64-msvc2022-release/
        ├── glm/1.0.1/include/
        └── sdl3/release-3.4.12/{include, lib, ...}
```

1. Reads `hangar.toml` and the profile.
2. Clones each dependency at its tag into `sources/`.
3. Header-only dependencies are copied; others are configured, built and
   installed with CMake into their own folder in `exports/`.
4. Writes a CMake file that sets `CMAKE_PREFIX_PATH` for compiled libraries and
   defines `hangar::<name>` targets for header-only ones.

## Current limitations

Hangar is early software.

- Only tested on Windows with MSVC, although nothing in it is Windows-specific.
- Compiled libraries must support `cmake --install` and `find_package()`.
- No transitive dependencies: list everything your project needs.
- Changing a dependency's `options` doesn't trigger a rebuild yet. Delete its
  folder in `exports/` to force one.
- No lockfile yet. A moved tag changes what you get.

## Roadmap

- [ ] `--force` flag and a lockfile
- [ ] Unit tests and CI
- [ ] Linux and macOS profiles
- [ ] Transitive dependencies

## License

MIT, see [LICENSE](LICENSE).

Inspired by [Shipp](https://github.com/linkdd/shipp), a registry-free package
manager for C/C++ that uses JSON and build scripts.