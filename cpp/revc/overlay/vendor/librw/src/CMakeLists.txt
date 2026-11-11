add_library(librw
    "${PROJECT_SOURCE_DIR}/args.h"
    "${PROJECT_SOURCE_DIR}/rw.h"

    anim.cpp
    base.cpp
    bmp.cpp
    charset.cpp
    engine.cpp
    geometry.cpp
    geoplg.cpp
    hanim.cpp
    image.cpp
    matfx.cpp
    pipeline.cpp
    plg.cpp
    png.cpp
    raster.cpp
    render.cpp
    rwanim.h
    rwengine.h
    rwerror.h
    rwobjects.h
    rwscene_ffi.h
    rwplugins_ffi.h
    rwpipeline.h
    rwplg.h
    rwplugins.h
    rwrender.h
    skin.cpp
    texture.cpp
    tga.cpp

    wgpu/rwwgpu.h
    wgpu/wgpudevice.cpp
    wgpu/wgpupipe.cpp
    wgpu/wgpuraster.cpp


)
add_library(librw::librw ALIAS librw)

target_include_directories(librw
    INTERFACE
        $<BUILD_INTERFACE:${PROJECT_SOURCE_DIR}>
        $<INSTALL_INTERFACE:${LIBRW_INSTALL_INCLUDEDIR}>
)

target_compile_definitions(librw
    PRIVATE
        "$<IF:$<CONFIG:DEBUG>,DEBUG,NDEBUG>"
    PUBLIC
        "RW_${LIBRW_PLATFORM}"
)

if(CMAKE_CXX_COMPILER_ID STREQUAL "GNU" OR CMAKE_CXX_COMPILER_ID STREQUAL "Clang")
    target_link_libraries(librw
        PRIVATE
            m
    )
endif()
if(CMAKE_CXX_COMPILER_ID STREQUAL "GNU" OR CMAKE_CXX_COMPILER_ID STREQUAL "Clang" OR CMAKE_CXX_COMPILER_ID STREQUAL "AppleClang")
    target_compile_options(librw
        PRIVATE
            "-Wall"
    )
    if (NOT LIBRW_PLATFORM_PS2)
        target_compile_options(librw
            PRIVATE
                "-Wextra"
                "-Wdouble-promotion"
                "-Wpedantic"
        )
    endif()
elseif(CMAKE_CXX_COMPILER_ID STREQUAL "MSVC")
    target_compile_options(librw
        PUBLIC
            /wd4996 /wd4244
    )
endif()

set_target_properties(librw
    PROPERTIES
        C_STANDARD 11
        C_EXTENSIONS OFF
        C_STANDARD_REQUIRED ON
        CXX_STANDARD 11
        CXX_EXTENSIONS OFF
        CXX_STANDARD_REQUIRED ON
        PREFIX ""
)

if(LIBRW_PLATFORM_WGPU)
    # revc: window from the host, drawing in the Rust host on wgpu.
    target_compile_definitions(librw PUBLIC LIBRW_HOST)
endif()

if(LIBRW_INSTALL)
    install(
        FILES
            "${PROJECT_SOURCE_DIR}/args.h"
            "${PROJECT_SOURCE_DIR}/rw.h"
        DESTINATION "${LIBRW_INSTALL_INCLUDEDIR}"
    )
    install(
        FILES
            base.err
            rwbase.h
            rwcharset.h
            rwerror.h
            rwplg.h
            rwrender.h
            rwengine.h
            rwpipeline.h
            rwobjects.h
            rwanim.h
            rwplugins.h
        DESTINATION "${LIBRW_INSTALL_INCLUDEDIR}/src"
    )

    install(
        TARGETS librw
        EXPORT librw-targets
        RUNTIME DESTINATION "${CMAKE_INSTALL_BINDIR}"
        LIBRARY DESTINATION "${CMAKE_INSTALL_LIBDIR}"
        ARCHIVE DESTINATION "${CMAKE_INSTALL_LIBDIR}"
    )
endif()
