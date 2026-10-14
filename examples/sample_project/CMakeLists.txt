cmake_minimum_required(VERSION 4.3.4)
project(SampleEngine LANGUAGES C CXX)
set(CMAKE_CXX_STANDARD 17)

find_program(HANGAR_EXE NAMES hangar Hangar REQUIRED)

execute_process(
    COMMAND ${HANGAR_EXE} install
        --manifest ${CMAKE_SOURCE_DIR}/hangar.toml
        --profile ${CMAKE_SOURCE_DIR}/profiles/windows-msvc.toml
        --cmake-out ${CMAKE_BINARY_DIR}/hangar.cmake
    RESULT_VARIABLE HANGAR_RESULT)
if(NOT HANGAR_RESULT EQUAL 0)
    message(FATAL_ERROR "hangar install failed")
endif()
include(${CMAKE_BINARY_DIR}/hangar.cmake)

# dependencies
find_package(SDL3 CONFIG REQUIRED)
find_package(box2d CONFIG REQUIRED)

# Box2D's 
if(TARGET box2d::box2d)
    set(BOX2D_TARGET box2d::box2d)
else()
    set(BOX2D_TARGET box2d)
endif()

add_executable(sample_engine src/main.cpp)
#target_include_directories(sample_engine PRIVATE ${HANGAR_INCLUDE_DIR})
target_link_libraries(sample_engine PRIVATE hangar::glm SDL3::SDL3 ${BOX2D_TARGET})