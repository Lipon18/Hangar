#include <iostream>

#include <glm/glm.hpp>
#include <SDL3/SDL.h>
#include <box2d/box2d.h>

int main() {
    // glm: header-only, so this only needs the include path
    glm::vec3 v(3.0f, 4.0f, 0.0f);
    std::cout << "glm: length(3,4,0) = " << glm::length(v) << "\n";

    // SDL3: static library, reports the version it was built from
    int ver = SDL_GetVersion();
    std::cout << "SDL: "
              << SDL_VERSIONNUM_MAJOR(ver) << "."
              << SDL_VERSIONNUM_MINOR(ver) << "."
              << SDL_VERSIONNUM_MICRO(ver) << "\n";

    // Box2D: create and destroy an empty physics world
    b2WorldDef worldDef = b2DefaultWorldDef();
    b2WorldId world = b2CreateWorld(&worldDef);
    std::cout << "box2d: world created\n";
    b2DestroyWorld(world);

    return 0;
}