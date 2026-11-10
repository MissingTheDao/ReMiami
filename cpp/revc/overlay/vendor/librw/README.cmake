Build with cmake
================

revc builds librw through rust/revc (build.rs) with -DLIBRW_PLATFORM=WGPU: the platform
draws through the Rust host on wgpu. LIBRW_PLATFORM=NULL builds the format tools only:

   mkdir build
   cd build
   cmake .. -DLIBRW_PLATFORM=NULL -DLIBRW_TOOLS=ON
   make
