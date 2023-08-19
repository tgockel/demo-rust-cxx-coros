cmake_minimum_required(VERSION 3.20)
project(rust-cxx-coros VERSION 0.1)

################################################################################
# Build Dependencies                                                           #
################################################################################

include(FetchContent)

FetchContent_Declare(
    Corrosion
    GIT_REPOSITORY https://github.com/corrosion-rs/corrosion.git
    GIT_TAG v0.4.2
)
FetchContent_MakeAvailable(Corrosion)

################################################################################
# External Libraries                                                           #
################################################################################

corrosion_import_crate(MANIFEST_PATH cachers/Cargo.toml)

################################################################################
# Build                                                                        #
################################################################################

add_executable(
    the-exe
    src/main.cpp
)
set_property(TARGET the-exe PROPERTY CXX_STANDARD 23)
target_link_libraries(the-exe PUBLIC cachers)
