librw
=====

This library is supposed to be a re-implementation of RenderWare graphics,
or a good part of it anyway.

It is intended to be cross-platform in two senses:
support rendering on different platforms similar to RW;
supporting all file formats for all platforms at all times and provide
way to convert to all other platforms.

Supported file formats are DFF and TXD for PS2, D3D8, D3D9 and Xbox.
Not all pre-instanced PS2 DFFs are supported.
BSP is not supported at all.

In revc, rendering goes through the WGPU platform (drawn by the Rust host on wgpu: Vulkan,
OpenGL or DX12 underneath). The D3D/Xbox/PS2 code stays for reading those native formats.

# Uses

librw can be used for rendering [GTA](https://github.com/gtamodding/re3).

# Building

See README.cmake.
