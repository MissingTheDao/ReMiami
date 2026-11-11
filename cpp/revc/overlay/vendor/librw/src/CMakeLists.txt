# revc (docs/RENDERER_PORT.md, stage 2 step 7d3): librw is Rust now (rust/revc/src/rw*.rs). What
# is left here are the headers the game's C++ is written against: the Rust-owned layouts and
# inline forwarders to the Rust functions (rwimpl.h). So this is an interface (header-only)
# library.
add_library(librw INTERFACE)
add_library(librw::librw ALIAS librw)

target_include_directories(librw
    INTERFACE
        $<BUILD_INTERFACE:${PROJECT_SOURCE_DIR}>
        $<INSTALL_INTERFACE:${LIBRW_INSTALL_INCLUDEDIR}>
)

target_compile_definitions(librw
    INTERFACE
        "RW_${LIBRW_PLATFORM}"
)

if(LIBRW_PLATFORM_WGPU)
    # revc: window from the host, drawing in the Rust host on wgpu.
    target_compile_definitions(librw INTERFACE LIBRW_HOST)
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
    )
endif()
